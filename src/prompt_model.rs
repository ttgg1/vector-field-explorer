//! Issue #213: interpret supported electrostatics homework prompts as editable
//! physical models.
//!
//! The grammar is a deliberately small, offline extension of the #168 tolerant
//! CAS line: a positive/negative point charge at the origin, or two equal and
//! opposite point charges (a dipole) along one Cartesian axis. Parsing, preview
//! and the model itself are pure. [`apply`] is the only function that touches
//! application state, and it goes through the ordinary source-scene setters so
//! undo and persistence keep working.
//!
//! Nothing here calls a remote model, runs a solver or invents a second physics
//! model: the result is an editable [`crate::source_scene::SourceScene`] plus
//! quantities to measure. Unsupported distributions return a model with an
//! explicit limitation and the existing formula path instead of a guess.

use std::fmt;

use glam::DVec3;

use crate::plot::PlotKind;
use crate::source_scene::{FieldMode, SourceEntry, SourceKindTag, SourceScene};
use crate::sources::SourceKind;
use crate::viewmodel::State;

/// The shared #168 input bound, in UTF-8 bytes.
pub const MAX_PROMPT_BYTES: usize = 512;
/// Normalized teaching default for an unspecified dipole separation.
pub const DEFAULT_SEPARATION: f64 = 1.0;
/// Normalized teaching default for an unspecified charge magnitude.
pub const DEFAULT_MAGNITUDE: f64 = 1.0;
/// The source-scene evaluator's default softening length (world units).
pub const DEFAULT_SOFTENING: f64 = 0.1;

/// The coordinate frame stated on every interpretation.
pub const FRAME_NOTE: &str =
    "Cartesian frame: +x right, +y up, +z out of the screen (right-handed).";
/// The unit convention stated on every interpretation.
pub const UNITS_NOTE: &str = "Normalized teaching units: the Coulomb constant is 1 and lengths are world units; these are not SI measurements.";
/// The existing formula route for text this grammar cannot represent.
pub const FORMULA_PATH: &str = "Formula path: open the Formula panel and enter the vector field directly, for example E = (x, y, 0), or a density such as ρ(x, y, z); build any other source from the Sources panel.";

/// One Cartesian axis of a dipole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// Every accepted axis, in x/y/z order.
    pub const ALL: [Self; 3] = [Self::X, Self::Y, Self::Z];

    /// The lowercase axis name used in prompts and previews.
    pub fn label(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }

    /// The unit vector along this axis.
    pub fn unit(self) -> DVec3 {
        match self {
            Self::X => DVec3::X,
            Self::Y => DVec3::Y,
            Self::Z => DVec3::Z,
        }
    }
}

/// A requested measurement quantity. These are labels, not a solver run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantity {
    Field,
    Potential,
    Force,
    Flux,
}

impl Quantity {
    /// The lowercase label shown in the preview.
    pub fn label(self) -> &'static str {
        match self {
            Self::Field => "field",
            Self::Potential => "potential",
            Self::Force => "force",
            Self::Flux => "flux",
        }
    }
}

/// Whether the interpretation is ideal, softened, or both.
///
/// The displayed equations are the ideal analytic form; the shipped source
/// evaluator always samples with a finite softening length. The two are kept
/// explicit so a softened peak is never graded against the ideal one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Convention {
    /// Ideal analytic equations sampled with the source scene's softening.
    Softened { length: f64 },
}

impl Convention {
    /// Whether the sampled curve is softened.
    pub fn is_softened(self) -> bool {
        matches!(self, Self::Softened { .. })
    }

    /// The sampling softening length, if any.
    pub fn length(self) -> Option<f64> {
        match self {
            Self::Softened { length } => Some(length),
        }
    }

    /// A softened sample is never graded against the ideal analytic peak.
    pub fn grades_against_ideal(self) -> bool {
        false
    }

    /// The one-line convention statement shown in the preview.
    pub fn note(self) -> String {
        match self {
            Self::Softened { length } => format!(
                "Convention: the equations are the ideal 1/r² analytic form; the sampled \
                 source field is softened with length {length}. A softened sample is never \
                 graded against the ideal peak."
            ),
        }
    }
}

