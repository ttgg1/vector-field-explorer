//! Validated source scene: catalogue, CRUD and scene-level validation.
//!
//! The scene owns the list of enabled/disabled source entries plus the shared
//! display, softening and accuracy opt-in settings. Source geometry and
//! numerical validation remain in [`crate::sources`]; this module only maps a
//! candidate scene through the existing [`SourceSet`] and [`EvalOptions`] seams.

use glam::{DVec3, UVec3, Vec3};

use crate::domain::Domain;
use crate::plane::PlaneFrame;
use crate::sampling::{ArrowData, ViewOptions, magnitude_range};
use crate::sources::{
    EvalOptions, Evaluator, FamilyResult, FieldFamily, InputError, InvalidOption, MAX_SOURCES,
    Source, SourceKind, SourceParameter, SourceSet, WorkLimits,
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

/// The flat, serde-friendly form of one scene entry.
///
/// Every kind shares one shape: the numeric parameters a kind does not use are
/// ignored, so adding a kind never grows the record. [`SourceRecord::to_source`]
/// maps the `kind` string back to exactly one [`SourceKind`] and validates the
/// result through the same [`SourceSet`] seam as the live scene.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SourceRecord {
    pub enabled: bool,
    pub kind: String,
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub strength: f64,
    pub radius: f64,
    pub length: f64,
    pub turns: u32,
}

impl SourceRecord {
    /// Flatten one source, naming its kind by the stable serialized spelling.
    pub fn from_source(source: &Source) -> Self {
        let tag = SourceKindTag::of(&source.kind);
        Self {
            enabled: true,
            kind: tag.name().to_owned(),
            center: source.center.to_array(),
            axis: source.axis.to_array(),
            strength: match source.kind {
                SourceKind::PointCharge { charge }
                | SourceKind::FiniteLineCharge { charge, .. }
                | SourceKind::ChargedRing { charge, .. }
                | SourceKind::ChargedDisc { charge, .. }
                | SourceKind::SolidSphere { charge, .. } => charge,
                SourceKind::StraightWire { current }
                | SourceKind::CurrentLoop { current, .. }
                | SourceKind::Solenoid { current, .. } => current,
            },
            radius: match source.kind {
                SourceKind::ChargedRing { radius, .. }
                | SourceKind::ChargedDisc { radius, .. }
                | SourceKind::SolidSphere { radius, .. }
                | SourceKind::CurrentLoop { radius, .. }
                | SourceKind::Solenoid { radius, .. } => radius,
                _ => 0.0,
            },
            length: match source.kind {
                SourceKind::FiniteLineCharge { length, .. }
                | SourceKind::Solenoid { length, .. } => length,
                _ => 0.0,
            },
            turns: match source.kind {
                SourceKind::Solenoid { turns, .. } => turns,
                _ => 0,
            },
        }
    }

    /// Rebuild and validate one source. Unknown kinds, non-finite numbers and
    /// impossible dimensions all fail; the caller decides whether to skip.
    pub fn to_source(&self) -> Result<Source, String> {
        let Some(tag) = SourceKindTag::from_name(&self.kind) else {
            return Err(format!("unknown source kind {:?}", self.kind));
        };
        let kind = match tag {
            SourceKindTag::PointCharge => SourceKind::PointCharge {
                charge: self.strength,
            },
            SourceKindTag::FiniteLineCharge => SourceKind::FiniteLineCharge {
                charge: self.strength,
                length: self.length,
            },
            SourceKindTag::ChargedRing => SourceKind::ChargedRing {
                charge: self.strength,
                radius: self.radius,
            },
            SourceKindTag::ChargedDisc => SourceKind::ChargedDisc {
                charge: self.strength,
                radius: self.radius,
            },
            SourceKindTag::SolidSphere => SourceKind::SolidSphere {
                charge: self.strength,
                radius: self.radius,
            },
            SourceKindTag::StraightWire => SourceKind::StraightWire {
                current: self.strength,
            },
            SourceKindTag::CurrentLoop => SourceKind::CurrentLoop {
                current: self.strength,
                radius: self.radius,
            },
            SourceKindTag::Solenoid => SourceKind::Solenoid {
                current: self.strength,
                radius: self.radius,
                length: self.length,
                turns: self.turns,
            },
        };
        let source = Source {
            center: DVec3::from_array(self.center),
            axis: DVec3::from_array(self.axis),
            kind,
        };
        SourceSet::new(
            std::slice::from_ref(&source),
            &WorkLimits::default(),
            &|| false,
        )
        .map_err(|error| error.to_string())?;
        Ok(source)
    }
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

