//! Pure, searchable catalogue over the existing built-in presets.
//!
//! This module deliberately owns no physics and no UI state. It indexes the
//! formula presets from [`crate::presets`] and the source scenes from
//! [`crate::source_presets`] with stable [`PresetId`]s and the metadata later
//! panel work needs: a display name, summary, group, filter fields, documented
//! assumptions, and (eventually) named parameters and exploration prompts.
//!
//! Ordering is deterministic: formula presets follow [`presets()`] order and
//! source presets follow [`source_presets()`] order. IDs are unique across the
//! whole catalogue; construction fails rather than overwriting a duplicate.

use std::collections::HashSet;
use std::fmt;

use crate::plot::PlotKind;
use crate::presets::{Preset, PresetCategory, preset_approximation_label, presets};
use crate::source_presets::{SourcePreset, source_presets};

/// The longest accepted [`PresetId`], in bytes.
pub const PRESET_ID_MAX_BYTES: usize = 64;

/// The source preset that places one of every source kind; the only entry in
/// [`PresetGroup::Gallery`].
const SOURCE_GALLERY_NAME: &str = "Source gallery";

/// A validated, stable identifier for one catalogue entry.
///
/// The serialized form is the ID itself: ASCII `[a-z0-9]` first, then
/// `[a-z0-9_.-]`, 1..=[`PRESET_ID_MAX_BYTES`] bytes. Formula entries look like
/// `formula.vortex`; source entries look like `source.coulomb_point`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PresetId(String);

impl PresetId {
    /// Validate and wrap an ID. Never panics; malformed input returns a
    /// descriptive [`PresetIdError`].
    pub fn new(value: &str) -> Result<Self, PresetIdError> {
        if value.is_empty() {
            return Err(PresetIdError::Empty);
        }
        if value.len() > PRESET_ID_MAX_BYTES {
            return Err(PresetIdError::TooLong { bytes: value.len() });
        }
        if !value.is_ascii() {
            return Err(PresetIdError::NonAscii);
        }

        let first = value.as_bytes()[0] as char;
        if !is_id_start(first) {
            return Err(PresetIdError::InvalidFirst { character: first });
        }
        for byte in &value.as_bytes()[1..] {
            let character = *byte as char;
            if !is_id_continue(character) {
                return Err(PresetIdError::InvalidCharacter { character });
            }
        }

        Ok(Self(value.to_owned()))
    }

    /// The validated ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PresetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for PresetId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn is_id_start(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit()
}

fn is_id_continue(character: char) -> bool {
    is_id_start(character) || matches!(character, '_' | '.' | '-')
}

/// Why a candidate [`PresetId`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetIdError {
    /// The candidate was empty.
    Empty,
    /// The candidate exceeded [`PRESET_ID_MAX_BYTES`].
    TooLong { bytes: usize },
    /// The candidate was not ASCII.
    NonAscii,
    /// The first byte was not a lowercase letter or digit.
    InvalidFirst { character: char },
    /// A later byte was outside `[a-z0-9_.-]`.
    InvalidCharacter { character: char },
}

impl fmt::Display for PresetIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("preset id must not be empty"),
            Self::TooLong { bytes } => write!(
                formatter,
                "preset id must be at most {PRESET_ID_MAX_BYTES} bytes, got {bytes}"
            ),
            Self::NonAscii => formatter.write_str("preset id must be ASCII"),
            Self::InvalidFirst { character } => write!(
                formatter,
                "preset id must start with a lowercase letter or digit, got `{character}`"
            ),
            Self::InvalidCharacter { character } => write!(
                formatter,
                "preset id contains an invalid character `{character}`; \
                 allowed characters are a-z, 0-9, `_`, `.` and `-`"
            ),
        }
    }
}

impl std::error::Error for PresetIdError {}

/// The catalogue section an entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresetGroup {
    /// Kinematic demos and generic flows from [`PresetCategory::Classic`].
    Classic,
    /// Electrical and magnetic physics formulas.
    Physics,
    /// Source-scene presets.
    Sources,
    /// The source gallery (the last source preset).
    Gallery,
}

