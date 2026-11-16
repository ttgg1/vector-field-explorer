//! Source-set preparation and family-local evaluation facade.

use glam::DVec3;

use super::electric;
use super::magnetic;
use super::model::{
    EvalOptions, FieldFamily, PrepareError, Source, SourceKind, SourceSet, WorkLimits,
};
use super::numeric::{FieldContribution, NumericRange, stable_norm};
use super::quadrature::{Cell, Failure as QuadratureFailure};

/// Kernel/version identifier published with every estimate.
pub const KERNEL_ID: &str = "plummer-v1";

/// Requested target used for the initial complete partition. It is finite so
/// the shared series and integration paths can accept it, but large enough
/// that no accuracy refinement happens before the family-global pass.
const COARSE_TARGET: f64 = f64::MAX / 4.0;

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
    /// Number of validated sources in this family, including zero-strength
    /// sources (which are validated and counted but need no calculation).
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

    /// Evaluate one point. Every call, including invalid queries, counts
    /// against the hard job query lifetime. A cancelled query publishes no new
    /// family vector.
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
        let electric = evaluate_family(
            FieldFamily::Electric,
            &self.sources,
            self.options,
            point,
            &mut self.electric_scratch,
            cancelled,
        );
        let magnetic = if cancelled() {
            FamilyResult::Unavailable(StopReason::Cancelled)
        } else {
            evaluate_family(
                FieldFamily::Magnetic,
                &self.sources,
                self.options,
                point,
                &mut self.magnetic_scratch,
                cancelled,
            )
        };
        Sample { electric, magnetic }
    }
}