    /// The stable spelling persisted in [`SourceRecord::kind`].
    fn name(self) -> &'static str {
        match self {
            Self::PointCharge => "PointCharge",
            Self::FiniteLineCharge => "FiniteLineCharge",
            Self::ChargedRing => "ChargedRing",
            Self::ChargedDisc => "ChargedDisc",
            Self::SolidSphere => "SolidSphere",
            Self::StraightWire => "StraightWire",
            Self::CurrentLoop => "CurrentLoop",
            Self::Solenoid => "Solenoid",
        }
    }

    /// The inverse of [`SourceKindTag::name`]; unknown spellings are rejected.
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "PointCharge" => Some(Self::PointCharge),
            "FiniteLineCharge" => Some(Self::FiniteLineCharge),
            "ChargedRing" => Some(Self::ChargedRing),
            "ChargedDisc" => Some(Self::ChargedDisc),
            "SolidSphere" => Some(Self::SolidSphere),
            "StraightWire" => Some(Self::StraightWire),
            "CurrentLoop" => Some(Self::CurrentLoop),
            "Solenoid" => Some(Self::Solenoid),
            _ => None,
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

    /// Flatten every entry (enabled and disabled) in list order.
    pub fn to_records(&self) -> Vec<SourceRecord> {
        self.entries
            .iter()
            .map(|entry| {
                let mut record = SourceRecord::from_source(&entry.source);
                record.enabled = entry.enabled;
                record
            })
            .collect()
    }

    /// Rebuild the scene entries from persisted records, skipping every record
    /// that fails validation. Display, softening and the accuracy opt-in start
    /// at their defaults; the restore path applies the saved settings.
    pub fn from_records(records: &[SourceRecord]) -> Self {
        let mut scene = Self::default();
        for record in records {
            if scene.entries.len() >= MAX_SOURCES {
                break;
            }
            let Ok(source) = record.to_source() else {
                continue;
            };
            scene.entries.push(SourceEntry {
                enabled: record.enabled,
                source,
            });
        }
        scene
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

/// Fixed electric-family arrow colour in `Both` display: amber.
pub const ELECTRIC_COLOR: [u8; 3] = [255, 176, 64];
/// Fixed magnetic-family arrow colour in `Both` display: cyan.
pub const MAGNETIC_COLOR: [u8; 3] = [64, 196, 255];

/// Per-family counters for one source sampling pass.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceFamilyStatus {
    /// Enabled sources that contribute to this family.
    pub sources: usize,
    /// Arrows emitted for this family in the active display.
    pub drawn: usize,
    /// Query results whose accuracy target was met.
    pub target_met: usize,
    /// Every `AccuracyNotMet` result, drawn or hidden.
    pub accuracy_not_met_total: usize,
    /// `AccuracyNotMet` results drawn because the opt-in is on.
    pub accuracy_not_met_drawn: usize,
    /// Results the evaluator could not publish as a finite vector.
    pub unavailable: usize,
    /// The family has no enabled sources; zero is its exact value, not an error.
    pub empty: bool,
}

