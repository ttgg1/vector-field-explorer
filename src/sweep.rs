//! Issue #234: bounded parameter sweeps from a shipped lesson reading.
//!
//! The sweep is a pure, session-only model over the existing readings:
//! [`ideal_axial_field`](crate::ui::ring_lesson::ideal_axial_field) for the
//! Griffiths ring and [`compute_reading`](crate::ui::gauss_shell::compute_reading)
//! plus [`GaussShellReading::flux_estimate`](crate::ui::gauss_shell::GaussShellReading::flux_estimate)
//! for the charged shell. It adds no solver, quadrature rule, physics model or
//! durable probe surface. Invalid samples stay [`SweepSample`] gaps with an
//! honest reason; they are never folded to zero.
//!
//! The work is bounded by [`SWEEP_MAX_SAMPLES`] and the run advances one sample
//! per caller step, so the UI can stay responsive and cancellation keeps a
//! clearly labelled partial curve.

use std::fmt;

use crate::maxwell::MaxwellScene;
use crate::sources::FieldFamily;
use crate::ui::gauss_shell::{GaussShellReadingKind, compute_reading};
use crate::ui::ring_lesson::{RING_CHARGE, RING_RADIUS, ideal_axial_field};

/// Default sample count for a new sweep.
pub const SWEEP_DEFAULT_SAMPLES: usize = 33;
/// Smallest accepted sample count.
pub const SWEEP_MIN_SAMPLES: usize = 3;
/// Hard upper bound on the work one run can request.
pub const SWEEP_MAX_SAMPLES: usize = 129;
/// Numeric tolerance scale for the odd-symmetry residual. The stated tolerance
/// is `scale * max|E_z|` over the paired samples, so it is derived from the
/// values rather than an unrelated absolute constant.
pub const ODD_SYMMETRY_RELATIVE_TOLERANCE: f64 = 1e-12;

/// The independent variable and reading a sweep walks over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepKind {
    /// Ideal axial ring field `E_z` versus signed probe position `z`.
    RingAxialField,
    /// Sampled net shell flux `Phi` versus measuring-surface radius `r`.
    ShellFlux,
}

impl SweepKind {
    /// The label shown for the sweep's reading.
    pub const fn label(self) -> &'static str {
        match self {
            Self::RingAxialField => "Ring axial field",
            Self::ShellFlux => "Shell net flux",
        }
    }

    /// The independent-variable symbol.
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::RingAxialField => "z",
            Self::ShellFlux => "r",
        }
    }

    /// The swept value's symbol.
    pub const fn value_symbol(self) -> &'static str {
        match self {
            Self::RingAxialField => "E_z",
            Self::ShellFlux => "Phi",
        }
    }

    /// The sentence naming the independent variable.
    pub const fn independent_label(self) -> &'static str {
        match self {
            Self::RingAxialField => "z (signed axial probe position)",
            Self::ShellFlux => "r (measuring-surface radius)",
        }
    }

    /// The horizontal-axis label, including the normalized units.
    pub const fn axis_x_label(self) -> &'static str {
        match self {
            Self::RingAxialField => "z (normalized units)",
            Self::ShellFlux => "r (normalized units)",
        }
    }

    /// The vertical-axis label.
    pub const fn axis_y_label(self) -> &'static str {
        match self {
            Self::RingAxialField => "E_z (ideal, k = 1)",
            Self::ShellFlux => "Phi (sampled)",
        }
    }

    /// The model-units note shown beside the controls.
    pub const fn units_note(self) -> &'static str {
        match self {
            Self::RingAxialField => {
                "k = 1 in normalized teaching units; E_z is the ideal axial field kQz/(R^2+z^2)^(3/2)."
            }
            Self::ShellFlux => {
                "Normalized teaching units sampled from the shipped Hollow sphere case; Phi is a sampled estimate with quadrature uncertainty, not an SI measurement."
            }
        }
    }
}