/// Evaluate one family from complete per-source estimates refined against the
/// terminal family target `T = epsilon*S + epsilon*norm(V)`.
fn evaluate_family(
    family: FieldFamily,
    sources: &[Source],
    options: EvalOptions,
    point: DVec3,
    scratch: &mut FamilyScratch,
    cancelled: &dyn Fn() -> bool,
) -> FamilyResult {
    let epsilon = options.accuracy.epsilon;
    let family_scale = match family {
        FieldFamily::Electric => options.accuracy.e_scale,
        FieldFamily::Magnetic => options.accuracy.b_scale,
    };
    let family_count = sources
        .iter()
        .filter(|source| family_of(source.kind) == family)
        .count();
    let active_count = sources
        .iter()
        .filter(|source| family_of(source.kind) == family && strength_of(source.kind) != 0.0)
        .count();

    if active_count == 0 {
        let target = epsilon * family_scale;
        if !target.is_finite() {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }
        return FamilyResult::TargetMet(Estimate {
            value: DVec3::ZERO,
            uncertainty: 0.0,
            target,
            absolute_contributions: 0.0,
            diagnostics: Diagnostics {
                kernel_id: KERNEL_ID,
                family,
                softening: options.softening,
                epsilon,
                family_scale,
                source_count: family_count,
                work_units: 0,
                cells_high_water: 0,
                model_regularized: true,
                roundoff_limited: false,
                underflow: false,
            },
        });
    }

    let FamilyScratch {
        cells,
        estimates,
        high_water,
    } = scratch;
    let mut work_units = 0u64;
    let mut last_stop: Option<StopReason> = None;

    // Phase 1: complete, resolved coverage for every active source. No
    // accuracy refinement is requested here, so cancellation is global and a
    // failure publishes no vector.
    for (source_index, source) in sources.iter().enumerate() {
        if family_of(source.kind) != family || strength_of(source.kind) == 0.0 {
            continue;
        }
        if cancelled() {
            return FamilyResult::Unavailable(StopReason::Cancelled);
        }
        let remaining = options.limits.work_per_family.saturating_sub(work_units);
        if remaining == 0 {
            return FamilyResult::Unavailable(StopReason::WorkLimit);
        }
        match contribution_once(
            family,
            point,
            source,
            options.softening,
            COARSE_TARGET,
            remaining,
            options.limits.cells_per_family,
            options.limits.max_depth,
            cells,
            cancelled,
        ) {
            Ok((contribution, used, used_cells, stop)) => {
                work_units = work_units.saturating_add(used);
                *high_water = (*high_water).max(used_cells);
                if let Some(failure) = stop {
                    last_stop = Some(map_quadrature_failure(failure));
                }
                estimates.push(SourceEstimate {
                    source_index,
                    contribution,
                    stop,
                    exhausted: false,
                });
            }
            Err(failure) => return FamilyResult::Unavailable(map_quadrature_failure(failure)),
        }
    }
    debug_assert_eq!(estimates.len(), active_count);

    // Phase 2: an epsilon-dependent accuracy allocation from the coarse
    // complete partition. This gives tighter requests a larger refinement
    // budget without ever forcing per-source accuracy relative to the
    // uncancelled source magnitudes.
    if let Ok(initial) = aggregate(estimates)
        && let Ok(initial_norm) = stable_norm(initial.value)
    {
        let initial_target = epsilon * family_scale + epsilon * initial_norm;
        if initial_target.is_finite() && initial_target > 0.0 {
            let share = 0.5 * initial_target / active_count as f64;
            if initial.uncertainty > initial_target && share > 0.0 && share.is_finite() {
                for index in 0..estimates.len() {
                    if cancelled() {
                        return FamilyResult::Unavailable(StopReason::Cancelled);
                    }
                    if estimates[index].contribution.allowance <= share {
                        continue;
                    }
                    let remaining = options.limits.work_per_family.saturating_sub(work_units);
                    if remaining == 0 {
                        for pending in estimates.iter_mut().skip(index) {
                            pending.exhausted = true;
                            pending.stop = Some(QuadratureFailure::WorkLimit);
                        }
                        last_stop = Some(StopReason::WorkLimit);
                        break;
                    }
                    let estimate = estimates[index];
                    let source = &sources[estimate.source_index];
                    match contribution_once(
                        family,
                        point,
                        source,
                        options.softening,
                        share,
                        remaining,
                        options.limits.cells_per_family,
                        options.limits.max_depth,
                        cells,
                        cancelled,
                    ) {
                        Err(QuadratureFailure::Cancelled) => {
                            return FamilyResult::Unavailable(StopReason::Cancelled);
                        }
                        Err(QuadratureFailure::NumericRange) => {
                            return FamilyResult::Unavailable(StopReason::NumericRange);
                        }
                        Err(failure) => {
                            estimates[index].stop = Some(failure);
                            estimates[index].exhausted = true;
                            last_stop = Some(map_quadrature_failure(failure));
                        }
                        Ok((contribution, used, used_cells, stop)) => {
                            work_units = work_units.saturating_add(used);
                            *high_water = (*high_water).max(used_cells);
                            if contribution.allowance
                                < estimate.contribution.allowance * (1.0 - 1e-6)
                            {
                                estimates[index].contribution = contribution;
                                estimates[index].stop = stop;
                                if let Some(failure) = stop {
                                    last_stop = Some(map_quadrature_failure(failure));
                                }
                            } else {
                                estimates[index].exhausted = true;
                                if let Some(failure) = stop {
                                    estimates[index].stop = Some(failure);
                                    last_stop = Some(map_quadrature_failure(failure));
                                } else {
                                    estimates[index].stop =
                                        Some(QuadratureFailure::RoundoffLimited);
                                    last_stop = Some(StopReason::RoundoffLimited);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 3: refine the source with the largest absolute uncertainty until
    // the terminal compensated family criterion passes or every source has
    // reached a limit/roundoff floor. Replacing an estimate is atomic; a failed
    // refinement keeps the previous complete contribution.
    loop {
        let aggregate = match aggregate(estimates) {
            Ok(aggregate) => aggregate,
            Err(NumericRange::Unrepresentable) => {
                return FamilyResult::Unavailable(StopReason::NumericRange);
            }
        };
        let norm = match stable_norm(aggregate.value) {
            Ok(norm) => norm,
            Err(NumericRange::Unrepresentable) => {
                return FamilyResult::Unavailable(StopReason::NumericRange);
            }
        };
        let target = epsilon * family_scale + epsilon * norm;
        if !target.is_finite() || target <= 0.0 {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }
        if aggregate.uncertainty <= target {
            let estimate = make_estimate(
                family,
                options,
                family_count,
                &aggregate,
                target,
                work_units,
                *high_water,
                roundoff_limited(estimates, last_stop),
            );
            return FamilyResult::TargetMet(estimate);
        }

        let Some(index) = select_largest(estimates) else {
            let estimate = make_estimate(
                family,
                options,
                family_count,
                &aggregate,
                target,
                work_units,
                *high_water,
                roundoff_limited(estimates, last_stop),
            );
            return FamilyResult::AccuracyNotMet(estimate, quiescent_reason(estimates, last_stop));
        };

        let estimate = estimates[index];
        let other = (aggregate.uncertainty - estimate.contribution.allowance).max(0.0);
        let needed = target - other;
        let request = if needed > 0.0 {
            0.5 * needed
        } else {
            0.5 * estimate.contribution.allowance
        };
        if !request.is_finite() || request < 0.0 {
            return FamilyResult::Unavailable(StopReason::NumericRange);
        }
        let remaining = options.limits.work_per_family.saturating_sub(work_units);
        if remaining == 0 {
            estimates[index].stop = Some(QuadratureFailure::WorkLimit);
            estimates[index].exhausted = true;
            last_stop = Some(StopReason::WorkLimit);
            continue;
        }
        let source = &sources[estimate.source_index];
        match contribution_once(
            family,
            point,
            source,
            options.softening,
            request,
            remaining,
            options.limits.cells_per_family,
            options.limits.max_depth,
            cells,
            cancelled,
        ) {
            Err(QuadratureFailure::Cancelled) => {
                return FamilyResult::Unavailable(StopReason::Cancelled);
            }
            Err(QuadratureFailure::NumericRange) => {
                return FamilyResult::Unavailable(StopReason::NumericRange);
            }
            Err(failure) => {
                estimates[index].stop = Some(failure);
                estimates[index].exhausted = true;
                last_stop = Some(map_quadrature_failure(failure));
            }
            Ok((contribution, used, used_cells, stop)) => {
                work_units = work_units.saturating_add(used);
                *high_water = (*high_water).max(used_cells);
                if contribution.allowance < estimate.contribution.allowance * (1.0 - 1e-6) {
                    estimates[index].contribution = contribution;
                    estimates[index].stop = stop;
                    estimates[index].exhausted = false;
                    if let Some(failure) = stop {
                        last_stop = Some(map_quadrature_failure(failure));
                    }
                } else {
                    // No material progress on this source: retain the previous
                    // complete estimate and move on instead of re-running it.
                    estimates[index].exhausted = true;
                    if let Some(failure) = stop {
                        estimates[index].stop = Some(failure);
                        last_stop = Some(map_quadrature_failure(failure));
                    } else {
                        estimates[index].stop = Some(QuadratureFailure::RoundoffLimited);
                        last_stop = Some(StopReason::RoundoffLimited);
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn contribution_once(
    family: FieldFamily,
    point: DVec3,
    source: &Source,
    softening: f64,
    target: f64,
    work_remaining: u64,
    cells_per_family: usize,
    max_depth: u8,
    cells: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(FieldContribution, u64, usize, Option<QuadratureFailure>), QuadratureFailure> {
    match family {
        FieldFamily::Electric => electric::contribution(
            point,
            source,
            softening,
            target,
            work_remaining,
            cells_per_family,
            max_depth,
            cells,
            cancelled,
        ),
        FieldFamily::Magnetic => magnetic::contribution(
            point,
            source,
            softening,
            target,
            work_remaining,
            cells_per_family,
            max_depth,
            cells,
            cancelled,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn make_estimate(
    family: FieldFamily,
    options: EvalOptions,
    family_count: usize,
    aggregate: &Aggregate,
    target: f64,
    work_units: u64,
    cells_high_water: usize,
    roundoff_limited: bool,
) -> Estimate {
    Estimate {
        value: aggregate.value,
        uncertainty: aggregate.uncertainty,
        target,
        absolute_contributions: aggregate.absolute_contributions,
        diagnostics: Diagnostics {
            kernel_id: KERNEL_ID,
            family,
            softening: options.softening,
            epsilon: options.accuracy.epsilon,
            family_scale: match family {
                FieldFamily::Electric => options.accuracy.e_scale,
                FieldFamily::Magnetic => options.accuracy.b_scale,
            },
            source_count: family_count,
            work_units,
            cells_high_water,
            model_regularized: true,
            roundoff_limited,
            underflow: aggregate.underflow,
        },
    }
}

#[derive(Debug, Clone, Copy)]
struct SourceEstimate {
    source_index: usize,
    contribution: FieldContribution,
    stop: Option<QuadratureFailure>,
    exhausted: bool,
}

#[derive(Debug, Clone, Copy)]
struct Aggregate {
    value: DVec3,
    uncertainty: f64,
    absolute_contributions: f64,
    underflow: bool,
}

fn aggregate(estimates: &[SourceEstimate]) -> Result<Aggregate, NumericRange> {
    let mut sums = [CompensatedSum::default(); 3];
    let mut uncertainty = CompensatedSum::default();
    let mut absolute = CompensatedSum::default();
    let mut underflow = false;
    for estimate in estimates {
        for (index, component) in estimate
            .contribution
            .value
            .to_array()
            .into_iter()
            .enumerate()
        {
            sums[index].add(component);
        }
        uncertainty.add(estimate.contribution.allowance);
        absolute.add(stable_norm(estimate.contribution.value)?);
        underflow |= estimate.contribution.underflow;
    }
    let value = DVec3::new(sums[0].total(), sums[1].total(), sums[2].total());
    if !value.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    let norm = stable_norm(value)?;
    let absolute_contributions = absolute.total();
    // Compensated family summation loses only O(eps^2) of the summed
    // magnitudes; per-source transform/quadrature rounding is already in each
    // contribution's allowance. Never inflate the family allowance by
    // `eps * sum(|v_i|)`, which would defeat cancellation semantics.
    let rounding =
        64.0 * f64::EPSILON * f64::EPSILON * absolute_contributions + 8.0 * f64::EPSILON * norm;
    if !rounding.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    uncertainty.add(rounding);
    let uncertainty = uncertainty.total();
    if !uncertainty.is_finite() || !absolute_contributions.is_finite() {
        return Err(NumericRange::Unrepresentable);
    }
    Ok(Aggregate {
        value,
        uncertainty,
        absolute_contributions,
        underflow,
    })
}

fn select_largest(estimates: &[SourceEstimate]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, estimate) in estimates.iter().enumerate() {
        if estimate.exhausted || estimate.contribution.allowance <= 0.0 {
            continue;
        }
        match best {
            None => best = Some(index),
            Some(current) => {
                let incumbent = &estimates[current];
                if estimate.contribution.allowance > incumbent.contribution.allowance
                    || (estimate.contribution.allowance == incumbent.contribution.allowance
                        && estimate.source_index < incumbent.source_index)
                {
                    best = Some(index);
                }
            }
        }
    }
    best
}

fn quiescent_reason(estimates: &[SourceEstimate], last_stop: Option<StopReason>) -> StopReason {
    // Prefer the limiting reason reported by the largest remaining uncertainty;
    // otherwise use the last observed refinement stop, and finally the
    // roundoff floor.
    let mut best: Option<&SourceEstimate> = None;
    for estimate in estimates {
        match best {
            None => best = Some(estimate),
            Some(current) if estimate.contribution.allowance > current.contribution.allowance => {
                best = Some(estimate);
            }
            _ => {}
        }
    }
    best.and_then(|estimate| estimate.stop)
        .map(map_quadrature_failure)
        .or(last_stop)
        .unwrap_or(StopReason::RoundoffLimited)
}

fn roundoff_limited(estimates: &[SourceEstimate], last_stop: Option<StopReason>) -> bool {
    last_stop == Some(StopReason::RoundoffLimited)
        || estimates
            .iter()
            .any(|estimate| estimate.stop == Some(QuadratureFailure::RoundoffLimited))
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

#[derive(Debug)]
struct FamilyScratch {
    cells: Vec<Cell>,
    estimates: Vec<SourceEstimate>,
    high_water: usize,
}

impl FamilyScratch {
    fn new(limits: WorkLimits) -> Result<Self, PrepareError> {
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(limits.cells_per_family)
            .map_err(|_| PrepareError::AllocationFailed)?;
        let mut estimates = Vec::new();
        estimates
            .try_reserve_exact(limits.max_sources)
            .map_err(|_| PrepareError::AllocationFailed)?;
        Ok(Self {
            cells,
            estimates,
            high_water: 0,
        })
    }

    fn reset(&mut self) {
        self.cells.clear();
        self.estimates.clear();
        self.high_water = 0;
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