/// The arrows and honest per-family status for one source sampling pass.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceSampling {
    pub arrows: Vec<ArrowData>,
    pub electric: SourceFamilyStatus,
    pub magnetic: SourceFamilyStatus,
    pub softening: f64,
    pub allow_inaccurate: bool,
    pub validation_error: Option<String>,
}

/// What one family result contributes to the active display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyDisposition {
    Draw,
    HiddenAccuracy,
    Unavailable,
}

/// Whether a family estimate may become an arrow, given the accuracy opt-in.
pub fn classify_family_result(result: &FamilyResult, allow_inaccurate: bool) -> FamilyDisposition {
    match result {
        FamilyResult::TargetMet(estimate) if drawable(estimate) => FamilyDisposition::Draw,
        FamilyResult::AccuracyNotMet(estimate, _) if allow_inaccurate && drawable(estimate) => {
            FamilyDisposition::Draw
        }
        FamilyResult::AccuracyNotMet(_, _) => FamilyDisposition::HiddenAccuracy,
        FamilyResult::Unavailable(_) => FamilyDisposition::Unavailable,
        FamilyResult::TargetMet(_) => FamilyDisposition::Unavailable,
    }
}

/// Sample the scene's enabled sources through the real evaluator.
///
/// A 3D view samples `domain.grid_points(grid)` in world coordinates. A 2D view
/// samples the existing [`PlaneFrame`]'s local grid, evaluates in world, projects
/// each vector onto the plane and returns local positions. Invalid scenes return
/// their typed message and no arrows; an empty family is an exact zero.
pub fn sample_source_scene(
    scene: &SourceScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
    view: &ViewOptions,
) -> SourceSampling {
    let electric = family_status(scene, FieldFamily::Electric);
    let magnetic = family_status(scene, FieldFamily::Magnetic);
    let invalid = |message: String| SourceSampling {
        arrows: Vec::new(),
        electric: electric.clone(),
        magnetic: magnetic.clone(),
        softening: scene.softening,
        allow_inaccurate: scene.allow_inaccurate,
        validation_error: Some(message),
    };

    if let Err(error) = scene.validate() {
        return invalid(scene_error_message(&error));
    }
    let sources: Vec<Source> = scene.enabled_sources().copied().collect();
    let set = match SourceSet::new(&sources, &WorkLimits::default(), &|| false) {
        Ok(set) => set,
        Err(error) => return invalid(error.to_string()),
    };
    let options = EvalOptions {
        softening: scene.softening,
        ..EvalOptions::default()
    };
    let mut evaluator = match Evaluator::new(&set, options, &|| false) {
        Ok(evaluator) => evaluator,
        Err(error) => return invalid(error.to_string()),
    };

    let mut electric_status = electric;
    let mut magnetic_status = magnetic;
    let electric_displayed = matches!(scene.display, SourceDisplay::Electric | SourceDisplay::Both);
    let magnetic_displayed = matches!(scene.display, SourceDisplay::Magnetic | SourceDisplay::Both);
    let mut electric_points = Vec::new();
    let mut magnetic_points = Vec::new();

    let mut visit = |position: Vec3, world: Vec3| {
        let sample = evaluator.evaluate(world.as_dvec3(), &|| false);
        observe_family(
            &sample.electric,
            scene.allow_inaccurate,
            electric_displayed,
            position,
            plane,
            &mut electric_status,
            &mut electric_points,
        );
        observe_family(
            &sample.magnetic,
            scene.allow_inaccurate,
            magnetic_displayed,
            position,
            plane,
            &mut magnetic_status,
            &mut magnetic_points,
        );
    };

    if let Some(frame) = plane {
        for local in frame
            .local_domain()
            .grid_points(UVec3::new(grid.x, grid.y, 1))
        {
            visit(local, frame.to_world(local));
        }
    } else {
        for world in domain.grid_points(grid) {
            visit(world, world);
        }
    }

    let arrows = match scene.display {
        SourceDisplay::Both => {
            let mut arrows = Vec::with_capacity(electric_points.len() + magnetic_points.len());
            arrows.extend(
                electric_points
                    .into_iter()
                    .map(|(position, dir, mag)| ArrowData {
                        position,
                        dir,
                        mag,
                        color: ELECTRIC_COLOR,
                    }),
            );
            arrows.extend(
                magnetic_points
                    .into_iter()
                    .map(|(position, dir, mag)| ArrowData {
                        position,
                        dir,
                        mag,
                        color: MAGNETIC_COLOR,
                    }),
            );
            arrows
        }
        SourceDisplay::Electric => color_family(electric_points, view),
        SourceDisplay::Magnetic => color_family(magnetic_points, view),
    };

    SourceSampling {
        arrows,
        electric: electric_status,
        magnetic: magnetic_status,
        softening: scene.softening,
        allow_inaccurate: scene.allow_inaccurate,
        validation_error: None,
    }
}