/// Why a requested sweep configuration was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepConfigError {
    /// The range endpoint was `NaN` or infinite.
    NonFiniteRange,
    /// The lower endpoint was not strictly below the upper endpoint.
    UnorderedRange,
    /// The sample count was outside [`SWEEP_MIN_SAMPLES`]`..=`[`SWEEP_MAX_SAMPLES`].
    SampleCount,
}

impl SweepConfigError {
    /// The inline explanation for the refused input.
    pub const fn message(self) -> &'static str {
        match self {
            Self::NonFiniteRange => "The sweep range must be finite.",
            Self::UnorderedRange => "The sweep range must have min < max.",
            Self::SampleCount => "The sample count must be between 3 and 129.",
        }
    }
}

impl fmt::Display for SweepConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for SweepConfigError {}

/// A validated, evenly spaced sweep configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepConfig {
    /// Which reading is swept.
    pub kind: SweepKind,
    /// First independent-variable value.
    pub min: f64,
    /// Last independent-variable value.
    pub max: f64,
    /// Number of samples, including both endpoints.
    pub samples: usize,
}

impl SweepConfig {
    /// Validate one configuration.
    pub fn new(
        kind: SweepKind,
        min: f64,
        max: f64,
        samples: usize,
    ) -> Result<Self, SweepConfigError> {
        if !min.is_finite() || !max.is_finite() {
            return Err(SweepConfigError::NonFiniteRange);
        }
        if min >= max {
            return Err(SweepConfigError::UnorderedRange);
        }
        if !(SWEEP_MIN_SAMPLES..=SWEEP_MAX_SAMPLES).contains(&samples) {
            return Err(SweepConfigError::SampleCount);
        }
        Ok(Self {
            kind,
            min,
            max,
            samples,
        })
    }

    /// The ring lesson's default range and sample count.
    pub fn ring_default() -> Self {
        Self {
            kind: SweepKind::RingAxialField,
            min: -2.0,
            max: 2.0,
            samples: SWEEP_DEFAULT_SAMPLES,
        }
    }

    /// The shell lesson's default range: interior, interface and exterior.
    pub fn shell_default(scene: &MaxwellScene) -> Self {
        Self {
            kind: SweepKind::ShellFlux,
            min: 0.5 * scene.radius,
            max: 2.5 * scene.radius,
            samples: SWEEP_DEFAULT_SAMPLES,
        }
    }

    /// The default session configuration for `kind`.
    pub fn default_for(kind: SweepKind, scene: &MaxwellScene) -> Self {
        match kind {
            SweepKind::RingAxialField => Self::ring_default(),
            SweepKind::ShellFlux => Self::shell_default(scene),
        }
    }

    /// The number of samples.
    pub fn len(&self) -> usize {
        self.samples
    }

    /// Whether the configuration requests no samples. Valid configurations
    /// always have at least [`SWEEP_MIN_SAMPLES`].
    pub fn is_empty(&self) -> bool {
        self.samples == 0
    }

    /// The evenly spaced independent-variable value at `index`.
    ///
    /// `index` is clamped to the available samples, so a stale caller can never
    /// read past either endpoint.
    pub fn x_at(&self, index: usize) -> f64 {
        let index = index.min(self.samples.saturating_sub(1));
        let denominator = self.samples.saturating_sub(1).max(1) as f64;
        self.min + (self.max - self.min) * index as f64 / denominator
    }
}

/// One evaluated sweep sample.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepSample {
    /// Independent-variable value.
    pub x: f64,
    /// The sampled reading, or `None` when the sample is a gap.
    pub value: Option<f64>,
    /// The shipped half-resolution quadrature uncertainty, when the reading has one.
    pub uncertainty: Option<f64>,
    /// Why the sample is a gap. `Some` exactly when `value` is `None`.
    pub gap: Option<String>,
}

impl SweepSample {
    /// A valid finite sample.
    pub fn valid(x: f64, value: f64, uncertainty: Option<f64>) -> Self {
        Self {
            x,
            value: Some(value),
            uncertainty,
            gap: None,
        }
    }