/// A charge magnitude: an optional symbol plus its editable normalized value.
#[derive(Clone, Debug, PartialEq)]
pub struct Magnitude {
    /// The symbolic name, such as `q`, when the prompt left the magnitude open.
    pub symbol: Option<String>,
    /// The editable normalized value used by the scene.
    pub value: f64,
    /// Whether the value is the labelled normalized default rather than parsed input.
    pub normalized_default: bool,
}

impl Magnitude {
    /// Whether the value is the labelled normalized default.
    pub fn is_normalized_default(&self) -> bool {
        self.normalized_default
    }

    /// The label shown in the source list and scene summary.
    pub fn label(&self) -> String {
        match (&self.symbol, self.normalized_default) {
            (Some(symbol), true) => {
                format!("{symbol} (normalized default {})", self.value)
            }
            (Some(symbol), false) => format!("{symbol} = {}", self.value),
            (None, true) => format!("normalized default {}", self.value),
            (None, false) => format!("{}", self.value),
        }
    }
}

/// A dipole separation: an optional symbol plus its editable normalized value.
#[derive(Clone, Debug, PartialEq)]
pub struct Separation {
    /// The symbolic name, such as `d`, when the prompt left the distance open.
    pub symbol: Option<String>,
    /// The editable normalized value used to place both charges.
    pub value: f64,
}

impl Separation {
    /// The label shown in the scene summary.
    pub fn label(&self) -> String {
        match &self.symbol {
            Some(symbol) => format!("{symbol} = {}", self.value),
            None => format!("{}", self.value),
        }
    }
}

/// The role a source plays in the interpreted model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRole {
    /// The single point charge at the origin.
    Point,
    /// The positive end of the dipole pair.
    DipolePositive,
    /// The negative end of the dipole pair.
    DipoleNegative,
}

/// One editable point-charge specification.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceSpec {
    pub role: SourceRole,
    /// `+1` or `-1`; explicit signs are preserved rather than normalized away.
    pub sign: i8,
    pub magnitude: Magnitude,
    pub position: DVec3,
}

impl SourceSpec {
    /// The signed charge used by the source scene.
    pub fn charge(&self) -> f64 {
        f64::from(self.sign) * self.magnitude.value
    }
}

/// A concrete question the model asks before it can apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Question {
    /// Which end of the dipole along `axis` carries the positive charge?
    DipoleOrientation { axis: Axis },
    /// The dipole axis is unnamed.
    DipoleAxis,
}

impl Question {
    /// The full question shown in the preview.
    pub fn text(&self) -> String {
        match self {
            Self::DipoleOrientation { axis } => format!(
                "Which end of the dipole along {} carries the positive charge? Choose +{} or -{}.",
                axis.label(),
                axis.label(),
                axis.label()
            ),
            Self::DipoleAxis => {
                "Along which axis does the dipole lie? Choose x, y or z.".to_owned()
            }
        }
    }
}

/// Which end of the dipole carries the positive charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DipoleEnd {
    /// Positive charge on the positive axis side.
    PositiveAxis,
    /// Positive charge on the negative axis side.
    NegativeAxis,
}

/// The shared geometry of the dipole pair.
#[derive(Clone, Debug, PartialEq)]
pub struct DipoleGeometry {
    pub axis: Axis,
    pub separation: Separation,
    pub positive_end: DipoleEnd,
}

/// The editable interpretation of one prompt.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptModel {
    pub frame: String,
    pub units: String,
    pub convention: Convention,
    pub sources: Vec<SourceSpec>,
    pub requested: Vec<Quantity>,
    pub ambiguities: Vec<Question>,
    pub limitations: Vec<String>,
    pub dipole: Option<DipoleGeometry>,
    /// The whitespace-normalised prompt the parser accepted.
    pub normalized: String,
}