/// The honesty label attached to a teaching preset where the catalogue makes
/// the distinction. Legacy entries predate the labels and carry `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetValidity {
    /// A kinematic or mathematical construction, not a physical field.
    MathematicalIllustration,
    /// A physical model in normalized teaching units.
    PhysicallyValid,
    /// A physical model regularized by softening so its singular centre stays
    /// finite.
    PhysicallyValidRegularized,
}

impl PresetValidity {
    /// The user-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::MathematicalIllustration => "Mathematical illustration",
            Self::PhysicallyValid => "Physically valid",
            Self::PhysicallyValidRegularized => "Physically valid (regularized)",
        }
    }
}

/// Filter fields carried by every entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PresetFilters {
    /// Formula dimension; `None` for dimension-agnostic source scenes.
    pub dimension: Option<usize>,
    /// Whether the preset animates over time; only clock-driven formula
    /// entries set this.
    pub animated: bool,
}

/// A surface-context scalar expression carried by a catalogue row.
///
/// The scalar view is the shipped [`PlotKind::Surface3D`] workbench: `kind`
/// names the scalar plot the row switches to, `show_heatmap` and
/// `contour_levels` choose the existing heatmap/contour controls. Loading a row
/// with a spec never touches the vector formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalarPresetSpec {
    /// Surface-context scalar expression over `x`, `y` and `t`.
    pub expr: &'static str,
    /// The shipped scalar view kind the row switches to.
    pub kind: PlotKind,
    /// Whether the filled heatmap is painted.
    pub show_heatmap: bool,
    /// Explicit contour-level count.
    pub contour_levels: u32,
}

/// A catalogue prompt that has a working, clickable destination. Only prompts
/// with a tested destination are listed; every other `exploration_prompt`
/// stays informational text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorationActivity {
    /// Issue #218 session-only closed-box flux investigation.
    UniformFlux,
}

/// The promoted activity for a catalogue entry, if its prompt is clickable.
pub fn exploration_activity(name: &str) -> Option<ExplorationActivity> {
    match name {
        "Uniform field" => Some(ExplorationActivity::UniformFlux),
        _ => None,
    }
}

/// One indexed preset.
#[derive(Debug, Clone)]
pub struct PresetEntry {
    /// Stable, unique ID.
    pub id: PresetId,
    /// Display name from the owning catalogue.
    pub name: &'static str,
    /// One-line explanation; empty when the catalogue has none.
    pub summary: &'static str,
    /// Catalogue section.
    pub group: PresetGroup,
    /// Filter metadata.
    pub filters: PresetFilters,
    /// Documented approximation, empty when the preset makes no claim.
    pub assumptions: &'static str,
    /// Named parameter bindings; empty until a preset uses them.
    pub parameters: &'static [&'static str],
    /// Optional exploration prompt; empty when none is supplied.
    pub exploration_prompt: &'static str,
    /// Mathematical-illustration or physically-valid label, when catalogued.
    pub validity: Option<PresetValidity>,
    /// Surface-context scalar twin for a row that loads the scalar workbench;
    /// `None` keeps the ordinary vector/source load path.
    pub scalar: Option<ScalarPresetSpec>,
}

/// Why a [`PresetLibrary`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetLibraryError {
    /// A catalogue name produced an invalid ID.
    InvalidId {
        /// The preset name that produced the ID.
        preset: &'static str,
        /// The validation failure.
        error: PresetIdError,
    },
    /// The same ID appeared twice.
    DuplicateId(PresetId),
}

impl fmt::Display for PresetLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId { preset, error } => {
                write!(formatter, "preset `{preset}` has an invalid id: {error}")
            }
            Self::DuplicateId(id) => write!(formatter, "duplicate preset id `{id}`"),
        }
    }
}

impl std::error::Error for PresetLibraryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidId { error, .. } => Some(error),
            Self::DuplicateId(_) => None,
        }
    }
}

/// An ordered, indexed collection of presets.
#[derive(Debug, Clone)]
pub struct PresetLibrary {
    entries: Vec<PresetEntry>,
}

impl PresetLibrary {
    /// Build the catalogue over the current built-in formula and source
    /// presets. Fails only if a name cannot form a valid ID or two entries
    /// collide.
    pub fn builtin() -> Result<Self, PresetLibraryError> {
        let mut entries =
            Vec::with_capacity(presets().len() + SCALAR_PRESETS.len() + source_presets().len());
        for preset in presets() {
            entries.push(formula_entry(preset)?);
        }
        for metadata in &SCALAR_PRESETS {
            entries.push(scalar_entry(metadata)?);
        }
        for preset in source_presets() {
            entries.push(source_entry(preset)?);
        }
        Self::from_entries(entries)
    }