    /// An invalid sample with its honest reason; never zero.
    pub fn gap(x: f64, reason: impl Into<String>) -> Self {
        Self {
            x,
            value: None,
            uncertainty: None,
            gap: Some(reason.into()),
        }
    }

    /// Whether this sample is a gap rather than a value.
    pub fn has_gap(&self) -> bool {
        self.value.is_none()
    }
}

/// Evaluate one sample through the shipped lesson reading.
pub fn evaluate(kind: SweepKind, scene: &MaxwellScene, x: f64) -> SweepSample {
    match kind {
        SweepKind::RingAxialField => {
            let value = ideal_axial_field(x);
            if value.is_finite() {
                SweepSample::valid(x, value, None)
            } else {
                SweepSample::gap(x, "The ideal axial field is not finite at this sample.")
            }
        }
        SweepKind::ShellFlux => shell_sample(scene, x),
    }
}

/// One shell sample: exterior flux with its uncertainty, or an honest gap.
fn shell_sample(scene: &MaxwellScene, x: f64) -> SweepSample {
    let reading = compute_reading(scene, x, FieldFamily::Electric, 0.0);
    match &reading.kind {
        GaussShellReadingKind::Exterior { .. } => match reading.flux_estimate() {
            Some((value, error)) if value.is_finite() && error.is_finite() => {
                SweepSample::valid(x, value, Some(error))
            }
            Some(_) => {
                SweepSample::gap(x, "The sampled flux estimate is not finite at this radius.")
            }
            None => SweepSample::gap(
                x,
                "The sampled flux estimate is unavailable at this radius.",
            ),
        },
        GaussShellReadingKind::Interior { .. } => SweepSample::gap(
            x,
            "Inside the shell (r < R): the surface encloses no net charge, so the exterior flux comparison does not apply.",
        ),
        GaussShellReadingKind::Excluded { reason } => SweepSample::gap(x, reason.clone()),
    }
}

/// The odd-symmetry check over paired samples of a symmetric range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SymmetryCheck {
    /// Number of finite `(z, -z)` pairs checked.
    pub pairs: usize,
    /// Largest `|E(z) + E(-z)|` over those pairs.
    pub max_residual: f64,
    /// The stated tolerance, derived from the largest sampled magnitude.
    pub tolerance: f64,
    /// Whether every pair met the tolerance.
    pub satisfied: bool,
}

/// The exterior-flux constancy check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FluxConstancyCheck {
    /// Number of finite exterior samples checked.
    pub samples: usize,
    /// Smallest sampled flux.
    pub min: f64,
    /// Largest sampled flux.
    pub max: f64,
    /// `max - min`.
    pub spread: f64,
    /// The largest half-resolution quadrature budget any two exterior samples
    /// could claim: twice the largest sample uncertainty.
    pub combined_uncertainty: f64,
    /// Whether the full spread stays within that largest pairwise budget.
    pub satisfied: bool,
}

/// One run's report over its completed samples.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepReport {
    /// Samples evaluated so far.
    pub samples: usize,
    /// Samples that are gaps.
    pub gaps: usize,
    /// Ring odd-symmetry result, when the range is symmetric.
    pub symmetry: Option<SymmetryCheck>,
    /// Shell exterior-flux constancy result, when exterior samples exist.
    pub flux_constancy: Option<FluxConstancyCheck>,
}

impl SweepReport {
    /// Build a report from `samples` for `config`.
    pub fn new(config: &SweepConfig, samples: &[SweepSample]) -> Self {
        Self {
            samples: samples.len(),
            gaps: samples.iter().filter(|sample| sample.has_gap()).count(),
            symmetry: match config.kind {
                SweepKind::RingAxialField => odd_symmetry_report(config, samples),
                SweepKind::ShellFlux => None,
            },
            flux_constancy: match config.kind {
                SweepKind::RingAxialField => None,
                SweepKind::ShellFlux => flux_constancy_report(samples),
            },
        }
    }
}