impl Default for PromptModel {
    fn default() -> Self {
        Self {
            frame: FRAME_NOTE.to_owned(),
            units: UNITS_NOTE.to_owned(),
            convention: Convention::Softened {
                length: DEFAULT_SOFTENING,
            },
            sources: Vec::new(),
            requested: Vec::new(),
            ambiguities: Vec::new(),
            limitations: Vec::new(),
            dipole: None,
            normalized: String::new(),
        }
    }
}

impl PromptModel {
    /// A model may apply only when it is supported, unambiguous and non-empty.
    pub fn can_apply(&self) -> bool {
        self.ambiguities.is_empty() && self.limitations.is_empty() && !self.sources.is_empty()
    }

    /// Edit the dipole separation and re-derive both positions from the same model.
    pub fn set_separation(&mut self, value: f64) -> Result<(), PromptModelError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(PromptModelError::BadValue(format!(
                "separation must be a finite positive number, got {value}"
            )));
        }
        let Some(dipole) = self.dipole.as_mut() else {
            return Err(PromptModelError::NoModel);
        };
        dipole.separation.value = value;
        self.place_dipole();
        Ok(())
    }

    /// Answer a concrete question. Returns whether the answer matched.
    pub fn resolve_question(&mut self, question: &Question, answer: DipoleEnd) -> bool {
        if !self.ambiguities.contains(question) {
            return false;
        }
        let Some(dipole) = self.dipole.as_mut() else {
            return false;
        };
        match question {
            Question::DipoleOrientation { axis } if *axis == dipole.axis => {
                dipole.positive_end = answer;
            }
            _ => return false,
        }
        self.ambiguities.retain(|pending| pending != question);
        self.place_dipole();
        true
    }

    /// Answer the axis question. Returns whether the answer matched.
    pub fn resolve_axis(&mut self, axis: Axis) -> bool {
        let Some(dipole) = self.dipole.as_mut() else {
            return false;
        };
        dipole.axis = axis;
        // A new axis keeps the orientation question if it was still open.
        if let Some(Question::DipoleOrientation { axis: pending }) = self
            .ambiguities
            .iter_mut()
            .find(|question| matches!(question, Question::DipoleOrientation { .. }))
        {
            *pending = axis;
        }
        self.ambiguities
            .retain(|question| !matches!(question, Question::DipoleAxis));
        if !self
            .ambiguities
            .iter()
            .any(|question| matches!(question, Question::DipoleOrientation { .. }))
        {
            self.ambiguities.push(Question::DipoleOrientation { axis });
        }
        self.place_dipole();
        true
    }

    fn place_dipole(&mut self) {
        let Some(dipole) = self.dipole.clone() else {
            return;
        };
        let half = dipole.separation.value / 2.0;
        let positive = match dipole.positive_end {
            DipoleEnd::PositiveAxis => dipole.axis.unit() * half,
            DipoleEnd::NegativeAxis => dipole.axis.unit() * -half,
        };
        for source in &mut self.sources {
            match source.role {
                SourceRole::Point => {}
                SourceRole::DipolePositive => source.position = positive,
                SourceRole::DipoleNegative => source.position = -positive,
            }
        }
    }

    /// The equation lines shown in the preview.
    pub fn equations(&self) -> Vec<String> {
        if self.sources.is_empty() {
            return Vec::new();
        }
        let mut field = String::from("E(r) = ");
        let mut potential = String::from("V(r) = ");
        for (index, source) in self.sources.iter().enumerate() {
            let sign = if source.sign >= 0 { "+" } else { "-" };
            let magnitude = source
                .magnitude
                .symbol
                .clone()
                .unwrap_or_else(|| format!("{}", source.magnitude.value));
            let label = index + 1;
            if index > 0 {
                field.push(' ');
                potential.push(' ');
            }
            field.push_str(&format!("{sign}{magnitude} (r - r{label})/|r - r{label}|³"));
            potential.push_str(&format!("{sign}{magnitude}/|r - r{label}|"));
        }
        vec![field, potential]
    }

    /// The scene summary shown in the preview.
    pub fn scene_summary(&self) -> String {
        let mut summary = String::from("Scene: ");
        if self.sources.is_empty() {
            summary.push_str("no supported source");
        } else {
            for (index, source) in self.sources.iter().enumerate() {
                if index > 0 {
                    summary.push_str("; ");
                }
                let sign = if source.sign >= 0 { '+' } else { '-' };
                summary.push_str(&format!(
                    "{sign}{} at ({:.3}, {:.3}, {:.3})",
                    source.magnitude.label(),
                    source.position.x,
                    source.position.y,
                    source.position.z
                ));
            }
        }
        if let Some(dipole) = &self.dipole {
            summary.push_str(&format!(
                "; separation {} along {}",
                dipole.separation.label(),
                dipole.axis.label()
            ));
        }
        summary
    }

    /// The existing formula route for an unsupported prompt.
    pub fn formula_path(&self) -> String {
        FORMULA_PATH.to_owned()
    }
}