    /// Build from an explicit entry list in catalogue order, rejecting
    /// duplicate IDs.
    pub fn from_entries(entries: Vec<PresetEntry>) -> Result<Self, PresetLibraryError> {
        let mut seen = HashSet::with_capacity(entries.len());
        for entry in &entries {
            if !seen.insert(entry.id.clone()) {
                return Err(PresetLibraryError::DuplicateId(entry.id.clone()));
            }
        }
        Ok(Self { entries })
    }

    /// All entries in catalogue order.
    pub fn entries(&self) -> &[PresetEntry] {
        &self.entries
    }

    /// Number of indexed presets.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalogue is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries whose name, summary or ID contains `query`, trimmed and
    /// case-insensitively. A blank query returns every entry. Results preserve
    /// catalogue order.
    pub fn search(&self, query: &str) -> Vec<&PresetEntry> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return self.entries.iter().collect();
        }
        self.entries
            .iter()
            .filter(|entry| {
                entry.name.to_ascii_lowercase().contains(&needle)
                    || entry.summary.to_ascii_lowercase().contains(&needle)
                    || entry.id.as_str().contains(&needle)
            })
            .collect()
    }

    /// Entries matching the filter fields. `dimension: Some(n)` keeps formulas
    /// of dimension `n` and every dimension-agnostic source; `animated: true`
    /// keeps only animated entries, while `false` applies no animated filter.
    pub fn filtered(&self, filters: PresetFilters) -> Vec<&PresetEntry> {
        self.entries
            .iter()
            .filter(|entry| matches_filters(entry, filters))
            .collect()
    }

    /// [`Self::search`] narrowed by [`Self::filtered`], equivalent to
    /// filtering the search results by the same predicates.
    pub fn search_filtered(&self, query: &str, filters: PresetFilters) -> Vec<&PresetEntry> {
        self.search(query)
            .into_iter()
            .filter(|entry| matches_filters(entry, filters))
            .collect()
    }

    /// The entry with `id`, if indexed.
    pub fn get(&self, id: &PresetId) -> Option<&PresetEntry> {
        self.entries.iter().find(|entry| entry.id == *id)
    }
}

fn matches_filters(entry: &PresetEntry, filters: PresetFilters) -> bool {
    let dimension_matches = match filters.dimension {
        Some(dimension) => {
            entry.filters.dimension.is_none() || entry.filters.dimension == Some(dimension)
        }
        None => true,
    };
    let animated_matches = !filters.animated || entry.filters.animated;
    dimension_matches && animated_matches
}

/// Catalogue metadata the formula-preset table does not itself carry. Kept
/// beside the indexed library so the formula catalogue stays a pure list of
/// formulas and the metadata remains the library's concern.
#[derive(Debug, Clone, Copy)]
struct FormulaMetadata {
    /// Explicit ID when the design fixes one; `None` uses the derived slug.
    id: Option<&'static str>,
    summary: &'static str,
    assumptions: &'static str,
    exploration_prompt: &'static str,
    animated: bool,
    validity: Option<PresetValidity>,
}

/// Catalogue metadata for the new source entries; the summary stays on the
/// source preset itself.
#[derive(Debug, Clone, Copy)]
struct SourceMetadata {
    assumptions: &'static str,
    exploration_prompt: &'static str,
    validity: Option<PresetValidity>,
}

/// One pure scalar catalogue row. The surface expression and view controls
/// come from [`scalar_metadata`], mirroring the formula/source metadata split.
#[derive(Debug, Clone, Copy)]
struct ScalarPresetMetadata {
    name: &'static str,
    summary: &'static str,
    assumptions: &'static str,
    exploration_prompt: &'static str,
    animated: bool,
    validity: PresetValidity,
}