/// Compare `E(z)` with `-E(-z)` over a range symmetric about zero.
///
/// Returns `None` unless the configuration is symmetric and at least one pair
/// of samples is finite. Gaps are skipped, never treated as zero.
pub fn odd_symmetry_report(config: &SweepConfig, samples: &[SweepSample]) -> Option<SymmetryCheck> {
    if config.kind != SweepKind::RingAxialField {
        return None;
    }
    let scale = config.min.abs().max(config.max.abs());
    if (config.min + config.max).abs() > 1e-12 * scale.max(1.0) {
        return None;
    }
    let len = samples.len();
    let mut pairs = 0;
    let mut max_residual: f64 = 0.0;
    let mut peak: f64 = 0.0;
    for (index, sample) in samples.iter().enumerate().take(len / 2) {
        let mirror_index = len - 1 - index;
        let Some(positive) = samples[mirror_index].value else {
            continue;
        };
        let Some(negative) = sample.value else {
            continue;
        };
        // The mirrored x must actually be the negation before pairing.
        if (sample.x + samples[mirror_index].x).abs() > 1e-9 * scale.max(1.0) {
            continue;
        }
        pairs += 1;
        max_residual = max_residual.max((positive + negative).abs());
        peak = peak.max(positive.abs()).max(negative.abs());
    }
    if pairs == 0 {
        return None;
    }
    let tolerance = (ODD_SYMMETRY_RELATIVE_TOLERANCE * peak).max(f64::EPSILON);
    Some(SymmetryCheck {
        pairs,
        max_residual,
        tolerance,
        satisfied: max_residual <= tolerance,
    })
}

/// Report whether the full exterior flux spread stays within the largest
/// pairwise combined half-resolution quadrature uncertainty.
///
/// The budget is twice the largest sample uncertainty, so it covers any pair
/// of exterior samples. Returns `None` for fewer than two finite samples. Gaps
/// (interior and the excluded interface) are skipped, never folded to zero.
pub fn flux_constancy_report(samples: &[SweepSample]) -> Option<FluxConstancyCheck> {
    let mut valid = samples.iter().filter_map(|sample| {
        sample
            .value
            .map(|value| (value, sample.uncertainty.unwrap_or(0.0)))
    });
    let (first_value, first_uncertainty) = valid.next()?;
    let mut count = 1;
    let mut min = first_value;
    let mut max = first_value;
    let mut largest_uncertainty = first_uncertainty;
    for (value, uncertainty) in valid {
        count += 1;
        min = min.min(value);
        max = max.max(value);
        largest_uncertainty = largest_uncertainty.max(uncertainty);
    }
    if count < 2 {
        return None;
    }
    let spread = max - min;
    let combined_uncertainty = 2.0 * largest_uncertainty;
    Some(FluxConstancyCheck {
        samples: count,
        min,
        max,
        spread,
        combined_uncertainty,
        satisfied: spread <= combined_uncertainty + 1e-12 * spread.abs().max(1.0),
    })
}

/// Fixed model parameters shown read-only beside the sweep. They are not swept.
pub fn fixed_parameters_text(kind: SweepKind, scene: &MaxwellScene) -> String {
    match kind {
        SweepKind::RingAxialField => {
            format!("R = {RING_RADIUS:.3}, Q = {RING_CHARGE:.3}")
        }
        SweepKind::ShellFlux => format!("R = {:.3}, Q = {:.3}", scene.radius, scene.charge),
    }
}

/// Run status shown by the controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepStatus {
    /// No run has been started in this session.
    Idle,
    /// Samples remain to be evaluated.
    Sampling,
    /// Every requested sample has been evaluated.
    Complete,
    /// The learner cancelled; completed samples stay labelled cancelled.
    Cancelled,
    /// The requested configuration was refused.
    Invalid,
}

