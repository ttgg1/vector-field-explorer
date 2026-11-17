//! Validated source scene: catalogue, CRUD and scene-level validation.
//!
//! The scene owns the list of enabled/disabled source entries plus the shared
//! display, softening and accuracy opt-in settings. Source geometry and
//! numerical validation remain in [`crate::sources`]; this module only maps a
//! candidate scene through the existing [`SourceSet`] and [`EvalOptions`] seams.

use glam::DVec3;

use crate::sources::{
    EvalOptions, FieldFamily, InputError, InvalidOption, MAX_SOURCES, Source, SourceKind,
    SourceParameter, SourceSet, WorkLimits,
};

/// Smallest accepted scene-wide softening length.
pub const MIN_SOFTENING: f64 = 1e-6;
/// Largest accepted scene-wide softening length.
pub const MAX_SOFTENING: f64 = 1.0;

/// Which field source the viewport shows: a formula or the source scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FieldMode {
    #[default]
    Formula,
    Sources,
}

/// Which field families the source scene displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SourceDisplay {
    Electric,
    Magnetic,
    #[default]
    Both,
}

/// One catalogue entry in the scene, with its enable state.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntry {
    pub enabled: bool,
    pub source: Source,
}

/// The editable source scene settings.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceScene {
    pub entries: Vec<SourceEntry>,
    pub display: SourceDisplay,
    pub softening: f64,
    pub allow_inaccurate: bool,
}

/// A scene change rejected by the shared source validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSceneError {
    Limit {
        max: usize,
    },
    InvalidSource {
        index: usize,
        parameter: &'static str,
    },
    SofteningOutOfRange,
    InvalidLimits,
    Cancelled,
    AllocationFailed,
}

/// The eight catalogue kinds in panel order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKindTag {
    PointCharge,
    FiniteLineCharge,
    ChargedRing,
    ChargedDisc,
    SolidSphere,
    StraightWire,
    CurrentLoop,
    Solenoid,
}

impl SourceKindTag {
    /// Every catalogue kind, in panel order.
    pub const ALL: [Self; 8] = [
        Self::PointCharge,
        Self::FiniteLineCharge,
        Self::ChargedRing,
        Self::ChargedDisc,
        Self::SolidSphere,
        Self::StraightWire,
        Self::CurrentLoop,
        Self::Solenoid,
    ];

    /// Human-readable name used by the source catalogue.
    pub fn label(self) -> &'static str {
        match self {
            Self::PointCharge => "Point charge",
            Self::FiniteLineCharge => "Finite line charge",
            Self::ChargedRing => "Charged ring",
            Self::ChargedDisc => "Charged disc",
            Self::SolidSphere => "Solid sphere",
            Self::StraightWire => "Straight wire",
            Self::CurrentLoop => "Current loop",
            Self::Solenoid => "Solenoid",
        }
    }

    /// The field family this kind contributes to.
    pub fn family(self) -> FieldFamily {
        match self {
            Self::PointCharge
            | Self::FiniteLineCharge
            | Self::ChargedRing
            | Self::ChargedDisc
            | Self::SolidSphere => FieldFamily::Electric,
            Self::StraightWire | Self::CurrentLoop | Self::Solenoid => FieldFamily::Magnetic,
        }
    }

    /// A valid source with the catalogue defaults for this kind.
    pub fn default_source(self) -> Source {
        let kind = match self {
            Self::PointCharge => SourceKind::PointCharge { charge: 1.0 },
            Self::FiniteLineCharge => SourceKind::FiniteLineCharge {
                charge: 1.0,
                length: 1.0,
            },
            Self::ChargedRing => SourceKind::ChargedRing {
                charge: 1.0,
                radius: 1.0,
            },
            Self::ChargedDisc => SourceKind::ChargedDisc {
                charge: 1.0,
                radius: 1.0,
            },
            Self::SolidSphere => SourceKind::SolidSphere {
                charge: 1.0,
                radius: 1.0,
            },
            Self::StraightWire => SourceKind::StraightWire { current: 1.0 },
            Self::CurrentLoop => SourceKind::CurrentLoop {
                current: 1.0,
                radius: 1.0,
            },
            Self::Solenoid => SourceKind::Solenoid {
                current: 1.0,
                radius: 0.5,
                length: 2.0,
                turns: 8,
            },
        };
        Source {
            center: DVec3::ZERO,
            axis: DVec3::Z,
            kind,
        }
    }

    fn of(kind: &SourceKind) -> Self {
        match kind {
            SourceKind::PointCharge { .. } => Self::PointCharge,
            SourceKind::FiniteLineCharge { .. } => Self::FiniteLineCharge,
            SourceKind::ChargedRing { .. } => Self::ChargedRing,
            SourceKind::ChargedDisc { .. } => Self::ChargedDisc,
            SourceKind::SolidSphere { .. } => Self::SolidSphere,
            SourceKind::StraightWire { .. } => Self::StraightWire,
            SourceKind::CurrentLoop { .. } => Self::CurrentLoop,
            SourceKind::Solenoid { .. } => Self::Solenoid,
        }
    }
}