/// Why a prompt could not be parsed or applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptModelError {
    Empty,
    TooLong,
    NoModel,
    Ambiguous(String),
    Unsupported(String),
    InvalidScene(String),
    BadValue(String),
}

impl fmt::Display for PromptModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the prompt is empty"),
            Self::TooLong => write!(f, "the prompt exceeds the {MAX_PROMPT_BYTES}-byte limit"),
            Self::NoModel => f.write_str("there is no interpreted model to apply"),
            Self::Ambiguous(reason) => write!(f, "the interpretation is ambiguous: {reason}"),
            Self::Unsupported(reason) => write!(f, "unsupported prompt: {reason}"),
            Self::InvalidScene(reason) => write!(f, "the interpreted scene is invalid: {reason}"),
            Self::BadValue(reason) => write!(f, "invalid value: {reason}"),
        }
    }
}

impl std::error::Error for PromptModelError {}

/// Everything the preview panel shows before anything is applied.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    pub equations: Vec<String>,
    pub scene_summary: String,
    pub frame: String,
    pub units: String,
    pub convention: String,
    pub requested: Vec<String>,
    pub questions: Vec<String>,
    pub limitations: Vec<String>,
    pub can_apply: bool,
    pub formula_path: String,
}

/// Parse one bounded, offline electrostatics prompt.
pub fn parse_prompt(input: &str) -> Result<PromptModel, PromptModelError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(PromptModelError::Empty);
    }
    if input.len() > MAX_PROMPT_BYTES {
        return Err(PromptModelError::TooLong);
    }
    let normalized = normalize(input);
    let lower = normalized.to_ascii_lowercase();
    let mut model = PromptModel {
        normalized: normalized.clone(),
        requested: requested_quantities(&lower),
        ..PromptModel::default()
    };

    if looks_like_dipole(&lower) {
        fill_dipole(&mut model, &lower);
    } else if looks_like_point_charge(&lower) {
        fill_point_charge(&mut model, &lower);
    } else {
        model.limitations.push(unsupported_limitation(&normalized));
    }
    Ok(model)
}

/// The pure preview of one interpreted model.
pub fn preview(model: &PromptModel) -> Preview {
    Preview {
        equations: model.equations(),
        scene_summary: model.scene_summary(),
        frame: model.frame.clone(),
        units: model.units.clone(),
        convention: model.convention.note(),
        requested: model
            .requested
            .iter()
            .map(|quantity| format!("{} (requested)", quantity.label()))
            .collect(),
        questions: model.ambiguities.iter().map(Question::text).collect(),
        limitations: model.limitations.clone(),
        can_apply: model.can_apply(),
        formula_path: model.formula_path(),
    }
}