/// One bounded incremental run.
#[derive(Debug, Clone)]
pub struct SweepRun {
    config: SweepConfig,
    scene: MaxwellScene,
    samples: Vec<SweepSample>,
    next: usize,
    status: SweepStatus,
}

impl SweepRun {
    /// A fresh, not-yet-started run.
    pub fn new(config: SweepConfig, scene: &MaxwellScene) -> Self {
        Self {
            config,
            scene: scene.clone(),
            samples: Vec::with_capacity(config.samples),
            next: 0,
            status: SweepStatus::Idle,
        }
    }

    /// Start (or restart) the run from its first sample.
    pub fn start(&mut self) {
        self.samples.clear();
        self.next = 0;
        self.status = SweepStatus::Sampling;
    }

    /// Evaluate at most one sample. Returns whether work was done.
    pub fn step(&mut self) -> bool {
        if self.status != SweepStatus::Sampling {
            return false;
        }
        if self.next >= self.config.samples {
            self.status = SweepStatus::Complete;
            return false;
        }
        let x = self.config.x_at(self.next);
        self.samples
            .push(evaluate(self.config.kind, &self.scene, x));
        self.next += 1;
        if self.next >= self.config.samples {
            self.status = SweepStatus::Complete;
        }
        true
    }

    /// Stop pending work. Completed samples are retained; the status is never
    /// promoted to complete.
    pub fn cancel(&mut self) {
        if self.status == SweepStatus::Sampling {
            self.status = SweepStatus::Cancelled;
        }
    }

    /// The current status.
    pub fn status(&self) -> SweepStatus {
        self.status
    }

    /// The configuration this run uses.
    pub fn config(&self) -> SweepConfig {
        self.config
    }

    /// Evaluated samples, in order.
    pub fn samples(&self) -> &[SweepSample] {
        &self.samples
    }

    /// Evaluated sample count.
    pub fn computed(&self) -> usize {
        self.samples.len()
    }

    /// Requested sample count.
    pub fn total(&self) -> usize {
        self.config.samples
    }
}

/// A recorded preview of one sample.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Preview {
    /// The probe position before the first preview.
    start: f64,
    /// The sample's independent-variable value, as applied by the lesson.
    target: f64,
}

/// Session-only sweep controls and preview bookkeeping.
#[derive(Debug, Clone)]
pub struct SweepSession {
    kind: SweepKind,
    /// Editable lower endpoint, validated when the run starts.
    pub min: f64,
    /// Editable upper endpoint, validated when the run starts.
    pub max: f64,
    /// Editable sample count, validated when the run starts.
    pub samples: usize,
    run: Option<SweepRun>,
    selected: Option<usize>,
    preview: Option<Preview>,
    error: Option<SweepConfigError>,
}

impl Default for SweepSession {
    fn default() -> Self {
        Self {
            kind: SweepKind::RingAxialField,
            min: SweepConfig::ring_default().min,
            max: SweepConfig::ring_default().max,
            samples: SWEEP_DEFAULT_SAMPLES,
            run: None,
            selected: None,
            preview: None,
            error: None,
        }
    }
}

impl SweepSession {
    /// A fresh session with `kind`'s default range for `scene`.
    pub fn new(kind: SweepKind, scene: &MaxwellScene) -> Self {
        let config = SweepConfig::default_for(kind, scene);
        Self {
            kind,
            min: config.min,
            max: config.max,
            samples: config.samples,
            run: None,
            selected: None,
            preview: None,
            error: None,
        }
    }

    /// Which reading this session sweeps. It is fixed by the lesson.
    pub fn kind(&self) -> SweepKind {
        self.kind
    }

    /// Validate the editable fields.
    pub fn config(&self) -> Result<SweepConfig, SweepConfigError> {
        SweepConfig::new(self.kind, self.min, self.max, self.samples)
    }