impl Default for SourceScene {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            display: SourceDisplay::Both,
            softening: 0.1,
            allow_inaccurate: false,
        }
    }
}

impl SourceScene {
    /// Append an enabled source after validating the resulting scene.
    pub fn add(&mut self, source: Source) -> Result<usize, SourceSceneError> {
        if self.entries.len() >= MAX_SOURCES {
            return Err(SourceSceneError::Limit { max: MAX_SOURCES });
        }
        let index = self.entries.len();
        let mut candidate = self.clone();
        candidate.entries.push(SourceEntry {
            enabled: true,
            source,
        });
        candidate.validate()?;
        self.entries = candidate.entries;
        Ok(index)
    }

    /// Remove one entry, returning it unchanged.
    pub fn remove(&mut self, index: usize) -> Option<SourceEntry> {
        if index < self.entries.len() {
            Some(self.entries.remove(index))
        } else {
            None
        }
    }

    /// Enable or disable one entry, returning whether the index existed.
    pub fn set_enabled(&mut self, index: usize, enabled: bool) -> bool {
        match self.entries.get_mut(index) {
            Some(entry) => {
                entry.enabled = enabled;
                true
            }
            None => false,
        }
    }

    /// Replace one entry's source after validating the resulting scene.
    pub fn replace(&mut self, index: usize, source: Source) -> Result<(), SourceSceneError> {
        if index >= self.entries.len() {
            return Err(SourceSceneError::InvalidSource {
                index,
                parameter: "index",
            });
        }
        let mut candidate = self.clone();
        candidate.entries[index].source = source;
        candidate.validate()?;
        self.entries = candidate.entries;
        Ok(())
    }

    /// Restore one entry to its kind defaults, keeping center, axis and enabled
    /// state.
    pub fn reset_entry(&mut self, index: usize) -> bool {
        match self.entries.get_mut(index) {
            Some(entry) => {
                let tag = SourceKindTag::of(&entry.source.kind);
                let mut reset = tag.default_source();
                reset.center = entry.source.center;
                reset.axis = entry.source.axis;
                entry.source = reset;
                true
            }
            None => false,
        }
    }

    /// Restore an empty scene and the default display/softening/opt-in.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Set the scene-wide softening, rejecting non-finite or out-of-range values.
    pub fn set_softening(&mut self, softening: f64) -> Result<(), SourceSceneError> {
        if !softening.is_finite() || !(MIN_SOFTENING..=MAX_SOFTENING).contains(&softening) {
            return Err(SourceSceneError::SofteningOutOfRange);
        }
        self.softening = softening;
        Ok(())
    }

    /// Select which field families are displayed.
    pub fn set_display(&mut self, display: SourceDisplay) {
        self.display = display;
    }

    /// Set whether finite accuracy-not-met results may be drawn.
    pub fn set_allow_inaccurate(&mut self, allowed: bool) {
        self.allow_inaccurate = allowed;
    }

    /// Only the enabled entries, in list order.
    pub fn enabled_sources(&self) -> impl Iterator<Item = &Source> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| &entry.source)
    }

    /// Validate the enabled entries and the scene-wide options together.
    pub fn validate(&self) -> Result<(), SourceSceneError> {
        if !self.softening.is_finite() || !(MIN_SOFTENING..=MAX_SOFTENING).contains(&self.softening)
        {
            return Err(SourceSceneError::SofteningOutOfRange);
        }
        let sources: Vec<Source> = self.enabled_sources().copied().collect();
        SourceSet::new(&sources, &WorkLimits::default(), &|| false).map_err(map_input_error)?;
        let options = EvalOptions {
            softening: self.softening,
            ..EvalOptions::default()
        };
        options.validate().map_err(|option| match option {
            InvalidOption::Softening => SourceSceneError::SofteningOutOfRange,
            _ => SourceSceneError::InvalidLimits,
        })?;
        Ok(())
    }
}

fn map_input_error(error: InputError) -> SourceSceneError {
    match error {
        InputError::Cancelled => SourceSceneError::Cancelled,
        InputError::SourceLimit { max, .. } => SourceSceneError::Limit { max },
        InputError::InvalidSource { index, parameter } => SourceSceneError::InvalidSource {
            index,
            parameter: parameter_name(parameter),
        },
        InputError::InvalidLimits(_) => SourceSceneError::InvalidLimits,
        InputError::AllocationFailed => SourceSceneError::AllocationFailed,
    }
}

fn parameter_name(parameter: SourceParameter) -> &'static str {
    match parameter {
        SourceParameter::Center => "center",
        SourceParameter::Axis => "axis",
        SourceParameter::Charge => "charge",
        SourceParameter::Current => "current",
        SourceParameter::Radius => "radius",
        SourceParameter::Length => "length",
        SourceParameter::Turns => "turns",
    }
}