/// The ticket-91 slice C scalar rows, in catalogue order after the vector
/// formulas and before the source scenes.
const SCALAR_PRESETS: [ScalarPresetMetadata; 5] = [
    ScalarPresetMetadata {
        name: "Scalar saddle",
        summary: "A saddle height field whose gradient is the Saddle potential",
        assumptions: "Mathematical illustration: the scalar height (x^2 - y^2)/2; its gradient is the shipped Saddle potential field (x, -y)",
        exploration_prompt: "Trace the two rising and two falling directions that cross at the saddle point.",
        animated: false,
        validity: PresetValidity::MathematicalIllustration,
    },
    ScalarPresetMetadata {
        name: "Scalar anisotropic well",
        summary: "An anisotropic harmonic bowl",
        assumptions: "Mathematical illustration: uncoupled harmonic well with stiffness 4 along the y axis",
        exploration_prompt: "Compare the contour spacing along x with the stiffer y axis.",
        animated: false,
        validity: PresetValidity::MathematicalIllustration,
    },
    ScalarPresetMetadata {
        name: "Softened dipole potential",
        summary: "Regularized potential of an opposite charge pair",
        assumptions: "Physically valid (regularized): +1 and -1 unit charges at (0.75, 0) and (-0.75, 0); each softened distance includes the in-plane y offset with s^2 = 0.0625, so both centres stay finite; normalized teaching units, not an SI potential",
        exploration_prompt: "See how softening keeps both charge centres finite while the potential still changes sign across the dipole.",
        animated: false,
        validity: PresetValidity::PhysicallyValidRegularized,
    },
    ScalarPresetMetadata {
        name: "Scalar moving Gaussian packet",
        summary: "A Gaussian packet oscillating along x",
        assumptions: "Mathematical illustration: kinematic Gaussian height whose centre follows 0.5*sin(t); not a physical wavefunction",
        exploration_prompt: "Track the packet centre as time advances and watch it reverse at the turning points.",
        animated: true,
        validity: PresetValidity::MathematicalIllustration,
    },
    ScalarPresetMetadata {
        name: "Interfering waves",
        summary: "Two counter-propagating waves forming a standing pattern",
        assumptions: "Mathematical illustration: sum of two equal counter-propagating sinusoids; a standing-wave interference pattern with no medium or physical calibration",
        exploration_prompt: "Watch the nodes stay fixed while the antinodes alternate with the shared clock.",
        animated: true,
        validity: PresetValidity::MathematicalIllustration,
    },
];

/// The shipped scalar view and controls for a pure scalar row. Only
/// [`PlotKind::Surface3D`] is a surface-context scalar workbench; heatmap and
/// contour visibility stay separate controls on that view.
fn scalar_metadata(name: &str) -> Option<ScalarPresetSpec> {
    let spec = match name {
        "Scalar saddle" => ScalarPresetSpec {
            expr: "(x^2 - y^2)/2",
            kind: PlotKind::Surface3D,
            show_heatmap: true,
            contour_levels: 8,
        },
        "Scalar anisotropic well" => ScalarPresetSpec {
            expr: "x^2 + 4*y^2",
            kind: PlotKind::Surface3D,
            show_heatmap: false,
            contour_levels: 8,
        },
        "Softened dipole potential" => ScalarPresetSpec {
            expr: "1/sqrt((x-0.75)^2+y^2+0.0625) - 1/sqrt((x+0.75)^2+y^2+0.0625)",
            kind: PlotKind::Surface3D,
            show_heatmap: true,
            contour_levels: 8,
        },
        "Scalar moving Gaussian packet" => ScalarPresetSpec {
            expr: "exp(-((x-0.5*sin(t))^2 + y^2)/0.5)",
            kind: PlotKind::Surface3D,
            show_heatmap: true,
            contour_levels: 8,
        },
        "Interfering waves" => ScalarPresetSpec {
            expr: "sin(6*x - 2*t) + sin(6*x + 2*t)",
            kind: PlotKind::Surface3D,
            show_heatmap: true,
            contour_levels: 8,
        },
        _ => return None,
    };
    Some(spec)
}