/// Apply the interpreted model through the ordinary source-scene setters.
///
/// Every check runs before the first mutation, so a refused model leaves the
/// document, dirty flag and edit history untouched.
pub fn apply(model: &PromptModel, state: &mut State) -> Result<(), PromptModelError> {
    if !model.ambiguities.is_empty() {
        return Err(PromptModelError::Ambiguous(model.ambiguities[0].text()));
    }
    if !model.limitations.is_empty() || model.sources.is_empty() {
        return Err(PromptModelError::Unsupported(
            model
                .limitations
                .first()
                .cloned()
                .unwrap_or_else(|| "no supported source was recognised".to_owned()),
        ));
    }

    let mut scene = SourceScene::default();
    if let Some(length) = model.convention.length() {
        scene
            .set_softening(length)
            .map_err(|error| PromptModelError::InvalidScene(format!("{error:?}")))?;
    }
    scene.entries = model
        .sources
        .iter()
        .map(|spec| {
            let mut source = SourceKindTag::PointCharge.default_source();
            source.center = spec.position;
            source.kind = SourceKind::PointCharge {
                charge: spec.charge(),
            };
            SourceEntry {
                enabled: true,
                source,
            }
        })
        .collect();
    scene
        .validate()
        .map_err(|error| PromptModelError::InvalidScene(format!("{error:?}")))?;

    state.set_plot_kind(PlotKind::VectorField);
    state.set_field_mode(FieldMode::Sources);
    state.set_mode(true);
    state.set_source_scene(scene);
    Ok(())
}

