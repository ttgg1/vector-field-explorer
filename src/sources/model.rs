//! Validated source model and work limits.

use std::error::Error;
use std::fmt;

use glam::DVec3;

/// Hard ceiling on the number of sources in one set.
pub const MAX_SOURCES: usize = 64;
/// Hard ceiling on work units per family per point.
pub const MAX_WORK_PER_FAMILY: u64 = 2_000_000;
/// Hard ceiling on resident cells per family.
pub const MAX_CELLS_PER_FAMILY: usize = 8_192;
/// Hard ceiling on subdivision depth.
pub const MAX_DEPTH: u8 = 48;
/// Hard ceiling on evaluated points per evaluator job.
pub const MAX_QUERIES: u64 = 65_536;

const DEFAULT_EPSILON: f64 = 1e-7;
const DEFAULT_FIELD_SCALE: f64 = 1e-2;
const DEFAULT_SOFTENING: f64 = 0.1;

/// Which field family a source contributes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldFamily {
    Electric,
    Magnetic,
}

/// Identifies the source parameter that failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceParameter {
    Center,
    Axis,
    Charge,
    Current,
    Radius,
    Length,
    Turns,
}

/// Identifies the work limit that failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkLimitField {
    MaxSources,
    WorkPerFamily,
    CellsPerFamily,
    MaxDepth,
    MaxQueries,
}

/// Identifies the evaluator option that failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvalidOption {
    Softening,
    Epsilon,
    ElectricScale,
    MagneticScale,
    Limits(WorkLimitField),
}

/// A source accepted by the public model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Source {
    pub center: DVec3,
    pub axis: DVec3,
    pub kind: SourceKind,
}

/// The eight supported source distributions: point charge, finite line charge,
/// charged ring, charged disc, solid sphere, straight wire, current loop and
/// finite solenoid. Each kind contributes to its own field family.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourceKind {
    PointCharge {
        charge: f64,
    },
    FiniteLineCharge {
        charge: f64,
        length: f64,
    },
    ChargedRing {
        charge: f64,
        radius: f64,
    },
    ChargedDisc {
        charge: f64,
        radius: f64,
    },
    SolidSphere {
        charge: f64,
        radius: f64,
    },
    StraightWire {
        current: f64,
    },
    CurrentLoop {
        current: f64,
        radius: f64,
    },
    Solenoid {
        current: f64,
        radius: f64,
        length: f64,
        turns: u32,
    },
}

impl SourceKind {
    /// The field family this source contributes to.
    pub fn family(&self) -> FieldFamily {
        match self {
            Self::PointCharge { .. }
            | Self::FiniteLineCharge { .. }
            | Self::ChargedRing { .. }
            | Self::ChargedDisc { .. }
            | Self::SolidSphere { .. } => FieldFamily::Electric,
            Self::StraightWire { .. } | Self::CurrentLoop { .. } | Self::Solenoid { .. } => {
                FieldFamily::Magnetic
            }
        }
    }
}

/// Requested numerical tolerance. Epsilon is not a certified bound and is
/// independent of physical softening.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccuracyRequest {
    pub epsilon: f64,
    pub e_scale: f64,
    pub b_scale: f64,
}

impl Default for AccuracyRequest {
    fn default() -> Self {
        Self {
            epsilon: DEFAULT_EPSILON,
            e_scale: DEFAULT_FIELD_SCALE,
            b_scale: DEFAULT_FIELD_SCALE,
        }
    }
}

/// Hard resource ceilings for one evaluator. Callers may lower but not raise
/// the documented defaults in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkLimits {
    pub max_sources: usize,
    pub work_per_family: u64,
    pub cells_per_family: usize,
    pub max_depth: u8,
    pub max_queries: u64,
}

impl Default for WorkLimits {
    fn default() -> Self {
        Self {
            max_sources: MAX_SOURCES,
            work_per_family: MAX_WORK_PER_FAMILY,
            cells_per_family: MAX_CELLS_PER_FAMILY,
            max_depth: MAX_DEPTH,
            max_queries: MAX_QUERIES,
        }
    }
}

impl WorkLimits {
    pub(crate) fn validate(&self) -> Result<(), WorkLimitField> {
        if self.max_sources == 0 || self.max_sources > MAX_SOURCES {
            return Err(WorkLimitField::MaxSources);
        }
        if self.work_per_family == 0 || self.work_per_family > MAX_WORK_PER_FAMILY {
            return Err(WorkLimitField::WorkPerFamily);
        }
        if self.cells_per_family == 0 || self.cells_per_family > MAX_CELLS_PER_FAMILY {
            return Err(WorkLimitField::CellsPerFamily);
        }
        if self.max_depth == 0 || self.max_depth > MAX_DEPTH {
            return Err(WorkLimitField::MaxDepth);
        }
        if self.max_queries == 0 || self.max_queries > MAX_QUERIES {
            return Err(WorkLimitField::MaxQueries);
        }
        self.cells_per_family
            .checked_mul(std::mem::size_of::<f64>())
            .ok_or(WorkLimitField::CellsPerFamily)?;
        Ok(())
    }
}

/// Evaluator options: one global softening length plus accuracy and limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvalOptions {
    pub softening: f64,
    pub accuracy: AccuracyRequest,
    pub limits: WorkLimits,
}

impl Default for EvalOptions {
    fn default() -> Self {
        Self {
            softening: DEFAULT_SOFTENING,
            accuracy: AccuracyRequest::default(),
            limits: WorkLimits::default(),
        }
    }
}