fn formula_metadata(name: &str) -> Option<FormulaMetadata> {
    let metadata = match name {
        "Pulsating radial field" => FormulaMetadata {
            id: Some("formula.pulsating-radial"),
            summary: "Radial outflow that breathes with the shared clock",
            assumptions: "Mathematical illustration: a radial flow whose strength oscillates; not a physical field",
            exploration_prompt: "Watch how the outward speed grows and shrinks while every direction stays radial.",
            animated: true,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Rotating field" => FormulaMetadata {
            id: Some("formula.rotating-field"),
            summary: "Rigid rotation about the origin",
            assumptions: "Mathematical illustration: constant angular speed with no decay away from the origin",
            exploration_prompt: "Compare the circulation with Vortex: both rotate, but this field never weakens.",
            animated: false,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Traveling wave field" => FormulaMetadata {
            id: Some("formula.traveling-wave-field"),
            summary: "A sinusoidal profile moving along x",
            assumptions: "Mathematical illustration: kinematic one-dimensional wave with no medium or speed calibration",
            exploration_prompt: "Follow one crest as time advances; the pattern translates at unit speed.",
            animated: true,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Standing wave field" => FormulaMetadata {
            id: Some("formula.standing-wave-field"),
            summary: "A spatial sinusoid oscillating in place",
            assumptions: "Mathematical illustration: a stationary sinusoid with no physical medium",
            exploration_prompt: "Find the nodes that never move and the antinodes that reverse every half period.",
            animated: true,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Quadrupole (unequal charges)" => FormulaMetadata {
            id: Some("formula.quadrupole-unequal-charges"),
            summary: "A symmetric quadrupole saddle field",
            assumptions: "Mathematical illustration: the symmetric (x, -y) saddle, not the field of four specified charges",
            exploration_prompt: "Trace the two inflow directions and the two outflow directions meeting at the origin.",
            animated: false,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Saddle potential" => FormulaMetadata {
            id: Some("formula.saddle-potential"),
            summary: "Negative gradient of a saddle-shaped potential",
            assumptions: "Mathematical illustration: potential x^2 - y^2 yields the field (x, -y) up to scale",
            exploration_prompt: "Identify the stable and unstable directions of the saddle.",
            animated: false,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Anisotropic well" => FormulaMetadata {
            id: Some("formula.anisotropic-well"),
            summary: "An anisotropic linear restoring field",
            assumptions: "Mathematical illustration: uncoupled harmonic well with stiffness 4 along y",
            exploration_prompt: "Compare how quickly motion converges along x and along the stiffer y axis.",
            animated: false,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Double-well potential" => FormulaMetadata {
            id: Some("formula.double-well"),
            summary: "Negative gradient of a symmetric double well",
            assumptions: "Mathematical illustration: potential (x^2 - 1)^2 gives -4x^3 + 4x, with a separate linear y well",
            exploration_prompt: "Locate the two stable x wells and the unstable barrier between them.",
            animated: false,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Softened dipole" => FormulaMetadata {
            id: Some("formula.softened-dipole"),
            summary: "Opposite softened unit charges along x",
            assumptions: "Physically valid (regularized): +1 and -1 unit charges at (1.25, 0) and (0.75, 0), each softened with s^2 = 0.0625 so neither centre is singular; a teaching regularization, not an SI field",
            exploration_prompt: "See how softening keeps both charge centres finite while preserving the dipole pattern between and outside them.",
            animated: false,
            validity: Some(PresetValidity::PhysicallyValidRegularized),
        },
        "Moving Gaussian packet" => FormulaMetadata {
            id: Some("formula.moving-gaussian"),
            summary: "A Gaussian packet translating along x",
            assumptions: "Mathematical illustration: kinematic packet derived from a translating Gaussian potential",
            exploration_prompt: "Track the packet centre as time advances.",
            animated: true,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Uniform field" => FormulaMetadata {
            id: Some("formula.uniform-field"),
            summary: "A constant field pointing along x",
            assumptions: "Physically valid: uniform field in normalized teaching units with no spatial variation",
            exploration_prompt: "Compare the flux entering and leaving a closed surface in this uniform field: they cancel, the Gauss's-law result when no charge is enclosed.",
            animated: false,
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Quadrupole field" => FormulaMetadata {
            id: Some("formula.quadrupole-field"),
            summary: "A traceless linear electrostatic quadrupole field",
            assumptions: "Physically valid: linear electrostatic field (x, y, -2z) with zero divergence in normalized teaching units; not the field of four specified charges",
            exploration_prompt: "Trace the inflow along z and the outflow in the xy plane meeting at the origin.",
            animated: false,
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Softened point charge" => FormulaMetadata {
            id: Some("formula.softened-point-charge"),
            summary: "A regularized positive point charge at the origin",
            assumptions: "Physically valid (regularized): unit point charge at the origin softened with s^2 = 0.0625 so the centre stays finite; normalized teaching units, not an SI field",
            exploration_prompt: "Compare the inverse-square falloff away from the origin with the finite value exactly at the charge centre.",
            animated: false,
            validity: Some(PresetValidity::PhysicallyValidRegularized),
        },
        "Helical solenoid field" => FormulaMetadata {
            id: Some("formula.helical-solenoid-field"),
            summary: "Azimuthal winding plus a uniform axial component",
            assumptions: "Physically valid: idealised solenoid field with azimuthal circulation (-y, x) and axial component 0.75 in normalized teaching units; no end effects",
            exploration_prompt: "Follow a field line as it winds around the axis while advancing steadily along z.",
            animated: false,
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "ABC flow" => FormulaMetadata {
            id: Some("formula.abc-flow"),
            summary: "A divergence-free chaotic advection field",
            assumptions: "Mathematical illustration: the ABC kinematic flow is divergence-free but is not an electromagnetic field",
            exploration_prompt: "Release nearby tracers and watch the flow stretch and fold them without any sources or sinks.",
            animated: false,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "3D shear flow" => FormulaMetadata {
            id: Some("formula.3d-shear-flow"),
            summary: "Linear shear transporting along x at speed y",
            assumptions: "Mathematical illustration: kinematic shear flow with no physical medium or forces",
            exploration_prompt: "Compare particle speeds at different heights: each layer slides along x with no vertical motion.",
            animated: false,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "Rotating uniform field" => FormulaMetadata {
            id: Some("formula.rotating-uniform-field"),
            summary: "A uniform field whose direction rotates in the xy plane",
            assumptions: "Mathematical illustration: the direction, not the magnitude, oscillates with the shared clock; not a physical radiating field",
            exploration_prompt: "Watch the arrow direction sweep a full circle while its length stays one.",
            animated: true,
            validity: Some(PresetValidity::MathematicalIllustration),
        },
        "3D plane wave" => FormulaMetadata {
            id: Some("formula.3d-plane-wave"),
            summary: "A plane wave travelling along z",
            assumptions: "Physically valid: unit-amplitude plane wave sin(z - t) polarised along y in normalized teaching units; no SI speed or amplitude calibration",
            exploration_prompt: "Fix z and watch the transverse component oscillate; then follow one crest as it advances along z.",
            animated: true,
            validity: Some(PresetValidity::PhysicallyValid),
        },
        _ => return None,
    };
    Some(metadata)
}

fn source_metadata(name: &str) -> Option<SourceMetadata> {
    let metadata = match name {
        "Electric dipole" => SourceMetadata {
            assumptions: "Physically valid: equal and opposite point charges separated along x; normalized teaching units",
            exploration_prompt: "Treat the mid-plane between the charges as an illustrative conducting boundary: the field crosses it at a right angle, the geometry behind an image-charge construction.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Infinite wire" => SourceMetadata {
            assumptions: "Physically valid: idealised straight current-carrying wire along z; normalized teaching units with no end effects",
            exploration_prompt: "Walk an Ampere loop around the wire at several radii: the circulation stays the same, illustrating Ampere's law.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Unequal charges" => SourceMetadata {
            assumptions: "Physically valid: Coulomb superposition of four unit point charges with alternating signs; normalized teaching units and softened field",
            exploration_prompt: "Find the neutral point where the four charge fields balance.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Off-axis ring" => SourceMetadata {
            assumptions: "Physically valid: uniformly charged unit ring displaced from the origin; normalized teaching units and softening",
            exploration_prompt: "Compare the near-axis field with the centred ring: the axis has moved.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Sphere inside and out" => SourceMetadata {
            assumptions: "Physically valid: uniformly charged solid sphere of unit charge and radius; normalized teaching units",
            exploration_prompt: "Compare the interior field growing with radius to the exterior inverse-square falloff; a conducting sphere would instead shield its interior, so this solid-sphere scene illustrates the boundary only.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Paired loops" => SourceMetadata {
            assumptions: "Physically valid: superposition of two separated coaxial unit-current loops; normalized teaching units",
            exploration_prompt: "Look for the field between the loops, where both contributions join.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Helmholtz pair" => SourceMetadata {
            assumptions: "Physically valid: two coaxial unit-current loops separated by one loop radius; normalized teaching units",
            exploration_prompt: "Check how uniform the axial field is near the midpoint between the loops.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Opposing wires" => SourceMetadata {
            assumptions: "Physically valid: superposition of two parallel unit-current straight wires with opposite currents; normalized teaching units",
            exploration_prompt: "Find where the two magnetic fields cancel between the wires.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        "Solenoid ends" => SourceMetadata {
            assumptions: "Physically valid: finite eight-turn unit-current solenoid; normalized teaching geometry with the canonical domain extending past both ends",
            exploration_prompt: "Compare the axial field near the middle with the spreading field past the ends.",
            validity: Some(PresetValidity::PhysicallyValid),
        },
        _ => return None,
    };
    Some(metadata)
}

fn scalar_entry(
    metadata: &'static ScalarPresetMetadata,
) -> Result<PresetEntry, PresetLibraryError> {
    let spec =
        scalar_metadata(metadata.name).expect("every SCALAR_PRESETS row has a surface scalar spec");
    Ok(PresetEntry {
        id: derive_id("formula", metadata.name)?,
        name: metadata.name,
        summary: metadata.summary,
        group: PresetGroup::Physics,
        filters: PresetFilters {
            dimension: Some(2),
            animated: metadata.animated,
        },
        assumptions: metadata.assumptions,
        parameters: &[],
        exploration_prompt: metadata.exploration_prompt,
        validity: Some(metadata.validity),
        scalar: Some(spec),
    })
}

fn formula_entry(preset: &'static Preset) -> Result<PresetEntry, PresetLibraryError> {
    let metadata = formula_metadata(preset.name);
    let id = match metadata.and_then(|metadata| metadata.id) {
        Some(id) => PresetId::new(id).map_err(|error| PresetLibraryError::InvalidId {
            preset: preset.name,
            error,
        })?,
        None => derive_id("formula", preset.name)?,
    };
    Ok(PresetEntry {
        id,
        name: preset.name,
        summary: metadata.map_or("", |metadata| metadata.summary),
        group: match preset.category {
            PresetCategory::Classic => PresetGroup::Classic,
            PresetCategory::Electrostatics
            | PresetCategory::Magnetostatics
            | PresetCategory::Electrodynamics => PresetGroup::Physics,
        },
        filters: PresetFilters {
            dimension: Some(preset.dimension),
            animated: metadata.is_some_and(|metadata| metadata.animated),
        },
        assumptions: metadata
            .map(|metadata| metadata.assumptions)
            .or_else(|| preset_approximation_label(preset.name))
            .unwrap_or(""),
        parameters: &[],
        exploration_prompt: metadata.map_or("", |metadata| metadata.exploration_prompt),
        validity: metadata.and_then(|metadata| metadata.validity),
        scalar: None,
    })
}

fn source_entry(preset: &'static SourcePreset) -> Result<PresetEntry, PresetLibraryError> {
    let metadata = source_metadata(preset.name);
    let id = derive_id("source", preset.name)?;
    let group = if preset.name == SOURCE_GALLERY_NAME {
        PresetGroup::Gallery
    } else {
        PresetGroup::Sources
    };
    Ok(PresetEntry {
        id,
        name: preset.name,
        summary: preset.summary,
        group,
        filters: PresetFilters {
            dimension: None,
            animated: false,
        },
        assumptions: metadata.map_or("", |metadata| metadata.assumptions),
        parameters: &[],
        exploration_prompt: metadata.map_or("", |metadata| metadata.exploration_prompt),
        validity: metadata.and_then(|metadata| metadata.validity),
        scalar: None,
    })
}

fn derive_id(namespace: &str, name: &'static str) -> Result<PresetId, PresetLibraryError> {
    let candidate = format!("{namespace}.{}", slug(name));
    PresetId::new(&candidate).map_err(|error| PresetLibraryError::InvalidId {
        preset: name,
        error,
    })
}

/// Lowercase `name`, replacing each run of non-`[a-z0-9]` bytes with `_`.
fn slug(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut in_separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            in_separator = false;
        } else if !in_separator {
            slug.push('_');
            in_separator = true;
        }
    }
    slug
}
