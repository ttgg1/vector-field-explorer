//! Source-set preparation and family-local evaluation facade.

use glam::DVec3;

use super::model::{
    EvalOptions, FieldFamily, PrepareError, Source, SourceKind, SourceSet, WorkLimits,
};
use super::numeric::{FieldContribution, NumericRange, point_field, stable_norm, wire_field};

/// Kernel/version identifier published with every estimate.
pub const KERNEL_ID: &str = "plummer-v1";

/// Why a family result is unavailable or did not meet its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StopReason {
    InvalidQuery,
    Cancelled,
    WorkLimit,
    StorageLimit,
    QueryLimit,
    UnresolvedFeature,
    RoundoffLimited,
    NumericRange,
}

/// Finite family estimate with provenance and estimated uncertainty.
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    pub value: DVec3,
    pub uncertainty: f64,
    pub target: f64,
    pub absolute_contributions: f64,
    pub diagnostics: Diagnostics,
}

/// Immutable provenance for one family result.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostics {
    pub kernel_id: &'static str,
    pub family: FieldFamily,
    pub softening: f64,
    pub epsilon: f64,
    pub family_scale: f64,
    pub source_count: usize,
    pub work_units: u64,
    pub cells_high_water: usize,
    pub model_regularized: bool,
    pub roundoff_limited: bool,
    pub underflow: bool,
}

/// One family result. Unavailable carries no vector.
#[derive(Debug, Clone, PartialEq)]
pub enum FamilyResult {
    TargetMet(Estimate),
    AccuracyNotMet(Estimate, StopReason),
    Unavailable(StopReason),
}

/// Independent electric and magnetic results for one query point.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub electric: FamilyResult,
    pub magnetic: FamilyResult,
}

/// Prepared immutable sources plus reusable bounded scratch.
#[derive(Debug)]
pub struct Evaluator {
    sources: Vec<Source>,
    options: EvalOptions,
    electric_scratch: FamilyScratch,
    magnetic_scratch: FamilyScratch,
    queries: u64,
}