impl EvalOptions {
    pub(crate) fn validate(&self) -> Result<(), InvalidOption> {
        if !self.softening.is_finite() || self.softening <= 0.0 {
            return Err(InvalidOption::Softening);
        }
        if !self.accuracy.epsilon.is_finite() || self.accuracy.epsilon <= 0.0 {
            return Err(InvalidOption::Epsilon);
        }
        if !self.accuracy.e_scale.is_finite() || self.accuracy.e_scale <= 0.0 {
            return Err(InvalidOption::ElectricScale);
        }
        if !self.accuracy.b_scale.is_finite() || self.accuracy.b_scale <= 0.0 {
            return Err(InvalidOption::MagneticScale);
        }
        self.limits.validate().map_err(InvalidOption::Limits)?;
        Ok(())
    }
}

/// A validated, immutable source list. Validation happens before the sources
/// are copied, and cancellation is polled before any reservation.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSet {
    sources: Vec<Source>,
    limits: WorkLimits,
}

impl SourceSet {
    pub fn new(
        sources: &[Source],
        limits: &WorkLimits,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, InputError> {
        if cancelled() {
            return Err(InputError::Cancelled);
        }
        limits.validate().map_err(InputError::InvalidLimits)?;
        if sources.len() > limits.max_sources {
            return Err(InputError::SourceLimit {
                requested: sources.len(),
                max: limits.max_sources,
            });
        }
        for (index, source) in sources.iter().enumerate() {
            validate_source(source)
                .map_err(|parameter| InputError::InvalidSource { index, parameter })?;
        }
        sources
            .len()
            .checked_mul(std::mem::size_of::<Source>())
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .ok_or(InputError::AllocationFailed)?;
        if cancelled() {
            return Err(InputError::Cancelled);
        }
        let mut copy = Vec::new();
        copy.try_reserve_exact(sources.len())
            .map_err(|_| InputError::AllocationFailed)?;
        copy.extend_from_slice(sources);
        Ok(Self {
            sources: copy,
            limits: *limits,
        })
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn limits(&self) -> WorkLimits {
        self.limits
    }

    pub(crate) fn sources(&self) -> &[Source] {
        &self.sources
    }
}

/// Errors raised while validating and preparing a source set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    Cancelled,
    SourceLimit {
        requested: usize,
        max: usize,
    },
    InvalidSource {
        index: usize,
        parameter: SourceParameter,
    },
    InvalidLimits(WorkLimitField),
    AllocationFailed,
}

/// Errors raised while preparing an evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareError {
    Cancelled,
    InvalidOptions(InvalidOption),
    SourceLimit { requested: usize, max: usize },
    AllocationFailed,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("source preparation cancelled"),
            Self::SourceLimit { requested, max } => {
                write!(f, "{requested} sources exceed the limit of {max}")
            }
            Self::InvalidSource { index, parameter } => {
                write!(f, "source {index} has invalid {parameter:?}")
            }
            Self::InvalidLimits(field) => write!(f, "invalid work limit {field:?}"),
            Self::AllocationFailed => f.write_str("source allocation failed"),
        }
    }
}

impl Error for InputError {}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("evaluator preparation cancelled"),
            Self::InvalidOptions(option) => write!(f, "invalid evaluator option {option:?}"),
            Self::SourceLimit { requested, max } => {
                write!(f, "{requested} sources exceed the limit of {max}")
            }
            Self::AllocationFailed => f.write_str("evaluator allocation failed"),
        }
    }
}

impl Error for PrepareError {}

fn validate_source(source: &Source) -> Result<(), SourceParameter> {
    if !source.center.is_finite() {
        return Err(SourceParameter::Center);
    }
    if !source.axis.is_finite() {
        return Err(SourceParameter::Axis);
    }
    match source.kind {
        SourceKind::PointCharge { charge } => finite_strength(charge, SourceParameter::Charge),
        SourceKind::FiniteLineCharge { charge, length } => {
            finite_strength(charge, SourceParameter::Charge)?;
            positive_dimension(length, SourceParameter::Length)?;
            used_axis(source.axis)
        }
        SourceKind::ChargedRing { charge, radius } => {
            finite_strength(charge, SourceParameter::Charge)?;
            positive_dimension(radius, SourceParameter::Radius)?;
            used_axis(source.axis)
        }
        SourceKind::ChargedDisc { charge, radius } => {
            finite_strength(charge, SourceParameter::Charge)?;
            positive_dimension(radius, SourceParameter::Radius)?;
            used_axis(source.axis)
        }
        SourceKind::SolidSphere { charge, radius } => {
            finite_strength(charge, SourceParameter::Charge)?;
            positive_dimension(radius, SourceParameter::Radius)
        }
        SourceKind::StraightWire { current } => {
            finite_strength(current, SourceParameter::Current)?;
            used_axis(source.axis)
        }
        SourceKind::CurrentLoop { current, radius } => {
            finite_strength(current, SourceParameter::Current)?;
            positive_dimension(radius, SourceParameter::Radius)?;
            used_axis(source.axis)
        }
        SourceKind::Solenoid {
            current,
            radius,
            length,
            turns,
        } => {
            finite_strength(current, SourceParameter::Current)?;
            positive_dimension(radius, SourceParameter::Radius)?;
            positive_dimension(length, SourceParameter::Length)?;
            if turns == 0 {
                return Err(SourceParameter::Turns);
            }
            used_axis(source.axis)
        }
    }
}

fn finite_strength(value: f64, parameter: SourceParameter) -> Result<(), SourceParameter> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(parameter)
    }
}

fn positive_dimension(value: f64, parameter: SourceParameter) -> Result<(), SourceParameter> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(parameter)
    }
}

fn used_axis(axis: DVec3) -> Result<(), SourceParameter> {
    if axis == DVec3::ZERO {
        Err(SourceParameter::Axis)
    } else {
        Ok(())
    }
}
