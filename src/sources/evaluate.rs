//! Source-set preparation and family-local evaluation facade.

use glam::DVec3;

use super::model::{
    EvalOptions, FieldFamily, PrepareError, Source, SourceKind, SourceSet, WorkLimits,
};
use super::numeric::{
    FieldContribution, FiniteLineGeometry, NumericRange, finite_line_axial_field, point_field,
    stable_norm, wire_field,
};
use super::quadrature::{self, Budget, Cell, Failure as QuadratureFailure, ResolutionGuard};

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
        let options = self.options;
        let sources = &self.sources;
        let active_sources = sources
            .iter()
            .filter(|source| family_of(source.kind) == family && strength_of(source.kind) != 0.0)
            .count();
        let epsilon = options.accuracy.epsilon;
        let family_scale = match family {
            FieldFamily::Electric => options.accuracy.e_scale,
            FieldFamily::Magnetic => options.accuracy.b_scale,
        };
        let per_source_target = if active_sources == 0 {
            0.0
        } else {
            epsilon * family_scale / active_sources as f64
        };
        if active_sources > 0 && (!per_source_target.is_finite() || per_source_target <= 0.0) {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }

        let scratch = match family {
            FieldFamily::Electric => &mut self.electric_scratch,
            FieldFamily::Magnetic => &mut self.magnetic_scratch,
        };
        scratch.reset();

        let mut sums = [CompensatedSum::default(); 3];
        let mut uncertainty = 0.0f64;
        let mut absolute_contributions = 0.0f64;
        let mut work_units = 0u64;
        let mut underflow = false;
        let mut refinement_stop: Option<StopReason> = None;

        for source in sources {
            if cancelled() {
                return FamilyResult::Unavailable(StopReason::Cancelled);
            }
            if family_of(source.kind) != family || strength_of(source.kind) == 0.0 {
                continue;
            }
            if work_units >= options.limits.work_per_family {
                return FamilyResult::Unavailable(StopReason::WorkLimit);
            }

            let contribution = match source.kind {
                SourceKind::FiniteLineCharge { charge, length } => {
                    let remaining = options.limits.work_per_family - work_units;
                    let result = line_contribution(
                        point,
                        source,
                        charge,
                        length,
                        options.softening,
                        per_source_target,
                        remaining,
                        options.limits.cells_per_family,
                        options.limits.max_depth,
                        scratch,
                        cancelled,
                    );
                    match result {
                        Ok((contribution, used, stop)) => {
                            work_units += used;
                            if let Some(reason) = stop {
                                refinement_stop.get_or_insert(reason);
                            }
                            contribution
                        }
                        Err(reason) => return FamilyResult::Unavailable(reason),
                    }
                }
                _ => {
                    work_units += 1;
                    match contribution_for(family, point, source, options.softening) {
                        Ok(Some(contribution)) => contribution,
                        Ok(None) => {
                            return FamilyResult::Unavailable(StopReason::UnresolvedFeature);
                        }
                        Err(NumericRange::Unrepresentable) => {
                            return FamilyResult::Unavailable(StopReason::NumericRange);
                        }
                    }
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
        let target = epsilon * family_scale + epsilon * norm;
        if !target.is_finite() || target <= 0.0 {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }

        let cells_high_water = scratch.high_water();
        let roundoff_limited = matches!(refinement_stop, Some(StopReason::RoundoffLimited));
        let estimate = Estimate {
            value,
            uncertainty,
            target,
            absolute_contributions,
            diagnostics: Diagnostics {
                kernel_id: KERNEL_ID,
                family,
                softening: options.softening,
                epsilon,
                family_scale,
                source_count: sources.len(),
                work_units,
                cells_high_water,
                model_regularized: true,
                roundoff_limited,
                underflow,
            },
        };
        if uncertainty <= target {
            FamilyResult::TargetMet(estimate)
        } else {
            FamilyResult::AccuracyNotMet(
                estimate,
                refinement_stop.unwrap_or(StopReason::RoundoffLimited),
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn line_contribution(
    point: DVec3,
    source: &Source,
    charge: f64,
    length: f64,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    scratch: &mut FamilyScratch,
    cancelled: &dyn Fn() -> bool,
) -> Result<(FieldContribution, u64, Option<StopReason>), StopReason> {
    let geometry =
        FiniteLineGeometry::new(point, source.center, source.axis, softening, charge, length)
            .map_err(|NumericRange::Unrepresentable| StopReason::NumericRange)?;

    if geometry.radial == 0.0 {
        let contribution =
            finite_line_axial_field(point, source.center, source.axis, softening, charge, length)
                .map_err(|NumericRange::Unrepresentable| StopReason::NumericRange)?;
        return Ok((contribution, 1, None));
    }

    let half_length = 0.5 * length;
    if !half_length.is_finite() {
        return Err(StopReason::NumericRange);
    }
    let guard = LineGuard {
        radial: geometry.radial,
        axial: geometry.axial,
        softening,
    };
    let mut kernel = |parameter: f64| geometry.sample(parameter);
    let mut budget = Budget::new(work_remaining, cells_per_family, max_depth, target);
    let seeds = [geometry.axial];
    let partition = quadrature::integrate(
        (-half_length, half_length),
        &seeds,
        &guard,
        &mut kernel,
        &mut budget,
        scratch.cells_mut(),
        cancelled,
    )
    .map_err(map_quadrature_failure)?;
    scratch.note_high_water(partition.cells_high_water);

    let world =
        geometry.radial_direction * partition.value[0] + geometry.unit_axis * partition.value[1];
    if !world.is_finite() {
        return Err(StopReason::NumericRange);
    }
    let norm =
        stable_norm(world).map_err(|NumericRange::Unrepresentable| StopReason::NumericRange)?;
    let allowance = partition.uncertainty + 64.0 * f64::EPSILON * norm;
    if !allowance.is_finite() {
        return Err(StopReason::NumericRange);
    }
    Ok((
        FieldContribution {
            value: world,
            allowance,
            underflow: partition.underflow,
        },
        partition.work_used,
        partition.stop.map(map_quadrature_failure),
    ))
}

fn map_quadrature_failure(failure: QuadratureFailure) -> StopReason {
    match failure {
        QuadratureFailure::WorkLimit => StopReason::WorkLimit,
        QuadratureFailure::StorageLimit => StopReason::StorageLimit,
        QuadratureFailure::UnresolvedFeature => StopReason::UnresolvedFeature,
        QuadratureFailure::RoundoffLimited => StopReason::RoundoffLimited,
        QuadratureFailure::NumericRange => StopReason::NumericRange,
        QuadratureFailure::Cancelled => StopReason::Cancelled,
    }
}

struct LineGuard {
    radial: f64,
    axial: f64,
    softening: f64,
}

impl ResolutionGuard for LineGuard {
    fn resolved(&self, lo: f64, hi: f64) -> bool {
        let width = hi - lo;
        if width <= 0.0 || !width.is_finite() {
            return false;
        }
        let distance = if self.axial < lo {
            lo - self.axial
        } else if self.axial > hi {
            self.axial - hi
        } else {
            0.0
        };
        let minimum_distance = self.radial.hypot(distance).hypot(self.softening);
        width <= 0.5 * minimum_distance
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
    cells: Vec<Cell>,
    high_water: usize,
}

impl FamilyScratch {
    fn new(limits: WorkLimits) -> Result<Self, PrepareError> {
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(limits.cells_per_family)
            .map_err(|_| PrepareError::AllocationFailed)?;
        Ok(Self {
            cells,
            high_water: 0,
        })
    }

    fn reset(&mut self) {
        self.cells.clear();
        self.high_water = 0;
    }

    fn cells_mut(&mut self) -> &mut Vec<Cell> {
        &mut self.cells
    }

    fn note_high_water(&mut self, count: usize) {
        self.high_water = self.high_water.max(count);
    }

    fn high_water(&self) -> usize {
        self.high_water
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