fn normalize(input: &str) -> String {
    let unified: String = input
        .chars()
        .map(|ch| match ch {
            '−' | '–' => '-',
            '’' => '\'',
            _ => ch,
        })
        .collect();
    unified.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn looks_like_dipole(lower: &str) -> bool {
    if lower.contains("dipole") {
        return true;
    }
    let pair = lower.contains("two") || lower.contains("pair");
    pair && lower.contains("charge") && (lower.contains("opposite") || lower.contains("equal"))
}

fn looks_like_point_charge(lower: &str) -> bool {
    lower.contains("charge") && lower.contains("origin")
}

fn requested_quantities(lower: &str) -> Vec<Quantity> {
    let mut requested = Vec::new();
    for (needle, quantity) in [
        ("field", Quantity::Field),
        ("potential", Quantity::Potential),
        ("force", Quantity::Force),
        ("flux", Quantity::Flux),
    ] {
        if lower.contains(needle) {
            requested.push(quantity);
        }
    }
    requested
}

fn fill_point_charge(model: &mut PromptModel, lower: &str) {
    let sign = if lower.contains("negative") { -1 } else { 1 };
    model.sources.push(SourceSpec {
        role: SourceRole::Point,
        sign,
        magnitude: magnitude_in(lower),
        position: DVec3::ZERO,
    });
}

fn fill_dipole(model: &mut PromptModel, lower: &str) {
    let axis = axis_in(lower);
    let separation = separation_in(lower);
    let magnitude = magnitude_in(lower);
    let positive_end = axis
        .and_then(|axis| orientation_in(lower, axis))
        .unwrap_or(DipoleEnd::PositiveAxis);
    let dipole_axis = axis.unwrap_or(Axis::X);
    model.dipole = Some(DipoleGeometry {
        axis: dipole_axis,
        separation,
        positive_end,
    });
    model.sources.push(SourceSpec {
        role: SourceRole::DipolePositive,
        sign: 1,
        magnitude: magnitude.clone(),
        position: DVec3::ZERO,
    });
    model.sources.push(SourceSpec {
        role: SourceRole::DipoleNegative,
        sign: -1,
        magnitude,
        position: DVec3::ZERO,
    });
    model.place_dipole();

    if axis.is_none() {
        model.ambiguities.push(Question::DipoleAxis);
    }
    if let Some(axis) = axis
        && orientation_in(lower, axis).is_none()
    {
        model.ambiguities.push(Question::DipoleOrientation { axis });
    }
}

fn magnitude_in(lower: &str) -> Magnitude {
    if let Some(value) = explicit_number(lower, &["q", "charge", "magnitude"]) {
        return Magnitude {
            symbol: Some("q".to_owned()),
            value,
            normalized_default: false,
        };
    }
    Magnitude {
        symbol: Some("q".to_owned()),
        value: DEFAULT_MAGNITUDE,
        normalized_default: true,
    }
}

fn explicit_number(lower: &str, names: &[&str]) -> Option<f64> {
    let compact: String = lower.chars().filter(|ch| !ch.is_whitespace()).collect();
    for name in names {
        let needle = format!("{name}=");
        if let Some(index) = compact.find(&needle) {
            let tail = &compact[index + needle.len()..];
            let number: String = tail
                .chars()
                .take_while(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == '-')
                .collect();
            if let Ok(value) = number.parse::<f64>()
                && value.is_finite()
                && value > 0.0
            {
                return Some(value);
            }
        }
    }
    None
}

fn axis_in(lower: &str) -> Option<Axis> {
    for axis in Axis::ALL {
        let label = axis.label();
        let patterns = [
            format!("{label} axis"),
            format!("{label}-axis"),
            format!("along {label}"),
            format!("along the {label}"),
            format!("on the {label}"),
            format!("{label} direction"),
            format!("{label}="),
            format!("{label} ="),
        ];
        if patterns.iter().any(|pattern| lower.contains(pattern)) {
            return Some(axis);
        }
    }
    None
}

fn separation_in(lower: &str) -> Separation {
    if lower.contains("d/2") {
        return Separation {
            symbol: Some("d".to_owned()),
            value: DEFAULT_SEPARATION,
        };
    }
    for marker in [
        "separated by ",
        "separation of ",
        "separation ",
        "distance of ",
        "distance ",
    ] {
        if let Some(index) = lower.find(marker) {
            let tail = &lower[index + marker.len()..];
            let Some(token) = tail.split_whitespace().next() else {
                continue;
            };
            let token = token.trim_matches(|ch: char| {
                ch == ',' || ch == '.' || ch == ';' || ch == ':' || ch == ')' || ch == ']'
            });
            if token.is_empty() {
                continue;
            }
            if let Ok(value) = token.parse::<f64>()
                && value.is_finite()
                && value > 0.0
            {
                return Separation {
                    symbol: None,
                    value,
                };
            }
            let clean = token.trim_matches(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'));
            if !clean.is_empty() {
                return Separation {
                    symbol: Some(clean.to_owned()),
                    value: DEFAULT_SEPARATION,
                };
            }
        }
    }
    Separation {
        symbol: Some("d".to_owned()),
        value: DEFAULT_SEPARATION,
    }
}

fn orientation_in(lower: &str, axis: Axis) -> Option<DipoleEnd> {
    let label = axis.label();
    for (word, positive_here) in [("positive", true), ("negative", false)] {
        for marker in [
            format!("{word} charge at"),
            format!("{word} charge is at"),
            format!("{word} at"),
        ] {
            let Some(index) = lower.find(&marker) else {
                continue;
            };
            let tail = lower[index + marker.len()..].trim_start();
            let negative = tail.starts_with('-')
                || tail.starts_with("negative")
                || tail.starts_with(&format!("-{label}"))
                || tail.starts_with(&format!("{label} = -"))
                || tail.starts_with(&format!("{label}=-"))
                || tail.starts_with(&format!("{label} -"));
            let positive = tail.starts_with('+')
                || tail.starts_with("positive")
                || tail.starts_with(&format!("+{label}"))
                || tail.starts_with(&format!("{label} = +"))
                || tail.starts_with(&format!("{label}=+"))
                || tail.starts_with(&format!("{label} +"));
            if negative {
                return Some(if positive_here {
                    DipoleEnd::NegativeAxis
                } else {
                    DipoleEnd::PositiveAxis
                });
            }
            if positive {
                return Some(if positive_here {
                    DipoleEnd::PositiveAxis
                } else {
                    DipoleEnd::NegativeAxis
                });
            }
        }
    }
    None
}

fn unsupported_limitation(normalized: &str) -> String {
    format!(
        "Unsupported charge distribution `{normalized}`. This offline grammar only builds a \
         single point charge at the origin or two equal/opposite point charges (a dipole) along \
         x, y or z. A general distribution needs the existing editable formula path."
    )
}