    /// Start a fresh run. An invalid configuration clears any old curve and
    /// leaves the session labelled invalid rather than showing a stale result.
    ///
    /// The swept readings are static: the ring reference has no time
    /// dependence, and the shipped hollow-sphere flux is evaluated at the
    /// lesson's `t = 0` convention.
    pub fn start(&mut self, scene: &MaxwellScene) -> Result<(), SweepConfigError> {
        let config = match self.config() {
            Ok(config) => config,
            Err(error) => {
                self.run = None;
                self.selected = None;
                self.error = Some(error);
                return Err(error);
            }
        };
        let mut run = SweepRun::new(config, scene);
        run.start();
        self.run = Some(run);
        self.selected = None;
        self.error = None;
        Ok(())
    }

    /// Advance the active run by at most one sample.
    pub fn step(&mut self) -> bool {
        self.run.as_mut().is_some_and(SweepRun::step)
    }

    /// Cancel pending work, keeping the labelled partial curve.
    pub fn cancel(&mut self) {
        if let Some(run) = &mut self.run {
            run.cancel();
        }
    }

    /// The current status.
    pub fn status(&self) -> SweepStatus {
        if self.error.is_some() {
            return SweepStatus::Invalid;
        }
        self.run
            .as_ref()
            .map_or(SweepStatus::Idle, SweepRun::status)
    }

    /// The inline status line, including progress.
    pub fn status_text(&self) -> String {
        match self.status() {
            SweepStatus::Idle => "idle".to_owned(),
            SweepStatus::Invalid => format!(
                "invalid: {}",
                self.error
                    .map(SweepConfigError::message)
                    .unwrap_or("the configuration is invalid")
            ),
            SweepStatus::Sampling => {
                format!("sampling {}/{}", self.computed(), self.total())
            }
            SweepStatus::Complete => format!("complete {}/{}", self.computed(), self.total()),
            SweepStatus::Cancelled => {
                format!("cancelled after {}/{}", self.computed(), self.total())
            }
        }
    }

    /// Evaluated samples for the active run.
    pub fn samples(&self) -> &[SweepSample] {
        self.run.as_ref().map_or(&[], SweepRun::samples)
    }

    /// The configuration the active run uses.
    pub fn config_used(&self) -> Option<SweepConfig> {
        self.run.as_ref().map(SweepRun::config)
    }

    /// The active run's report.
    pub fn report(&self) -> Option<SweepReport> {
        let run = self.run.as_ref()?;
        Some(SweepReport::new(&run.config(), run.samples()))
    }

    /// The selected sample index, if any.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Select one sample for preview and marking.
    pub fn select(&mut self, index: usize) {
        if index < self.samples().len() {
            self.selected = Some(index);
        }
    }

    /// Record a preview request. The first call in a preview captures the
    /// starting probe; later sample clicks keep that starting probe.
    pub fn note_preview(&mut self, target: f64, current_probe: f64) {
        let start = self.preview.map_or(current_probe, |preview| preview.start);
        self.preview = Some(Preview { start, target });
    }

    /// Update the previewed target after the lesson applies its bounded setter.
    pub fn update_preview_target(&mut self, target: f64) {
        if let Some(preview) = &mut self.preview {
            preview.target = target;
        }
    }

    /// The probe position captured before the first preview.
    pub fn preview_start(&self) -> Option<f64> {
        self.preview.map(|preview| preview.start)
    }

    /// The sample target currently previewed.
    pub fn preview_target(&self) -> Option<f64> {
        self.preview.map(|preview| preview.target)
    }

    /// Whether a sample is being previewed.
    pub fn is_previewing(&self) -> bool {
        self.preview.is_some()
    }

    /// End preview mode, returning the starting probe for restoration.
    pub fn end_preview(&mut self) -> Option<f64> {
        self.preview.take().map(|preview| preview.start)
    }

    fn computed(&self) -> usize {
        self.run.as_ref().map_or(0, SweepRun::computed)
    }

    fn total(&self) -> usize {
        self.run.as_ref().map_or(self.samples, SweepRun::total)
    }
}