impl Evaluator {
    pub fn new(
        set: &SourceSet,
        options: EvalOptions,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, PrepareError> {
        if cancelled() {
            return Err(PrepareError::Cancelled);
        }
        options.validate().map_err(PrepareError::InvalidOptions)?;
        if set.len() > options.limits.max_sources {
            return Err(PrepareError::SourceLimit {
                requested: set.len(),
                max: options.limits.max_sources,
            });
        }
        set.len()
            .checked_mul(std::mem::size_of::<Source>())
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .ok_or(PrepareError::AllocationFailed)?;
        if cancelled() {
            return Err(PrepareError::Cancelled);
        }
        let mut sources = Vec::new();
        sources
            .try_reserve_exact(set.len())
            .map_err(|_| PrepareError::AllocationFailed)?;
        sources.extend_from_slice(set.sources());
        let electric_scratch = FamilyScratch::new(options.limits)?;
        let magnetic_scratch = FamilyScratch::new(options.limits)?;
        Ok(Self {
            sources,
            options,
            electric_scratch,
            magnetic_scratch,
            queries: 0,
        })
    }

    /// Evaluate one point. A cancelled query publishes no new family vector.
    pub fn evaluate(&mut self, point: DVec3, cancelled: &dyn Fn() -> bool) -> Sample {
        if self.queries >= self.options.limits.max_queries {
            return Sample {
                electric: FamilyResult::Unavailable(StopReason::QueryLimit),
                magnetic: FamilyResult::Unavailable(StopReason::QueryLimit),
            };
        }
        self.queries += 1;
        if !point.is_finite() {
            return Sample {
                electric: FamilyResult::Unavailable(StopReason::InvalidQuery),
                magnetic: FamilyResult::Unavailable(StopReason::InvalidQuery),
            };
        }
        if cancelled() {
            return Sample {
                electric: FamilyResult::Unavailable(StopReason::Cancelled),
                magnetic: FamilyResult::Unavailable(StopReason::Cancelled),
            };
        }
        self.electric_scratch.reset();
        self.magnetic_scratch.reset();
        let electric = self.evaluate_family(FieldFamily::Electric, point, cancelled);
        let magnetic = if cancelled() {
            FamilyResult::Unavailable(StopReason::Cancelled)
        } else {
            self.evaluate_family(FieldFamily::Magnetic, point, cancelled)
        };
        Sample { electric, magnetic }
    }

    fn evaluate_family(
        &mut self,
        family: FieldFamily,
        point: DVec3,
        cancelled: &dyn Fn() -> bool,
    ) -> FamilyResult {
        let mut sums = [CompensatedSum::default(); 3];
        let mut uncertainty = 0.0f64;
        let mut absolute_contributions = 0.0f64;
        let mut work_units = 0u64;
        let mut underflow = false;

        for source in &self.sources {
            if cancelled() {
                return FamilyResult::Unavailable(StopReason::Cancelled);
            }
            if family_of(source.kind) != family {
                continue;
            }
            if strength_of(source.kind) == 0.0 {
                continue;
            }
            if work_units >= self.options.limits.work_per_family {
                return FamilyResult::Unavailable(StopReason::WorkLimit);
            }
            work_units += 1;
            let contribution = match contribution_for(family, point, source, self.options.softening)
            {
                Ok(Some(contribution)) => contribution,
                Ok(None) => return FamilyResult::Unavailable(StopReason::UnresolvedFeature),
                Err(NumericRange::Unrepresentable) => {
                    return FamilyResult::Unavailable(StopReason::NumericRange);
                }
            };
            if !contribution.allowance.is_finite() {
                return FamilyResult::Unavailable(StopReason::NumericRange);
            }
            underflow |= contribution.underflow;
            uncertainty += contribution.allowance;
            let contribution_norm = match stable_norm(contribution.value) {
                Ok(norm) => norm,
                Err(NumericRange::Unrepresentable) => {
                    return FamilyResult::Unavailable(StopReason::NumericRange);
                }
            };
            absolute_contributions += contribution_norm;
            for (index, component) in contribution.value.to_array().into_iter().enumerate() {
                sums[index].add(component);
            }
            if !uncertainty.is_finite() || !absolute_contributions.is_finite() {
                return FamilyResult::Unavailable(StopReason::NumericRange);
            }
        }

        let value = DVec3::new(sums[0].total(), sums[1].total(), sums[2].total());
        if !value.is_finite() {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }
        let rounding = 64.0 * f64::EPSILON * absolute_contributions;
        if !rounding.is_finite() {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }
        uncertainty += rounding;
        if !uncertainty.is_finite() {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }
        let norm = match stable_norm(value) {
            Ok(norm) => norm,
            Err(NumericRange::Unrepresentable) => {
                return FamilyResult::Unavailable(StopReason::NumericRange);
            }
        };
        let epsilon = self.options.accuracy.epsilon;
        let family_scale = match family {
            FieldFamily::Electric => self.options.accuracy.e_scale,
            FieldFamily::Magnetic => self.options.accuracy.b_scale,
        };
        let target = epsilon * family_scale + epsilon * norm;
        if !target.is_finite() || target <= 0.0 {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }

        let cells_high_water = match family {
            FieldFamily::Electric => self.electric_scratch.high_water(),
            FieldFamily::Magnetic => self.magnetic_scratch.high_water(),
        };
        let estimate = Estimate {
            value,
            uncertainty,
            target,
            absolute_contributions,
            diagnostics: Diagnostics {
                kernel_id: KERNEL_ID,
                family,
                softening: self.options.softening,
                epsilon,
                family_scale,
                source_count: self.sources.len(),
                work_units,
                cells_high_water,
                model_regularized: true,
                roundoff_limited: false,
                underflow,
            },
        };
        if uncertainty <= target {
            FamilyResult::TargetMet(estimate)
        } else {
            FamilyResult::AccuracyNotMet(estimate, StopReason::RoundoffLimited)
        }
    }
}

fn family_of(kind: SourceKind) -> FieldFamily {
    match kind {
        SourceKind::PointCharge { .. }
        | SourceKind::FiniteLineCharge { .. }
        | SourceKind::ChargedRing { .. }
        | SourceKind::ChargedDisc { .. }
        | SourceKind::SolidSphere { .. } => FieldFamily::Electric,
        SourceKind::StraightWire { .. }
        | SourceKind::CurrentLoop { .. }
        | SourceKind::Solenoid { .. } => FieldFamily::Magnetic,
    }
}

fn strength_of(kind: SourceKind) -> f64 {
    match kind {
        SourceKind::PointCharge { charge }
        | SourceKind::FiniteLineCharge { charge, .. }
        | SourceKind::ChargedRing { charge, .. }
        | SourceKind::ChargedDisc { charge, .. }
        | SourceKind::SolidSphere { charge, .. } => charge,
        SourceKind::StraightWire { current }
        | SourceKind::CurrentLoop { current, .. }
        | SourceKind::Solenoid { current, .. } => current,
    }
}

fn contribution_for(
    family: FieldFamily,
    point: DVec3,
    source: &Source,
    softening: f64,
) -> Result<Option<FieldContribution>, NumericRange> {
    match (family, source.kind) {
        (FieldFamily::Electric, SourceKind::PointCharge { charge }) => {
            point_field(point - source.center, softening, charge).map(Some)
        }
        (FieldFamily::Magnetic, SourceKind::StraightWire { current }) => {
            wire_field(point, source.center, source.axis, softening, current).map(Some)
        }
        // Later tasks replace these branches with real geometry reductions.
        (FieldFamily::Electric, _) | (FieldFamily::Magnetic, _) => Ok(None),
    }
}

#[derive(Debug)]
struct FamilyScratch {
    cells: Vec<f64>,
}

impl FamilyScratch {
    fn new(limits: WorkLimits) -> Result<Self, PrepareError> {
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(limits.cells_per_family)
            .map_err(|_| PrepareError::AllocationFailed)?;
        Ok(Self { cells })
    }

    fn reset(&mut self) {
        self.cells.clear();
    }

    fn high_water(&self) -> usize {
        self.cells.len()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct CompensatedSum {
    value: f64,
    compensation: f64,
}

impl CompensatedSum {
    fn add(&mut self, addend: f64) {
        let next = self.value + addend;
        if self.value.abs() >= addend.abs() {
            self.compensation += (self.value - next) + addend;
        } else {
            self.compensation += (addend - next) + self.value;
        }
        self.value = next;
    }

    fn total(self) -> f64 {
        self.value + self.compensation
    }
}