fn family_status(scene: &SourceScene, family: FieldFamily) -> SourceFamilyStatus {
    let sources = scene
        .enabled_sources()
        .filter(|source| source.kind.family() == family)
        .count();
    SourceFamilyStatus {
        sources,
        empty: sources == 0,
        ..SourceFamilyStatus::default()
    }
}

fn observe_family(
    result: &FamilyResult,
    allow_inaccurate: bool,
    displayed: bool,
    position: Vec3,
    plane: Option<&PlaneFrame>,
    status: &mut SourceFamilyStatus,
    points: &mut Vec<(Vec3, Vec3, f32)>,
) {
    match result {
        FamilyResult::TargetMet(_) => status.target_met += 1,
        FamilyResult::AccuracyNotMet(_, _) => status.accuracy_not_met_total += 1,
        FamilyResult::Unavailable(_) => status.unavailable += 1,
    }
    if !displayed || classify_family_result(result, allow_inaccurate) != FamilyDisposition::Draw {
        return;
    }
    let estimate = match result {
        FamilyResult::TargetMet(estimate) | FamilyResult::AccuracyNotMet(estimate, _) => estimate,
        FamilyResult::Unavailable(_) => return,
    };
    let Some(world) = finite_vec(estimate.value) else {
        return;
    };
    let (dir, mag) = match plane {
        Some(frame) => {
            let projected = frame.project(world);
            let mag = projected.length();
            if !mag.is_finite() || mag < 1e-6 {
                return;
            }
            (projected / mag, mag)
        }
        None => {
            let mag = world.length();
            if !mag.is_finite() || mag < 1e-6 {
                return;
            }
            (world / mag, mag)
        }
    };
    points.push((position, dir, mag));
    status.drawn += 1;
    if matches!(result, FamilyResult::AccuracyNotMet(_, _)) {
        status.accuracy_not_met_drawn += 1;
    }
}

fn color_family(points: Vec<(Vec3, Vec3, f32)>, view: &ViewOptions) -> Vec<ArrowData> {
    let mut arrows: Vec<ArrowData> = points
        .into_iter()
        .map(|(position, dir, mag)| ArrowData {
            position,
            dir,
            mag,
            color: [128, 128, 128],
        })
        .collect();
    let (min, max) = view.color_range.unwrap_or_else(|| magnitude_range(&arrows));
    for arrow in &mut arrows {
        arrow.color = view.colormap.colormap(arrow.mag, min, max);
    }
    arrows
}

fn drawable(estimate: &crate::sources::Estimate) -> bool {
    estimate.value.is_finite()
        && estimate.value.length().is_finite()
        && estimate.value.length() >= 1e-6
}

fn finite_vec(value: DVec3) -> Option<Vec3> {
    if !value.is_finite() {
        return None;
    }
    let vector = Vec3::new(value.x as f32, value.y as f32, value.z as f32);
    vector.is_finite().then_some(vector)
}

fn scene_error_message(error: &SourceSceneError) -> String {
    format!("{error:?}")
}
