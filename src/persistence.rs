//! Persisted application settings (ticket 14): the appearance record (ticket
//! 07), the layout record, and the view state that determines what the
//! viewport shows.
//!
//! One envelope, one storage key (`eframe::APP_KEY`), so style, layout and
//! view state can never drift apart across restarts. Compatibility matters:
//! ticket 07 stored a *bare* [`AppStyle`] under the same key, so [`load`] falls
//! back to that shape when the envelope decode fails. Absent or corrupt
//! storage always yields defaults (cosmetic/session state must never brick
//! startup; camera presets stay fail-fast for data integrity).
//!
//! Camera state is deliberately *not* persisted: it resets to the default on
//! launch and camera presets are the way to restore a viewpoint.

use serde::{Deserialize, Serialize};

use crate::appearance_profile::AppearanceProfile;
use crate::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use crate::color_mapping::ColorQuantity;
use crate::color_ramp::{ColorRamp, ScaleMode};
use crate::colormap::Colormap;
use crate::custom_density::{
    DensityFamily, DensitySpec, DensitySupport, SupportKind, TimeLaw, validate_time_law_combination,
};
pub use crate::field_bake::BakeSettings;
use crate::field_probe::{
    CirculationLoop, GaussianSurface, LoopKind, MAX_LOOPS, MAX_SURFACES, SurfaceKind,
};
use crate::induction::{InductionCase, InductionScene};
use crate::interaction::{self, DEFAULT_PROFILE_NAME, InteractionProfile};
use crate::layer_style::LayerStyles;
use crate::material::{
    MaterialCase, MaterialError, MaterialFamily, MaterialScene, MaterialView, material_family,
    validate_material,
};
use crate::maxwell::{
    ConductorOutline, MaxwellCase, MaxwellError, MaxwellFamily, MaxwellScene, MaxwellView,
    maxwell_family, validate_maxwell,
};
use crate::maxwell_wave::WaveSpec;
use crate::notebook::Notebook;
use crate::object_scene::ObjectStyle;
use crate::ode::{OdeChart, OdeScene, OdeSeed, OdeSource};
use crate::parameters::ParameterSet;
use crate::particles::{
    DEFAULT_PARTICLE_FIXED_COLOR, DEFAULT_PARTICLE_OPACITY, DEFAULT_STAGNATION_SECONDS,
    DEFAULT_TRAIL_FADE, DEFAULT_TRAIL_WIDTH, MAX_PARTICLES, ParticleAppearance, ParticleColorMode,
};
use crate::pin_layout::PinUiRecord;
use crate::plane::PlaneAxis;
use crate::plot::PlotKind;
use crate::profile_library::MAX_USER_PROFILES;
use crate::scalar_analysis::{ComparisonSet, GradientSpec};
use crate::scalar_overlay::ScalarOverlaySet;
use crate::scalar_view::ProbeSet;
use crate::scalar_view::{MAX_CONTOUR_LEVELS, MAX_GRID_SIDE};
use crate::seed_layout::SeedLayout;
use crate::seed_layout_library::{DEFAULT_LAYOUT_NAME, MAX_USER_LAYOUTS, SeedLayoutProfile};
use crate::source_scene::{
    FieldMode, MAX_SOFTENING, MIN_SOFTENING, SourceDisplay, SourceRecord, SourceScene,
};
use crate::sources::FieldFamily;
use crate::ui::layout::LayoutState;
use crate::ui::sidebar::{self, Controls};
use crate::ui::style_panel::AppStyle;
use crate::view_orientation::UpAxis;
use crate::viewmodel::{
    ArrowLengthMode, LeftClickAction, MAX_DENSITIES, MIN_SCALAR_GRID_SIDE, State,
    StreamlineSeedMode,
};
use crate::viewport_style::ViewportStyle;
use glam::{DVec3, Vec3};

/// Everything the app remembers across restarts. `style` is a required field
/// on purpose: a ticket-07 record (a bare `AppStyle`) must *fail* this decode
/// so [`load`] can detect and migrate it, rather than silently defaulting the
/// appearance. The other two records are defaulted for forward/backward
/// compatibility.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AppSettings {
    pub style: AppStyle,
    #[serde(default)]
    pub layout: LayoutState,
    #[serde(default)]
    pub view: ViewRecord,
    /// Shared named parameters (issue #89). Missing on older envelopes, which
    /// load the empty version-1 default; an invalid stored set is replaced by
    /// [`AppSettings::sanitized`].
    #[serde(default)]
    pub parameters: ParameterSet,
    /// Viewport and reference colours/widths (issue #82). Missing on older
    /// envelopes, which load the shipped dark-laboratory look; corrupt values
    /// are repaired by [`AppSettings::sanitized`].
    #[serde(default)]
    pub viewport_style: ViewportStyle,
    /// Per-layer and per-family appearance overrides (issue #84 slice A1).
    /// Missing on older envelopes, which load the neutral record; corrupt
    /// values are repaired by [`AppSettings::sanitized`].
    #[serde(default)]
    pub layer_styles: LayerStyles,
    /// User appearance profiles (issue #85 slice A2a). Missing on older
    /// envelopes, which start with an empty library; corrupt values, duplicate
    /// names and excess entries are repaired by [`AppSettings::sanitized`].
    /// Restoring them never applies a profile.
    #[serde(default)]
    pub appearance_profiles: Vec<AppearanceProfile>,
    /// The active interaction bindings and camera feel (issue #88 slice A2a).
    /// Missing on older envelopes, which load the documented default chords
    /// and neutral feel; an invalid stored record is repaired by
    /// [`AppSettings::sanitized`].
    #[serde(default)]
    pub interaction: InteractionProfile,
    /// User interaction profiles (issue #88 slice A2a). Missing on older
    /// envelopes, which start with an empty library; corrupt records, the
    /// built-in name, duplicate names and excess entries are repaired by
    /// [`AppSettings::sanitized`]. The built-in `Default` is never stored, and
    /// restoring the library never applies a profile.
    #[serde(default)]
    pub interaction_profiles: Vec<InteractionProfile>,
    /// User named seed layouts (issue #87 slice S3a). Missing on older
    /// envelopes, which start with an empty library; corrupt records, the
    /// built-in name, duplicate names and excess entries are repaired by
    /// [`AppSettings::sanitized`]. The built-in `Default` is never stored, and
    /// restoring the library never applies a layout.
    #[serde(default)]
    pub seed_layouts: Vec<SeedLayoutProfile>,
    /// Persisted Presets-panel expansion (issue #90 slice B). Missing on older
    /// envelopes, which open every group; a corrupt record falls back to the
    /// all-expanded default without failing the envelope.
    #[serde(default, deserialize_with = "deserialize_preset_ui")]
    pub preset_ui: PresetUiRecord,
    /// How source/induction bodies are drawn (issue #115). Missing on older
    /// envelopes, which load Solid; an unknown stored token is repaired to
    /// Solid by [`ObjectStyle`]'s decoder with one warning, and a non-string
    /// stored value is repaired to Solid by the field wrapper instead of
    /// failing the whole envelope.
    #[serde(default, deserialize_with = "deserialize_object_style")]
    pub object_style: ObjectStyle,
    /// Display-only vertical world axis (issue #208). Missing on older
    /// envelopes, which load the Z default; an unknown stored token is repaired
    /// to Z with one warning and a non-string value is repaired by the field
    /// wrapper. Restoring it never dirties the scene and it is never stored in
    /// [`ViewRecord`], scenes, appearance profiles or history.
    #[serde(default, deserialize_with = "deserialize_up_axis")]
    pub up_axis: UpAxis,
    /// The opt-in baked-flow lattice settings (P3 slice 1b). Missing on older
    /// envelopes, which load disabled at 48³ / 128². Out-of-range resolutions
    /// are repaired by [`AppSettings::sanitized`] with one warning. Only this
    /// record is stored; the lattice itself is runtime-only and never enters a
    /// [`ViewRecord`], scene, profile or history snapshot.
    #[serde(default)]
    pub bake: BakeSettings,
    /// The active pin working set and named pin arrangements (issue #232).
    /// Missing on older envelopes, which start with no pins. This record never
    /// contains model state, probe geometry or sampled values; restoring it
    /// cannot mutate or stale the physical model. Invalid items/names are
    /// repaired by [`AppSettings::sanitized`].
    #[serde(default)]
    pub pin_ui: PinUiRecord,
    /// The persisted experiment notebook (issue #222). Missing on older
    /// envelopes, which start empty. Each entry owns an immutable measurement
    /// snapshot and a captured scene; restoring cannot mutate the live model.
    /// Invalid or excess entries are dropped by [`AppSettings::sanitized`].
    #[serde(default)]
    pub notebook: Notebook,
}

/// Persisted Presets-panel state. Only the per-group expansion is durable; the
/// search text and filters stay session-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetUiRecord {
    /// Expansion of Classic, Physics, Sources and Gallery, in that order.
    #[serde(default = "default_preset_expanded")]
    pub expanded: [bool; 4],
}

impl Default for PresetUiRecord {
    fn default() -> Self {
        Self {
            expanded: default_preset_expanded(),
        }
    }
}

/// Every group starts expanded, including every field of an older record.
fn default_preset_expanded() -> [bool; 4] {
    [true; 4]
}

/// Typeset CAS results are on unless an older or explicit record says otherwise.
fn default_typeset_output() -> bool {
    true
}

impl AppSettings {
    /// Restore the durable records into a live state in dependency order: the
    /// parameter set is assigned first (a startup restore never dirties the
    /// scene on its own), then the view, whose stored formula is parsed against
    /// those committed values.
    ///
    /// This is the only supported startup order. Applying the view first would
    /// parse a saved parameterised formula against the empty default set,
    /// record a bogus parse error and keep the default field.
    pub fn restore(&self, state: &mut State, controls: &mut Controls) {
        state.parameters = self.parameters.clone();
        // A display-only startup assignment: the object style never dirties
        // the first scene, and the field decoder already repaired bad values.
        state.restore_object_style(self.object_style);
        // Issue #208: orientation is display-only. Assign it at the startup
        // seam so it never dirties the document, matches the history baseline,
        // and never enters a `ViewRecord`.
        state.up_axis = self.up_axis;
        self.view.restore(state, controls);
        // P3 slice 1b: the bake opt-in is runtime-only once loaded. Applying it
        // last builds at most one bounded lattice against the restored field and
        // never dirties the first scene; a refusal is recorded, not fatal.
        state.apply_bake_settings(self.bake);
    }

    /// Clamp corrupt numbers from storage into usable ranges: style into the
    /// Settings slider bounds, layout widths into the draggable range, and
    /// layer-style overrides into their multiplier bounds.
    pub fn sanitized(mut self) -> Self {
        // Repair the bake record before the rest: an old or hand-edited
        // resolution must clamp instead of failing the envelope. One warning
        // covers both axes.
        let repaired_bake = self.bake.sanitized();
        if repaired_bake != self.bake {
            log::warn!(
                "persisted bake settings are out of range ({:?}); repairing them",
                self.bake
            );
            self.bake = repaired_bake;
        }
        // Repair the newest style record first: an older or hand-edited
        // envelope must not fail on an out-of-range multiplier.
        if let Err(error) = self.layer_styles.validate() {
            log::warn!("persisted layer styles are invalid ({error}); repairing them");
            self.layer_styles = self.layer_styles.sanitized();
        }
        self.style = self.style.sanitized();
        self.layout = self.layout.sanitized();
        self.viewport_style = self.viewport_style.sanitized();
        // User profiles are records, not live appearance: repair each entry,
        // keep the first occurrence of every sanitized name and the first
        // `MAX_USER_PROFILES` entries. A damaged profile must never fail the
        // envelope or apply itself.
        let mut names = std::collections::HashSet::with_capacity(
            self.appearance_profiles.len().min(MAX_USER_PROFILES),
        );
        let mut profiles =
            Vec::with_capacity(self.appearance_profiles.len().min(MAX_USER_PROFILES));
        for profile in self.appearance_profiles.drain(..) {
            let profile = profile.sanitized();
            if !names.insert(profile.name.clone()) {
                log::warn!(
                    "persisted appearance profile {:?} duplicates an earlier name; dropping it",
                    profile.name
                );
                continue;
            }
            if profiles.len() == MAX_USER_PROFILES {
                log::warn!(
                    "more than {MAX_USER_PROFILES} persisted appearance profiles; \
                     dropping the rest"
                );
                break;
            }
            profiles.push(profile);
        }
        self.appearance_profiles = profiles;
        // The active interaction record is repaired; if it still cannot
        // validate (unreachable for `sanitized`'s total output, but kept
        // total), fall back to the documented default instead of failing
        // startup or carrying an invalid table.
        if let Err(error) = self.interaction.validate() {
            log::warn!("persisted interaction settings are invalid ({error}); repairing them");
            self.interaction = self.interaction.sanitized();
            if let Err(error) = self.interaction.validate() {
                log::warn!(
                    "persisted interaction settings are still invalid ({error}); \
                     resetting them to defaults"
                );
                self.interaction = InteractionProfile::default();
            }
        }
        // User interaction profiles are records too: repair each entry, drop
        // an entry named like the built-in, keep the first occurrence of every
        // sanitized name and the first `MAX_USER_PROFILES` entries. A damaged
        // profile must never fail the envelope or apply itself.
        let mut interaction_names = std::collections::HashSet::with_capacity(
            self.interaction_profiles
                .len()
                .min(interaction::MAX_USER_PROFILES),
        );
        let mut interaction_profiles = Vec::with_capacity(
            self.interaction_profiles
                .len()
                .min(interaction::MAX_USER_PROFILES),
        );
        for profile in self.interaction_profiles.drain(..) {
            let profile = profile.sanitized();
            if profile.name == DEFAULT_PROFILE_NAME {
                log::warn!(
                    "persisted interaction profile {:?} is the built-in name; dropping it",
                    profile.name
                );
                continue;
            }
            if !interaction_names.insert(profile.name.clone()) {
                log::warn!(
                    "persisted interaction profile {:?} duplicates an earlier name; dropping it",
                    profile.name
                );
                continue;
            }
            if interaction_profiles.len() == interaction::MAX_USER_PROFILES {
                log::warn!(
                    "more than {} persisted interaction profiles; dropping the rest",
                    interaction::MAX_USER_PROFILES
                );
                break;
            }
            interaction_profiles.push(profile);
        }
        self.interaction_profiles = interaction_profiles;
        // User seed layouts are records too. Their seeds are only meaningful
        // against the view the envelope restores, so recover that domain by
        // running the same `ViewRecord::restore` repair the app will run: a 2D
        // view collapses z to the domain midpoint and a plane slice uses its
        // local domain. Then repair each layout, drop entries whose trimmed
        // name is empty or names the built-in, keep the first occurrence of a
        // case-sensitive duplicate and the first `MAX_USER_LAYOUTS` entries.
        // A damaged layout must never fail the envelope or apply itself.
        let seed_domain = {
            let mut probe = State::new(false);
            let mut probe_controls = Controls::new();
            // Bind the saved formula the same way `AppSettings::restore` does;
            // otherwise a parameterised formula records a bogus parse error
            // here. The probe is discarded, only its scene domain is consumed.
            probe.parameters = self.parameters.clone();
            self.view.restore(&mut probe, &mut probe_controls);
            probe.scene_domain()
        };
        let mut seed_names =
            std::collections::HashSet::with_capacity(self.seed_layouts.len().min(MAX_USER_LAYOUTS));
        let mut seed_layouts = Vec::with_capacity(self.seed_layouts.len().min(MAX_USER_LAYOUTS));
        for profile in self.seed_layouts.drain(..) {
            let trimmed = profile.name.trim();
            if trimmed.is_empty() || trimmed == DEFAULT_LAYOUT_NAME {
                log::warn!(
                    "persisted seed layout name {:?} is empty or the built-in name; dropping it",
                    profile.name
                );
                continue;
            }
            let profile = profile.sanitized(&seed_domain);
            if !seed_names.insert(profile.name.clone()) {
                log::warn!(
                    "persisted seed layout {:?} duplicates an earlier name; dropping it",
                    profile.name
                );
                continue;
            }
            if seed_layouts.len() == MAX_USER_LAYOUTS {
                log::warn!(
                    "more than {MAX_USER_LAYOUTS} persisted seed layouts; dropping the rest"
                );
                break;
            }
            seed_layouts.push(profile);
        }
        self.seed_layouts = seed_layouts;
        // Issue #232: the pin preference is a pure reference list. Repair its
        // items, names, duplicates and bounds; it can never touch the model.
        let repaired_pins = self.pin_ui.clone().sanitized();
        if repaired_pins != self.pin_ui {
            log::warn!("persisted pin preference is invalid; repairing it");
            self.pin_ui = repaired_pins;
        }
        // Issue #222: the notebook is a bounded record. Drop corrupt, duplicate
        // or excess entries and normalize the version; never mutate a snapshot.
        let repaired_notebook = self.notebook.clone().sanitized();
        if repaired_notebook != self.notebook {
            log::warn!("persisted experiment notebook is invalid; repairing it");
            self.notebook = repaired_notebook;
        }
        // `preset_ui` is four bools, so every persisted value is valid; the
        // lenient decoder has already replaced a corrupt record.
        if let Err(error) = self.parameters.validate() {
            log::warn!(
                "persisted parameters are invalid ({error}); falling back to the default set"
            );
            self.parameters = ParameterSet::default();
        }
        self
    }
}

/// One persisted Gaussian surface (issue #95 slice B), in a serde-friendly
/// shape. The kind is the stable token `sphere`, `box` or `cylinder`; an
/// unknown token drops the record on restore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeSurfaceRecord {
    pub kind: String,
    pub center: [f32; 3],
    pub axis: [f32; 3],
    pub size: [f32; 3],
    pub segments: u32,
}

impl ProbeSurfaceRecord {
    /// Flatten one stored surface.
    pub fn from_surface(surface: &GaussianSurface) -> Self {
        Self {
            kind: match surface.kind {
                SurfaceKind::Sphere => "sphere",
                SurfaceKind::Box => "box",
                SurfaceKind::Cylinder => "cylinder",
            }
            .to_string(),
            center: surface.center.to_array(),
            axis: surface.axis.to_array(),
            size: surface.size.to_array(),
            segments: surface.segments,
        }
    }

    /// Rebuild the model surface. An unknown kind returns `None`; geometry is
    /// validated by the restore path, not here.
    pub fn to_surface(&self) -> Option<GaussianSurface> {
        let kind = match self.kind.as_str() {
            "sphere" => SurfaceKind::Sphere,
            "box" => SurfaceKind::Box,
            "cylinder" => SurfaceKind::Cylinder,
            _ => return None,
        };
        Some(GaussianSurface {
            kind,
            center: glam::Vec3::from_array(self.center),
            axis: glam::Vec3::from_array(self.axis),
            size: glam::Vec3::from_array(self.size),
            segments: self.segments,
        })
    }
}

/// One persisted circulation loop (issue #95 slice B). The kind is the stable
/// token `circle` or `rectangle`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeLoopRecord {
    pub kind: String,
    pub center: [f32; 3],
    pub axis: [f32; 3],
    pub size: [f32; 3],
    pub segments: u32,
}

impl ProbeLoopRecord {
    /// Flatten one stored loop.
    pub fn from_loop(loop_: &CirculationLoop) -> Self {
        Self {
            kind: match loop_.kind {
                LoopKind::Circle => "circle",
                LoopKind::Rectangle => "rectangle",
            }
            .to_string(),
            center: loop_.center.to_array(),
            axis: loop_.axis.to_array(),
            size: loop_.size.to_array(),
            segments: loop_.segments,
        }
    }

    /// Rebuild the model loop. An unknown kind returns `None`.
    pub fn to_loop(&self) -> Option<CirculationLoop> {
        let kind = match self.kind.as_str() {
            "circle" => LoopKind::Circle,
            "rectangle" => LoopKind::Rectangle,
            _ => return None,
        };
        Some(CirculationLoop {
            kind,
            center: glam::Vec3::from_array(self.center),
            axis: glam::Vec3::from_array(self.axis),
            size: glam::Vec3::from_array(self.size),
            segments: self.segments,
        })
    }
}

/// The stable probe-family token written by [`ViewRecord::capture`].
fn probe_family_label(family: FieldFamily) -> &'static str {
    match family {
        FieldFamily::Electric => "electric",
        FieldFamily::Magnetic => "magnetic",
    }
}

/// Decode one stored probe-family token. Unknown or missing text returns
/// `None`; the restore path treats that as the electric fallback.
fn probe_family_from_label(label: &str) -> Option<FieldFamily> {
    match label {
        "electric" => Some(FieldFamily::Electric),
        "magnetic" => Some(FieldFamily::Magnetic),
        _ => None,
    }
}

/// Whether the restored mode can supply a probe field for this family. Sources
/// supply both families even when one has no enabled sources (its zero is
/// exact); Induction supplies only the induced electric field; Materials and
/// ordinary Maxwell scenes supply the family of their active case. A restored
/// Maxwell plane wave supplies both fields. Formula and Ode supply neither.
fn probe_family_available(state: &State, family: FieldFamily) -> bool {
    match state.field_mode {
        FieldMode::Sources => true,
        FieldMode::Induction => family == FieldFamily::Electric,
        FieldMode::Materials => {
            matches!(
                (material_family(state.material.case), family),
                (MaterialFamily::Electric, FieldFamily::Electric)
                    | (MaterialFamily::Magnetic, FieldFamily::Magnetic)
            )
        }
        FieldMode::Maxwell if state.maxwell_wave().is_some() => true,
        FieldMode::Maxwell => {
            matches!(
                (maxwell_family(state.maxwell.case), family),
                (MaxwellFamily::Electric, FieldFamily::Electric)
                    | (MaxwellFamily::Magnetic, FieldFamily::Magnetic)
            )
        }
        FieldMode::Formula | FieldMode::Ode => false,
    }
}

/// Wire form of one custom density. `glam` has no serde feature in this crate,
/// so the support vectors are stored as plain arrays, matching the probe record
/// style. The model types stay free of persistence attributes. The family and
/// support kind are stable tokens; an unknown token makes the whole entry
/// unrepresentable, so [`DensitySpecRecord::to_spec`] returns `None` and the
/// restore `filter_map` drops it instead of failing the settings envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DensitySpecRecord {
    pub name: String,
    pub family: String,
    pub support: DensitySupportRecord,
    pub formula: String,
    pub samples: u32,
    pub softening: f64,
    /// Optional declared clock (issue #98 slice A). A steady law writes no key,
    /// so documents written before the field stay byte-identical. An unknown
    /// token or out-of-range value drops only the law on restore, keeping the
    /// spatial density with [`TimeLaw::Steady`]. A structurally malformed
    /// object (missing `kind`, wrong-typed `omega` or `phase`) instead fails the
    /// whole document at deserialization, like any wrong-typed field. If a
    /// harmonic current loses its law this way, a still-valid `charge_formula`
    /// survives on the resulting steady current and is ignored by the
    /// historical `div J = 0` check; slice B must surface or drop that orphaned
    /// pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_law: Option<TimeLawRecord>,
    /// Optional independent charge density paired with a harmonic current
    /// (issue #98 slice A). Missing on older documents and on every steady or
    /// charge record, so their wire form stays byte-identical. An invalid
    /// stored formula drops only the pairing on restore, keeping the spatial
    /// density and its law. A law dropped on restore can leave this pairing
    /// orphaned on a steady current, where it is ignored; see `time_law`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charge_formula: Option<String>,
}

/// Plain-token mirror of one [`TimeLaw`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeLawRecord {
    pub kind: String,
    #[serde(default)]
    pub omega: f64,
    #[serde(default)]
    pub phase: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DensitySupportRecord {
    pub kind: String,
    pub center: [f32; 3],
    pub axis: [f32; 3],
    pub size: [f32; 3],
}

fn density_family_label(family: DensityFamily) -> &'static str {
    match family {
        DensityFamily::VolumeCharge => "volume_charge",
        DensityFamily::SurfaceCharge => "surface_charge",
        DensityFamily::SteadyCurrent => "steady_current",
    }
}

fn density_family_from_label(label: &str) -> Option<DensityFamily> {
    match label {
        "volume_charge" => Some(DensityFamily::VolumeCharge),
        "surface_charge" => Some(DensityFamily::SurfaceCharge),
        "steady_current" => Some(DensityFamily::SteadyCurrent),
        _ => None,
    }
}

fn time_law_record(law: TimeLaw) -> Option<TimeLawRecord> {
    match law {
        TimeLaw::Steady => None,
        TimeLaw::Harmonic { omega, phase } => Some(TimeLawRecord {
            kind: "harmonic".to_string(),
            omega,
            phase,
        }),
    }
}

fn time_law_from_record(record: &TimeLawRecord) -> Option<TimeLaw> {
    match record.kind.as_str() {
        "steady" => Some(TimeLaw::Steady),
        "harmonic" => Some(TimeLaw::Harmonic {
            omega: record.omega,
            phase: record.phase,
        }),
        _ => None,
    }
}

fn support_kind_label(kind: SupportKind) -> &'static str {
    match kind {
        SupportKind::Sphere => "sphere",
        SupportKind::Box => "box",
        SupportKind::Cylinder => "cylinder",
    }
}

fn support_kind_from_label(label: &str) -> Option<SupportKind> {
    match label {
        "sphere" => Some(SupportKind::Sphere),
        "box" => Some(SupportKind::Box),
        "cylinder" => Some(SupportKind::Cylinder),
        _ => None,
    }
}

impl DensitySpecRecord {
    /// Flatten one committed density into its stable wire form.
    pub fn from_spec(spec: &DensitySpec) -> Self {
        Self {
            name: spec.name.clone(),
            family: density_family_label(spec.family).to_string(),
            support: DensitySupportRecord {
                kind: support_kind_label(spec.support.kind).to_string(),
                center: spec.support.center.to_array(),
                axis: spec.support.axis.to_array(),
                size: spec.support.size.to_array(),
            },
            formula: spec.formula.clone(),
            samples: spec.samples,
            softening: spec.softening,
            time_law: time_law_record(spec.time_law),
            charge_formula: spec.charge_formula.clone(),
        }
    }

    /// Rebuild the model density. An unknown family or support-kind token
    /// returns `None`; the restore path validates the rest of the record.
    ///
    /// An unknown or invalid time law is dropped on its own: the returned
    /// density keeps the spatial formula with [`TimeLaw::Steady`]. The restore
    /// path logs one warning for the loss. A structurally malformed
    /// `time_law` object never reaches here: it fails deserialization, like a
    /// wrong-typed `samples`.
    pub fn to_spec(&self) -> Option<DensitySpec> {
        let family = density_family_from_label(&self.family)?;
        let kind = support_kind_from_label(&self.support.kind)?;
        let (time_law, _) = self.restored_time_law();
        Some(DensitySpec {
            name: self.name.clone(),
            family,
            support: DensitySupport {
                kind,
                center: Vec3::from_array(self.support.center),
                axis: Vec3::from_array(self.support.axis),
                size: Vec3::from_array(self.support.size),
            },
            formula: self.formula.clone(),
            samples: self.samples,
            softening: self.softening,
            time_law,
            charge_formula: self.charge_formula.clone(),
        })
    }

    /// Decode the optional law and report whether a stored law was dropped.
    fn restored_time_law(&self) -> (TimeLaw, bool) {
        let Some(record) = &self.time_law else {
            return (TimeLaw::Steady, false);
        };
        match time_law_from_record(record) {
            Some(law) if law.validate().is_ok() => (law, false),
            _ => (TimeLaw::Steady, true),
        }
    }
}

/// The view state the Controls panel edits, in a serde-friendly shape (JSON/RON
/// arrays instead of glam types). These are the *effective* values from
/// [`State`], so a restored record reproduces exactly what was on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewRecord {
    pub formula: String,
    pub mode_3d: bool,
    pub plot_kind: PlotKind,
    pub curve_source: String,
    pub surface_source: String,
    pub curve_samples: usize,
    pub surface_resolution: u32,
    pub plot_line_width: f32,
    pub plot_auto_height: bool,
    pub plot_height_min: f32,
    pub plot_height_max: f32,
    #[serde(default)]
    pub last_vector_mode_3d: Option<bool>,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    pub density: u32,
    pub arrow_scale: f32,
    /// Global arrow opacity (ticket 36). Older records lack the key; the serde
    /// default keeps them fully opaque.
    #[serde(default = "default_arrow_opacity")]
    pub arrow_opacity: f32,
    /// Plane-mode off-plane opacity (ticket 36). Missing on older records; the
    /// serde default is the historical `DIM_ALPHA`.
    #[serde(default = "default_plane_dim_opacity")]
    pub plane_dim_opacity: f32,
    /// Ticket-14's normalization flag. Only read: a record written before
    /// ticket 33 still carries a bare `normalize: true|false`, and
    /// [`ViewRecord::restore`] maps it onto [`ArrowLengthMode`]. New records
    /// never write the key, so it stays `None` and the old flat bool decodes
    /// through [`deserialize_legacy_normalize`] rather than `Option`'s RON
    /// `Some(...)` spelling.
    #[serde(
        default,
        skip_serializing,
        deserialize_with = "deserialize_legacy_normalize"
    )]
    pub normalize: Option<bool>,
    /// The three-way replacement for `normalize` (ticket 33). Missing on a
    /// legacy record; the container-level default then supplies Proportional.
    pub arrow_length_mode: ArrowLengthMode,
    /// World units per unit of magnitude, used only in Magnitude mode.
    /// Missing on a legacy record; the container-level default supplies 1.0.
    pub magnitude_scale: f32,
    /// The Proportional contrast exponent (ticket 57). Missing on records
    /// written before ticket 57; the serde default loads them as linear.
    #[serde(default = "default_proportional_exponent")]
    pub proportional_exponent: f32,
    pub colormap: Colormap,
    /// Custom colour ramp override (issue #83). Missing on older records, where
    /// `None` keeps the builtin colormap.
    #[serde(default)]
    pub color_ramp: Option<ColorRamp>,
    /// Colour scaling mode (issue #83). Missing on older records; the default
    /// is linear.
    #[serde(default)]
    pub color_mode: ScaleMode,
    /// What the colour represents (issue #83). Missing on older records; the
    /// default is magnitude.
    #[serde(default)]
    pub color_quantity: ColorQuantity,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    /// Display the 2D lattice or 3D face grids and wireframe domain box.
    /// Missing in older records: reference geometry was always visible.
    #[serde(default = "default_reference_visible")]
    pub show_grid: bool,
    /// Display only the coloured X/Y (2D) or X/Y/Z (3D) axes.
    /// Independent of the grid and domain box.
    #[serde(default = "default_reference_visible")]
    pub show_coordinate_system: bool,
    /// Show arrows (ticket 45). Older records did not carry it, so the
    /// container default (true) keeps them showing arrows.
    #[serde(default = "default_arrows")]
    pub arrows: bool,
    /// The 3D arrow family (ticket 45).
    #[serde(default)]
    pub arrow_style: ArrowStyle,
    /// The 2D arrow family (ticket 61). Missing on pre-61 records; the serde
    /// default loads them as [`ArrowStyle2D::Flat`]. Old `Line` spellings also
    /// decode as Flat (ticket 64).
    #[serde(default)]
    pub arrow_style_2d: ArrowStyle2D,
    /// The 3D arrow head/shaft proportions (ticket 45).
    #[serde(default)]
    pub arrow_params: ArrowParams,
    pub streamlines: bool,
    /// Which seeds the streamline Feature paints (ticket 50). Older records
    /// did not carry it; the container default loads them as Auto.
    #[serde(default)]
    pub seed_mode: StreamlineSeedMode,
    /// The explicit streamline seed count; `0` is Auto. Missing on records
    /// written before the count existed; the serde default loads them as Auto.
    #[serde(default)]
    pub streamline_count: u32,
    /// The durable explicit streamline seed layout (issue #87 slice S1).
    /// Missing on older records; the serde default is the empty layout, which
    /// keeps the automatic grid.
    #[serde(default)]
    pub seed_layout: SeedLayout,
    /// Durable scalar point and line probes (issue #92 slice A). Missing on
    /// older envelopes/scenes, which restore as no probes with no dirtying.
    /// Only positions and anchors are stored; sampled values are runtime.
    #[serde(default)]
    pub probes: ProbeSet,
    /// Durable Gaussian surfaces (issue #95 slice B). Missing on older
    /// envelopes/scenes, which restore as no surfaces with no extra dirtying.
    /// Invalid or excess entries drop on restore.
    #[serde(default)]
    pub probe_surfaces: Vec<ProbeSurfaceRecord>,
    /// Durable circulation loops (issue #95 slice B), with the same restore
    /// sanitising as surfaces.
    #[serde(default)]
    pub probe_loops: Vec<ProbeLoopRecord>,
    /// The explicit probe family as the stable token `electric` or `magnetic`
    /// (issue #95 slice B). Missing on older envelopes/scenes, which restore
    /// electric. Corrupt or mode-unavailable tokens also restore electric with
    /// one warning and never dirty the scene.
    #[serde(default)]
    pub probe_family: String,
    /// Scalar view heatmap toggle (issue #92 slice B2). Missing on older
    /// records/scenes, which restore hidden without dirtying.
    #[serde(default)]
    pub show_scalar_heatmap: bool,
    /// Manual scalar sampling controls. Older records use the current defaults.
    pub scalar_grid_side: u32,
    pub scalar_contour_levels: u32,
    /// Display-only comparison series (issue #93 slice A). Missing on older
    /// records, which restore as no series with no extra dirtying.
    #[serde(default)]
    pub comparison_series: ComparisonSet,
    /// The validated potential-gradient request (issue #93 slice A). Missing
    /// on older records, which restore as no gradient.
    #[serde(default)]
    pub gradient: Option<GradientSpec>,
    /// Durable bounded CAS overlays (issue #233). Missing on older records,
    /// which restore as an empty set. Kept in the workspace view record as well
    /// as the scene document so an overlay edit enters document history and
    /// survives an ordinary restart without a named scene.
    #[serde(default, skip_serializing_if = "ScalarOverlaySet::is_empty")]
    pub overlays: ScalarOverlaySet,
    /// Durable custom densities (issue #96 slice B1), at most
    /// [`MAX_DENSITIES`]. Missing on older records/scenes, which restore as an
    /// empty list. Unknown family/support-kind tokens and invalid entries drop
    /// on restore and keep the rest in order.
    #[serde(default)]
    pub densities: Vec<DensitySpecRecord>,
    /// What one explicit left click does (ticket 60). Older records did not
    /// carry it; they load as SeedStreamline. An unknown spelling also falls
    /// back to the default instead of failing the whole envelope.
    #[serde(default, deserialize_with = "deserialize_left_click_action")]
    pub left_click_action: LeftClickAction,
    pub particles: bool,
    /// The particle dot size multiplier (ticket 30). Missing on older
    /// records; the serde default loads them at the base fraction.
    #[serde(default = "default_particle_size")]
    pub particle_size: f32,
    /// The explicit particle count; `0` is Auto (ticket 30). Missing on older
    /// records; the serde default loads them as Auto.
    #[serde(default)]
    pub particle_count: u32,
    /// Particle appearance (ticket 48), stored flat so an older record can add
    /// each key independently. All eight default from the fresh-state record.
    #[serde(default = "default_particle_opacity")]
    pub particle_opacity: f32,
    #[serde(default)]
    pub particle_color_mode: ParticleColorMode,
    #[serde(default = "default_particle_fixed_color")]
    pub particle_fixed_color: [u8; 4],
    #[serde(default)]
    pub particle_trail_len: u32,
    #[serde(default = "default_particle_trail_fade")]
    pub particle_trail_fade: f32,
    #[serde(default = "default_particle_trail_width")]
    pub particle_trail_width: f32,
    #[serde(default = "default_particle_loop_on_stagnation")]
    pub particle_loop_on_stagnation: bool,
    #[serde(default = "default_particle_stagnation_seconds")]
    pub particle_stagnation_seconds: f32,
    /// Which adapter the viewport shows. A pre-#25 record has no key and opens
    /// in Formula mode with an empty source scene.
    #[serde(default)]
    pub field_mode: FieldMode,
    /// CAS result display preference (issue #171). Missing on older records and
    /// defaults to typeset; restoring it never dirties the scene.
    #[serde(default = "default_typeset_output")]
    pub typeset_output: bool,
    /// The durable source scene in list order. Each record is validated again
    /// on restore; malformed entries are skipped.
    #[serde(default, deserialize_with = "deserialize_source_entries")]
    pub source_entries: Vec<SourceRecord>,
    /// Which field families the source scene draws.
    #[serde(default)]
    pub source_display: SourceDisplay,
    /// Scene-wide softening. Out-of-range or non-finite values fall back to
    /// the default in [`ViewRecord::restore`].
    #[serde(default = "default_source_softening")]
    pub source_softening: f64,
    /// Whether accuracy-not-met estimates may be drawn. Only an exact `true`
    /// restores the opt-in; every other value becomes false.
    #[serde(default, deserialize_with = "deserialize_bool_or_false")]
    pub source_allow_inaccurate: bool,
    /// The durable induction case token: `current_loop` or `solenoid`. Missing
    /// on a legacy record; the default is the valid current-loop scene.
    #[serde(default = "default_induction_case")]
    pub induction_case: String,
    /// Induction conductor center (design §8). Validated on restore.
    #[serde(default = "default_induction_center")]
    pub induction_center: [f64; 3],
    /// Induction conductor axis; must be finite and unit length.
    #[serde(default = "default_induction_axis")]
    pub induction_axis: [f64; 3],
    #[serde(default = "default_induction_radius")]
    pub induction_radius: f64,
    #[serde(default)]
    pub induction_length: f64,
    #[serde(default = "default_induction_turns")]
    pub induction_turns: u32,
    #[serde(default = "default_induction_amplitude")]
    pub induction_amplitude: f64,
    #[serde(default = "default_induction_frequency")]
    pub induction_frequency: f64,
    #[serde(default)]
    pub induction_display: SourceDisplay,
    #[serde(default, deserialize_with = "deserialize_bool_or_false")]
    pub induction_allow_inaccurate: bool,
    /// Durable ODE source token: `equations` or `current_field`. Missing on a
    /// legacy record; the default is the typed harmonic scene.
    #[serde(default = "default_ode_source")]
    pub ode_source: String,
    /// ODE scene dimension (2 or 3). Validated together with the rest of the
    /// scene on restore; malformed values fall back to the default scene.
    #[serde(default = "default_ode_dimension")]
    pub ode_dimension: u8,
    /// The three durable typed right-hand-side slots; unused slots may be any
    /// string because validation parses only the first `ode_dimension`.
    #[serde(default = "default_ode_equations")]
    pub ode_equations: [String; 3],
    /// Initial conditions as plain `[x, y, z]` arrays so `glam` never appears
    /// in the persisted schema.
    #[serde(default = "default_ode_seeds")]
    pub ode_seeds: Vec<[f64; 3]>,
    /// The ODE scene duration, validated to `(0, 25]` on restore.
    #[serde(default = "default_ode_duration")]
    pub ode_duration: f64,
    /// ODE chart token: `phase` or `time_series`.
    #[serde(default = "default_ode_chart")]
    pub ode_chart: String,
    /// Time-series variable selection; at least one is restored on.
    #[serde(default = "default_ode_variables")]
    pub ode_variables: [bool; 3],
    /// Durable Materials case token. Missing on a legacy record; the default is
    /// the valid dielectric sphere.
    #[serde(default = "default_material_case")]
    pub material_case: String,
    #[serde(default = "default_material_background_epsilon_r")]
    pub material_background_epsilon_r: f64,
    #[serde(default = "default_material_background_mu_r")]
    pub material_background_mu_r: f64,
    #[serde(default = "default_material_material_epsilon_r")]
    pub material_material_epsilon_r: f64,
    #[serde(default = "default_material_material_mu_r")]
    pub material_material_mu_r: f64,
    #[serde(default = "default_material_radius")]
    pub material_radius: f64,
    /// `x`, `y` or `z`; validated on restore.
    #[serde(default = "default_material_interface_axis")]
    pub material_interface_axis: String,
    #[serde(default)]
    pub material_interface_offset: f64,
    /// Applied field as plain `[x, y, z]` so `glam` never appears in storage.
    #[serde(default = "default_material_applied")]
    pub material_applied: [f64; 3],
    /// `total`, `applied` or `response`.
    #[serde(default = "default_material_view")]
    pub material_view: String,
    /// Durable Maxwell teaching-case token. Missing on a legacy record; the
    /// default is the valid parallel-plate scene.
    #[serde(default = "default_maxwell_case")]
    pub maxwell_case: String,
    #[serde(default = "default_maxwell_e0")]
    pub maxwell_e0: f64,
    #[serde(default = "default_maxwell_b0")]
    pub maxwell_b0: f64,
    #[serde(default = "default_maxwell_separation")]
    pub maxwell_separation: f64,
    #[serde(default = "default_maxwell_radius")]
    pub maxwell_radius: f64,
    #[serde(default = "default_maxwell_outer_radius")]
    pub maxwell_outer_radius: f64,
    #[serde(default = "default_maxwell_charge")]
    pub maxwell_charge: f64,
    #[serde(default = "default_maxwell_charge_height")]
    pub maxwell_charge_height: f64,
    /// Conductor cross-section as `circle:<radius>` or `square:<half-extent>`.
    /// One field keeps the schema flat while preserving every outline value.
    #[serde(default = "default_maxwell_outline")]
    pub maxwell_outline: String,
    /// Applied 2D field as plain `[x, y]` display amplitudes.
    #[serde(default = "default_maxwell_applied")]
    pub maxwell_applied: [f64; 2],
    #[serde(default = "default_maxwell_elements")]
    pub maxwell_elements: u32,
    #[serde(default, deserialize_with = "deserialize_bool_or_false")]
    pub maxwell_allow_inaccurate: bool,
    /// Optional source-free plane wave (issue #97 slice B). Missing on older
    /// records/scenes, which keep the selected Maxwell case. The spec is
    /// self-contained; an invalid stored value is dropped with one warning on
    /// restore rather than failing the envelope or dirtying the first scene.
    #[serde(default, with = "wave_spec_serde")]
    pub maxwell_wave: Option<WaveSpec>,
    /// Obsolete pre-ticket-26 trace speed. Read-only compatibility: old
    /// envelopes still carry the key, but the simulation clock owns speed now
    /// and streamlines use a fixed budget. New records never write it.
    #[serde(default, skip_serializing)]
    pub speed: f32,
}

impl Default for ViewRecord {
    fn default() -> Self {
        Self::capture(&State::new(false))
    }
}

impl ViewRecord {
    /// Carry the CAS-panel display preference into the persisted envelope. The
    /// view record itself has no live state to capture it from.
    pub fn with_typeset_output(mut self, typeset_output: bool) -> Self {
        self.typeset_output = typeset_output;
        self
    }

    /// Snapshot the effective view state (the state's clamped/swapped values,
    /// not the widgets' in-flight mirror).
    pub fn capture(state: &State) -> Self {
        Self {
            formula: state.formula.clone(),
            mode_3d: state.mode_3d,
            plot_kind: state.plot.kind,
            curve_source: state.plot.curve.source.clone(),
            surface_source: state.plot.surface.source.clone(),
            curve_samples: state.plot.settings.curve_samples,
            surface_resolution: state.plot.settings.surface_resolution,
            plot_line_width: state.plot.settings.line_width,
            plot_auto_height: state.plot.settings.auto_height,
            plot_height_min: state.plot.settings.height_min,
            plot_height_max: state.plot.settings.height_max,
            last_vector_mode_3d: Some(if state.is_vector_mode() {
                state.mode_3d
            } else {
                state.plot.vector_mode_3d
            }),
            domain_min: state.domain.min.to_array(),
            domain_max: state.domain.max.to_array(),
            density: state.density,
            arrow_scale: state.arrow_scale,
            arrow_opacity: state.arrow_opacity,
            plane_dim_opacity: state.plane_dim_opacity,
            normalize: None,
            arrow_length_mode: state.arrow_length_mode,
            magnitude_scale: state.magnitude_scale,
            proportional_exponent: state.proportional_exponent,
            colormap: state.colormap,
            color_ramp: state.color_ramp.clone(),
            color_mode: state.color_mode,
            color_quantity: state.color_quantity,
            auto_fit: state.auto_fit,
            color_min: state.color_min,
            color_max: state.color_max,
            show_grid: state.show_grid,
            show_coordinate_system: state.show_coordinate_system,
            arrows: state.arrows,
            arrow_style: state.arrow_style,
            arrow_style_2d: state.arrow_style_2d,
            arrow_params: state.arrow_params,
            streamlines: state.streamlines,
            seed_mode: state.seed_mode,
            streamline_count: state.streamline_count,
            seed_layout: state.seed_layout.clone(),
            probes: state.probes.clone(),
            probe_surfaces: state
                .probe_surfaces()
                .iter()
                .map(ProbeSurfaceRecord::from_surface)
                .collect(),
            probe_loops: state
                .probe_loops()
                .iter()
                .map(ProbeLoopRecord::from_loop)
                .collect(),
            probe_family: probe_family_label(state.probe_family()).to_string(),
            show_scalar_heatmap: state.show_scalar_heatmap(),
            scalar_grid_side: state.scalar_grid_side(),
            scalar_contour_levels: state.scalar_contour_levels(),
            comparison_series: state.comparison_series.clone(),
            gradient: state.gradient.clone(),
            overlays: state.overlays().clone(),
            densities: state
                .densities()
                .iter()
                .map(DensitySpecRecord::from_spec)
                .collect(),
            left_click_action: state.left_click_action,
            particles: state.particles,
            particle_size: state.particle_size,
            particle_count: state.particle_count,
            particle_opacity: state.particle_appearance.opacity,
            particle_color_mode: state.particle_appearance.color_mode,
            particle_fixed_color: state.particle_appearance.fixed_color,
            particle_trail_len: state.particle_appearance.trail_len,
            particle_trail_fade: state.particle_appearance.trail_fade,
            particle_trail_width: state.particle_appearance.trail_width,
            particle_loop_on_stagnation: state.particle_appearance.loop_on_stagnation,
            particle_stagnation_seconds: state.particle_appearance.stagnation_seconds,
            field_mode: state.field_mode,
            typeset_output: true,
            source_entries: state.sources.to_records(),
            source_display: state.sources.display,
            source_softening: state.sources.softening,
            source_allow_inaccurate: state.sources.allow_inaccurate,
            induction_case: induction_case_label(state.induction.case).to_string(),
            induction_center: state.induction.center.to_array(),
            induction_axis: state.induction.axis.to_array(),
            induction_radius: state.induction.radius,
            induction_length: state.induction.length,
            induction_turns: state.induction.turns,
            induction_amplitude: state.induction.amplitude,
            induction_frequency: state.induction.frequency,
            induction_display: state.induction.display,
            induction_allow_inaccurate: state.induction.allow_inaccurate,
            ode_source: ode_source_label(state.ode.source).to_string(),
            ode_dimension: state.ode.dimension as u8,
            ode_equations: state.ode.equations.clone(),
            ode_seeds: state.ode.seeds.iter().map(|seed| seed.position).collect(),
            ode_duration: state.ode.duration,
            ode_chart: ode_chart_label(state.ode_chart).to_string(),
            ode_variables: state.ode_variables,
            material_case: material_case_label(state.material.case).to_string(),
            material_background_epsilon_r: state.material.background_epsilon_r,
            material_background_mu_r: state.material.background_mu_r,
            material_material_epsilon_r: state.material.material_epsilon_r,
            material_material_mu_r: state.material.material_mu_r,
            material_radius: state.material.radius,
            material_interface_axis: material_axis_label(state.material.interface_axis).to_string(),
            material_interface_offset: state.material.interface_offset,
            material_applied: state.material.applied,
            material_view: material_view_label(state.material.view).to_string(),
            maxwell_case: maxwell_case_label(state.maxwell.case).to_string(),
            maxwell_e0: state.maxwell.e0,
            maxwell_b0: state.maxwell.b0,
            maxwell_separation: state.maxwell.separation,
            maxwell_radius: state.maxwell.radius,
            maxwell_outer_radius: state.maxwell.outer_radius,
            maxwell_charge: state.maxwell.charge,
            maxwell_charge_height: state.maxwell.charge_height,
            maxwell_outline: maxwell_outline_label(state.maxwell.outline),
            maxwell_applied: state.maxwell.applied,
            maxwell_elements: state.maxwell.elements,
            maxwell_allow_inaccurate: state.maxwell.allow_inaccurate,
            maxwell_wave: state.maxwell_wave(),
            // Never written; kept at the serde default for old-record
            // compatibility only.
            speed: 0.0,
        }
    }

    /// Apply the record through the viewmodel setters (they own clamping,
    /// swapping and the dirty flag), then refresh the widget mirror with
    /// [`sidebar::apply`]. Non-finite domain components are left for that
    /// handoff — NaN keeps the previous effective extent and infinities clamp
    /// — while the other non-finite numbers fall back to defaults so a corrupt
    /// record cannot poison the sampling grid.
    pub fn restore(&self, state: &mut State, controls: &mut Controls) {
        state.view_restore_epoch = state.view_restore_epoch.wrapping_add(1);
        // A bake is runtime-only and cannot be described by a `ViewRecord`;
        // restoring a document snapshot never revives one.
        state.clear_field_bake();
        let defaults = Controls::new();
        state.set_plot_kind(PlotKind::VectorField);
        state.set_mode(self.last_vector_mode_3d.unwrap_or(self.mode_3d));
        state.set_formula(self.formula.clone());

        controls.density = self.density;
        controls.arrow_scale = finite_or(self.arrow_scale, defaults.arrow_scale);
        controls.arrow_opacity = finite_or(self.arrow_opacity, defaults.arrow_opacity);
        controls.plane_dim_opacity = finite_or(self.plane_dim_opacity, defaults.plane_dim_opacity);
        // Legacy records carried a `normalize` bool instead of a mode; it wins
        // when present so old settings land in the matching mode, and new
        // records (where it is absent) use the stored three-way mode.
        controls.arrow_length_mode = self
            .normalize
            .map(ArrowLengthMode::from_normalize)
            .unwrap_or(self.arrow_length_mode);
        controls.magnitude_scale = finite_or(self.magnitude_scale, defaults.magnitude_scale);
        controls.proportional_exponent =
            finite_or(self.proportional_exponent, defaults.proportional_exponent);
        controls.colormap = self.colormap;
        controls.auto_fit = self.auto_fit;
        controls.color_min = finite_or(self.color_min, defaults.color_min);
        controls.color_max = finite_or(self.color_max, defaults.color_max);
        controls.show_grid = self.show_grid;
        controls.show_coordinate_system = self.show_coordinate_system;
        controls.arrows = self.arrows;
        controls.arrow_style = self.arrow_style;
        controls.arrow_style_2d = self.arrow_style_2d;
        controls.arrow_params = self.arrow_params.sanitized();
        controls.streamlines = self.streamlines;
        controls.seed_mode = self.seed_mode;
        controls.streamline_count = self.streamline_count;
        controls.left_click_action = self.left_click_action;
        controls.particles = self.particles;
        controls.particle_size = finite_or(self.particle_size, defaults.particle_size);
        controls.particle_count = self.particle_count.min(MAX_PARTICLES as u32);
        controls.particle_appearance = ParticleAppearance {
            opacity: finite_or(self.particle_opacity, defaults.particle_appearance.opacity),
            color_mode: self.particle_color_mode,
            fixed_color: self.particle_fixed_color,
            trail_len: self.particle_trail_len,
            trail_fade: finite_or(
                self.particle_trail_fade,
                defaults.particle_appearance.trail_fade,
            ),
            trail_width: finite_or(
                self.particle_trail_width,
                defaults.particle_appearance.trail_width,
            ),
            loop_on_stagnation: self.particle_loop_on_stagnation,
            stagnation_seconds: finite_or(
                self.particle_stagnation_seconds,
                defaults.particle_appearance.stagnation_seconds,
            ),
        };
        // `self.speed` is read-only legacy data and deliberately ignored: the
        // transport clock owns speed and streamlines use a fixed budget.
        // Raw components: `sidebar::apply` is the one place that bounds the
        // extents, swaps a reversed pair and keeps the previous value for NaN.
        controls.domain_min = Vec3::from_array(self.domain_min);
        controls.domain_max = Vec3::from_array(self.domain_max);
        controls.scalar_grid_side = self
            .scalar_grid_side
            .clamp(MIN_SCALAR_GRID_SIDE, MAX_GRID_SIDE);
        controls.scalar_contour_levels = self.scalar_contour_levels.min(MAX_CONTOUR_LEVELS as u32);
        sidebar::apply(state, controls);
        // Explicit seeds are document state (issue #87 slice S1). Assign them
        // directly so startup does not go through the dirtying commit seam; a
        // corrupt stored layout is sanitised against the visible scene domain
        // (`scene_domain`, the same authority `commit_seed_layout` validates
        // against) and reported once, and the app keeps running.
        let stored_seeds = self.seed_layout.seeds.len();
        let seed_layout = self.seed_layout.clone().sanitized(&state.scene_domain());
        if seed_layout.seeds.len() != stored_seeds {
            log::warn!(
                "persisted seed layout dropped {} invalid or excess seed(s)",
                stored_seeds - seed_layout.seeds.len()
            );
        }
        state.seed_layout = seed_layout;
        // Scalar probes are document state (issue #92 slice A). Assign directly
        // so the startup seam does not dirty per probe; drop non-finite or
        // excess records instead of failing the whole envelope.
        let stored_probes = self.probes.len();
        let probes = self.probes.clone().sanitized();
        if probes.len() != stored_probes {
            log::warn!(
                "persisted probes dropped {} invalid or excess probe(s)",
                stored_probes - probes.len()
            );
        }
        state.probes = probes;
        // Probe surfaces and loops are document state (issue #95 slice B).
        // Assign directly so the startup seam does not dirty per probe; drop
        // unknown kinds, invalid geometry and excess entries, then log once.
        let stored_surfaces = self.probe_surfaces.len();
        let stored_loops = self.probe_loops.len();
        let surfaces: Vec<GaussianSurface> = self
            .probe_surfaces
            .iter()
            .filter_map(ProbeSurfaceRecord::to_surface)
            .filter(|surface| surface.validate().is_ok())
            .take(MAX_SURFACES)
            .collect();
        let loops: Vec<CirculationLoop> = self
            .probe_loops
            .iter()
            .filter_map(ProbeLoopRecord::to_loop)
            .filter(|loop_| loop_.validate().is_ok())
            .take(MAX_LOOPS)
            .collect();
        if surfaces.len() != stored_surfaces || loops.len() != stored_loops {
            log::warn!(
                "persisted probes dropped {} invalid or excess surface(s) and {} loop(s)",
                stored_surfaces - surfaces.len(),
                stored_loops - loops.len()
            );
        }
        state.probe_surfaces = surfaces;
        state.probe_loops = loops;
        // The heatmap toggle is document state assigned directly at the startup
        // seam, so a restored hidden/enabled scene does not request a second
        // rebuild beyond the enclosing restore's one.
        state.restore_show_scalar_heatmap(self.show_scalar_heatmap);
        // Issue #93 slice A: comparison series and the gradient request are
        // document state assigned directly at the startup seam, so they never
        // dirty per field. Corrupt stored data is repaired once (series) or
        // dropped (gradient) instead of failing the envelope.
        let stored_series = self.comparison_series.clone();
        let series = stored_series.clone().sanitized();
        if series != stored_series {
            log::warn!(
                "persisted comparison series were repaired or dropped ({} -> {} entries)",
                stored_series.len(),
                series.len()
            );
        }
        state.comparison_series = series;
        state.gradient = match &self.gradient {
            Some(spec) => match spec.validate() {
                Ok(()) => Some(spec.clone()),
                Err(error) => {
                    log::warn!("ignoring invalid persisted gradient: {error}");
                    None
                }
            },
            None => None,
        };
        // CAS overlays follow the same direct startup assignment: a valid set
        // restores without dirtying the first scene, and an invalid hand-edited
        // set is dropped with one warning rather than failing the envelope.
        match self.overlays.validate() {
            Ok(()) => state.restore_overlays(self.overlays.clone()),
            Err(error) => log::warn!("ignoring invalid persisted CAS overlays: {error}"),
        }
        // Custom densities are document state (issue #96 slice B1). Assign
        // directly so the startup seam does not dirty per entry; entries that
        // fail validation against the already restored parameters drop in
        // place, the survivors keep their order, and one warning reports the
        // loss.
        let stored_densities = self.densities.len();
        // An invalid stored clock drops only that law; the spatial density is
        // kept. One aggregated warning reports the loss without failing the
        // envelope or dirtying the scene. A structurally malformed `time_law`
        // never reaches this point: it fails at deserialization, like any
        // wrong-typed field. If the dropped law belonged to a harmonic current,
        // a still-valid charge formula stays on the now-steady current and is
        // ignored by the historical div J = 0 check; slice B must surface or
        // drop that orphaned pairing.
        let dropped_laws = self
            .densities
            .iter()
            .filter(|record| record.restored_time_law().1)
            .count();
        if dropped_laws > 0 {
            log::warn!(
                "ignored {dropped_laws} invalid persisted density time law{}; kept the spatial density",
                if dropped_laws == 1 { "" } else { "s" }
            );
        }
        let mut densities: Vec<DensitySpec> = self
            .densities
            .iter()
            .filter_map(DensitySpecRecord::to_spec)
            .collect();
        // A paired charge formula is optional and independent of the spatial
        // density. Drop it alone when it cannot parse against the restored
        // parameters or when it sits on a charge family (whose own formula is
        // already `rho`); keep the density and never dirty. One warning covers
        // every loss.
        let mut dropped_charge_formulas = 0;
        for spec in &mut densities {
            if spec.charge_formula.is_none() {
                continue;
            }
            let charge_family = matches!(
                spec.family,
                DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge
            );
            if charge_family || spec.validate_charge_formula(&state.parameters).is_err() {
                spec.charge_formula = None;
                dropped_charge_formulas += 1;
            }
        }
        if dropped_charge_formulas > 0 {
            log::warn!(
                "ignored {dropped_charge_formulas} invalid persisted density charge formula{}; kept the spatial density",
                if dropped_charge_formulas == 1 {
                    ""
                } else {
                    "s"
                }
            );
        }
        let mut densities: Vec<DensitySpec> = densities
            .into_iter()
            .filter(|spec| spec.validate(&state.parameters).is_ok())
            .take(MAX_DENSITIES)
            .collect();
        if densities.len() != stored_densities {
            log::warn!(
                "persisted custom densities dropped {} invalid or excess entr{}",
                stored_densities - densities.len(),
                if stored_densities - densities.len() == 1 {
                    "y"
                } else {
                    "ies"
                }
            );
        }
        // The same explicit rule as the edit path: demote a stored harmonic
        // charge when it would sit beside a steady current. This drops only
        // the invalid law and keeps every spatial density.
        if validate_time_law_combination(&densities).is_err() {
            let mut demoted = 0;
            for spec in &mut densities {
                if matches!(
                    spec.family,
                    DensityFamily::VolumeCharge | DensityFamily::SurfaceCharge
                ) && spec.time_law.is_harmonic()
                {
                    spec.time_law = TimeLaw::Steady;
                    demoted += 1;
                }
            }
            if demoted > 0 {
                log::warn!(
                    "ignored {demoted} persisted harmonic density law{} that a steady current cannot balance",
                    if demoted == 1 { "" } else { "s" }
                );
            }
        }
        state.restore_densities(densities);
        // Issue #83 colour mapping restores directly, not through the setters:
        // startup must not dirty per field, and corrupt values fall back
        // individually so the rest of the record still loads.
        state.color_ramp = match &self.color_ramp {
            Some(ramp) if ramp.validate().is_ok() => Some(ramp.clone()),
            Some(ramp) => {
                log::warn!("ignoring invalid persisted colour ramp: {ramp:?}");
                None
            }
            None => None,
        };
        state.color_mode = if self.color_mode.validate().is_ok() {
            self.color_mode
        } else {
            log::warn!(
                "ignoring invalid persisted colour mode: {:?}",
                self.color_mode
            );
            ScaleMode::default()
        };
        state.color_quantity = self.color_quantity;
        // Direction colouring has no 3D encoding; a record captured from the
        // 2D view may be restored into 3D, so fold it back to magnitude here.
        if state.color_quantity == ColorQuantity::Direction2d && state.is_3d() {
            log::warn!("ignoring direction colour quantity in the 3D view; using magnitude");
            state.color_quantity = ColorQuantity::Magnitude;
        }
        state.set_plot_samples(self.curve_samples);
        state.set_plot_resolution(self.surface_resolution);
        state.set_plot_width(self.plot_line_width);
        state.set_plot_auto_height(self.plot_auto_height);
        state.set_plot_height_range(self.plot_height_min, self.plot_height_max);
        state.restore_plot_drafts(self.curve_source.clone(), self.surface_source.clone());
        state.set_plot_kind(self.plot_kind);

        // Sources restore last so a legacy formula above is never overwritten
        // by opening the saved scene. Every record is revalidated; a bad one is
        // skipped rather than failing the envelope.
        let mut scene = SourceScene::from_records(&self.source_entries);
        let softening = if self.source_softening.is_finite()
            && (MIN_SOFTENING..=MAX_SOFTENING).contains(&self.source_softening)
        {
            self.source_softening
        } else {
            default_source_softening()
        };
        let _ = scene.set_softening(softening);
        scene.set_display(self.source_display);
        scene.set_allow_inaccurate(self.source_allow_inaccurate);
        let valid_entries = scene.entries.len();
        let requested_sources = self.field_mode == FieldMode::Sources;
        let requested_induction = self.field_mode == FieldMode::Induction;
        let requested_ode = self.field_mode == FieldMode::Ode;
        let requested_materials = self.field_mode == FieldMode::Materials;
        let requested_maxwell = self.field_mode == FieldMode::Maxwell;
        let material_valid = self.restored_material_scene().is_ok();
        let restored_induction = self.restored_induction_scene();
        let induction_valid = restored_induction.is_ok();
        state.set_source_scene(scene);
        // ODE restores lazily: validate through the normal prepare path, store
        // the durable scene/chart/variables, clear any stale solution and mark
        // the first Ode frame pending. Malformed values keep the explicitly
        // stored Ode mode but use the valid default scene and report the error.
        let ode_chart = restored_ode_chart(&self.ode_chart);
        let ode_variables = if self.ode_variables.iter().any(|enabled| *enabled) {
            self.ode_variables
        } else {
            [true, true, false]
        };
        match self.restored_ode_scene() {
            Ok(scene) => {
                let _ = state.restore_ode_fields(scene, ode_chart, ode_variables);
            }
            Err(error) => {
                let _ = state.restore_ode_fields(OdeScene::default(), ode_chart, ode_variables);
                state.ode_error = Some(error);
            }
        }
        // A saved Sources mode without any usable entry cannot draw anything;
        // an invalid saved induction scene cannot either. Both fall back to
        // Formula instead of presenting an empty field. Ode stays selected when
        // it was stored explicitly; its lazy solve owns the error state.
        // Materials is selected only from an explicitly stored mode and a
        // valid record, so a malformed payload can never promote the mode.
        self.restore_material_fields(state);
        // Maxwell restores lazily too: validate and store the durable scene,
        // drop any sampling/BEM cache and mark the first frame pending. A
        // malformed payload always falls back to the default scene and only
        // reports the error for an explicitly stored Maxwell mode, so it can
        // never promote the mode.
        let maxwell_valid = self.restore_maxwell_fields(state).is_none();
        state.set_field_mode(if requested_sources && valid_entries > 0 {
            FieldMode::Sources
        } else if requested_induction && induction_valid {
            FieldMode::Induction
        } else if requested_ode {
            FieldMode::Ode
        } else if requested_materials && material_valid {
            FieldMode::Materials
        } else if requested_maxwell && maxwell_valid {
            FieldMode::Maxwell
        } else {
            FieldMode::Formula
        });
        // The probe family is document state restored directly at the startup
        // seam, after the mode so availability can be checked against it. A
        // missing, corrupt or mode-unavailable token falls back to electric
        // with one warning only when the stored non-empty token could not be
        // used; an unavailable electric already equals the fallback.
        let requested_family = probe_family_from_label(&self.probe_family);
        let sanitised_family = match requested_family {
            Some(family) if probe_family_available(state, family) => family,
            _ => FieldFamily::Electric,
        };
        let adjusted = !self.probe_family.is_empty()
            && (requested_family.is_none() || requested_family != Some(sanitised_family));
        if adjusted {
            log::warn!(
                "persisted probe family {:?} is invalid or unavailable in the restored mode; using {}",
                self.probe_family,
                probe_family_label(sanitised_family)
            );
        }
        state.restore_probe_family(sanitised_family);
        state.source_sampling = None;
        if valid_entries != self.source_entries.len() {
            state.source_notice = Some("Some saved sources were invalid and were skipped".into());
        }
        // The induction scene restores after the mode so its validation result
        // owns `induction_error`; a bad payload falls back to the valid default
        // scene and never fails startup.
        match restored_induction {
            Ok(scene) => {
                let _ = state.set_induction_scene(scene);
                state.induction_prepared = None;
                state.induction_sampling = None;
            }
            Err(error) => {
                state.induction = InductionScene::default();
                state.induction_prepared = None;
                state.induction_sampling = None;
                state.induction_error = Some(error);
            }
        }
        state.mark_dirty();
    }

    /// Restore the durable Materials scene from the record without sampling.
    ///
    /// Validation is the same [`validate_material`] gate the editor uses. A
    /// malformed payload always falls back to the default dielectric sphere;
    /// only an explicitly stored Materials mode surfaces the error, so an old
    /// or damaged record can never promote Materials. The returned `Some` is
    /// the stored error for the caller/tests, already mirrored on the state.
    pub fn restore_material_fields(&self, state: &mut State) -> Option<String> {
        let requested_materials = self.field_mode == FieldMode::Materials;
        match self.restored_material_scene() {
            Ok(scene) => {
                state.material = scene;
                state.material_error = None;
            }
            Err(error) => {
                state.material = MaterialScene::default();
                state.material_error = requested_materials.then_some(error);
            }
        }
        state.material_sampling = None;
        state.material_error.clone()
    }

    /// Rebuild the persisted Materials scene. Every field is serde-defaulted,
    /// so an old payload yields the valid default scene; unknown case/view/axis
    /// tokens and invalid numbers are reported instead of silently promoting
    /// the mode.
    fn restored_material_scene(&self) -> Result<MaterialScene, String> {
        let case = match self.material_case.as_str() {
            "dielectric_sphere" => MaterialCase::DielectricSphere,
            "dielectric_boundary" => MaterialCase::DielectricBoundary,
            "permeable_sphere" => MaterialCase::PermeableSphere,
            "permeable_boundary" => MaterialCase::PermeableBoundary,
            other => return Err(format!("material case is not recognized: {other}")),
        };
        let view = match self.material_view.as_str() {
            "total" => MaterialView::Total,
            "applied" => MaterialView::Applied,
            "response" => MaterialView::Response,
            other => return Err(format!("material view is not recognized: {other}")),
        };
        let interface_axis = match self.material_interface_axis.as_str() {
            "x" => PlaneAxis::X,
            "y" => PlaneAxis::Y,
            "z" => PlaneAxis::Z,
            other => {
                return Err(format!(
                    "material interface axis is not recognized: {other}"
                ));
            }
        };
        let scene = MaterialScene {
            case,
            background_epsilon_r: self.material_background_epsilon_r,
            background_mu_r: self.material_background_mu_r,
            material_epsilon_r: self.material_material_epsilon_r,
            material_mu_r: self.material_material_mu_r,
            radius: self.material_radius,
            interface_axis,
            interface_offset: self.material_interface_offset,
            applied: self.material_applied,
            view,
        };
        validate_material(&scene)
            .map_err(|error| format!("material scene: {}", material_validation_text(&error)))?;
        Ok(scene)
    }

    /// Rebuild the persisted Maxwell scene and hand it to the lazy restore
    /// seam. Validation is the same [`validate_maxwell`] gate the editor uses;
    /// a malformed payload always falls back to the valid default scene, and
    /// only a stored Maxwell mode surfaces the error. The returned `Some` is
    /// that stored error for tests and the caller.
    pub fn restore_maxwell_fields(&self, state: &mut State) -> Option<String> {
        let requested_maxwell = self.field_mode == FieldMode::Maxwell;
        self.restore_maxwell_wave(state);
        match self.restored_maxwell_scene() {
            Ok(scene) => {
                state.restore_maxwell_scene(scene);
                None
            }
            Err(error) => {
                state.restore_maxwell_scene(MaxwellScene::default());
                if requested_maxwell {
                    state.maxwell_error = Some(error.clone());
                    Some(error)
                } else {
                    None
                }
            }
        }
    }

    /// Restore the optional plane wave through the non-dirtying state seam. An
    /// invalid stored spec is dropped with one warning; the normal Maxwell case
    /// sampling then remains available.
    pub fn restore_maxwell_wave(&self, state: &mut State) {
        state.restore_maxwell_wave(self.maxwell_wave);
    }

    /// Rebuild the persisted Maxwell scene. Every field is serde-defaulted, so
    /// an old payload yields the valid default scene. Unknown case/outline
    /// tokens and invalid numbers are reported instead of silently poisoning
    /// the sampler.
    fn restored_maxwell_scene(&self) -> Result<MaxwellScene, String> {
        let case = match self.maxwell_case.as_str() {
            "parallel_plates" => MaxwellCase::ParallelPlates,
            "coaxial_electric" => MaxwellCase::CoaxialElectric,
            "coaxial_magnetic" => MaxwellCase::CoaxialMagnetic,
            "hollow_sphere" => MaxwellCase::HollowSphere,
            "infinite_wire" => MaxwellCase::InfiniteWire,
            "solenoid" => MaxwellCase::Solenoid,
            "image_sphere" => MaxwellCase::ImageSphere,
            "image_plane" => MaxwellCase::ImagePlane,
            "conductor_2d" => MaxwellCase::Conductor2D,
            other => return Err(format!("Maxwell case is not recognized: {other}")),
        };
        let outline = restored_maxwell_outline(&self.maxwell_outline)?;
        let scene = MaxwellScene {
            case,
            e0: self.maxwell_e0,
            b0: self.maxwell_b0,
            separation: self.maxwell_separation,
            radius: self.maxwell_radius,
            outer_radius: self.maxwell_outer_radius,
            charge: self.maxwell_charge,
            charge_height: self.maxwell_charge_height,
            outline,
            applied: self.maxwell_applied,
            elements: self.maxwell_elements,
            allow_inaccurate: self.maxwell_allow_inaccurate,
            view: MaxwellView::Field,
        };
        validate_maxwell(&scene)
            .map_err(|error| format!("Maxwell scene: {}", maxwell_validation_text(&error)))?;
        Ok(scene)
    }

    /// Rebuild the persisted induction scene. Every field is serde-defaulted,
    /// so an old payload yields the valid default scene; unknown case tokens
    /// and invalid numbers are reported instead of silently poisoning the
    /// sampler.
    fn restored_induction_scene(&self) -> Result<InductionScene, String> {
        let case = match self.induction_case.as_str() {
            "current_loop" => InductionCase::CurrentLoop,
            "solenoid" => InductionCase::Solenoid,
            other => return Err(format!("induction case is not recognized: {other}")),
        };
        let scene = InductionScene {
            case,
            center: DVec3::from_array(self.induction_center),
            axis: DVec3::from_array(self.induction_axis),
            radius: self.induction_radius,
            length: self.induction_length,
            turns: self.induction_turns,
            amplitude: self.induction_amplitude,
            frequency: self.induction_frequency,
            display: self.induction_display,
            allow_inaccurate: self.induction_allow_inaccurate,
        };
        scene.validate()?;
        Ok(scene)
    }

    /// Rebuild the persisted ODE scene. Every field is serde-defaulted, so an
    /// old payload yields the valid default scene. Unknown source tokens are
    /// reported; all numeric/equation validity is decided by
    /// [`State::restore_ode_fields`] through the normal prepare path.
    fn restored_ode_scene(&self) -> Result<OdeScene, String> {
        let source = match self.ode_source.as_str() {
            "equations" => OdeSource::Equations,
            "current_field" => OdeSource::CurrentField,
            other => return Err(format!("ODE source is not recognized: {other}")),
        };
        Ok(OdeScene {
            source,
            dimension: usize::from(self.ode_dimension),
            equations: self.ode_equations.clone(),
            seeds: self
                .ode_seeds
                .iter()
                .map(|position| OdeSeed {
                    position: *position,
                })
                .collect(),
            duration: self.ode_duration,
        })
    }
}

/// Map the persisted chart token; unknown tokens restore the phase portrait.
fn restored_ode_chart(token: &str) -> OdeChart {
    match token {
        "time_series" => OdeChart::TimeSeries,
        _ => OdeChart::Phase,
    }
}

fn ode_chart_label(chart: OdeChart) -> &'static str {
    match chart {
        OdeChart::Phase => "phase",
        OdeChart::TimeSeries => "time_series",
    }
}

fn ode_source_label(source: OdeSource) -> &'static str {
    match source {
        OdeSource::Equations => "equations",
        OdeSource::CurrentField => "current_field",
    }
}

fn material_case_label(case: MaterialCase) -> &'static str {
    match case {
        MaterialCase::DielectricSphere => "dielectric_sphere",
        MaterialCase::DielectricBoundary => "dielectric_boundary",
        MaterialCase::PermeableSphere => "permeable_sphere",
        MaterialCase::PermeableBoundary => "permeable_boundary",
    }
}

fn material_view_label(view: MaterialView) -> &'static str {
    match view {
        MaterialView::Total => "total",
        MaterialView::Applied => "applied",
        MaterialView::Response => "response",
    }
}

fn material_axis_label(axis: PlaneAxis) -> &'static str {
    match axis {
        PlaneAxis::X => "x",
        PlaneAxis::Y => "y",
        PlaneAxis::Z => "z",
    }
}

fn material_validation_text(error: &MaterialError) -> String {
    match error {
        MaterialError::InvalidScene(message) | MaterialError::Unsupported(message) => {
            message.clone()
        }
    }
}

fn maxwell_case_label(case: MaxwellCase) -> &'static str {
    match case {
        MaxwellCase::ParallelPlates => "parallel_plates",
        MaxwellCase::CoaxialElectric => "coaxial_electric",
        MaxwellCase::CoaxialMagnetic => "coaxial_magnetic",
        MaxwellCase::HollowSphere => "hollow_sphere",
        MaxwellCase::InfiniteWire => "infinite_wire",
        MaxwellCase::Solenoid => "solenoid",
        MaxwellCase::ImageSphere => "image_sphere",
        MaxwellCase::ImagePlane => "image_plane",
        MaxwellCase::Conductor2D => "conductor_2d",
    }
}

/// Serialize both outline kinds through one flat token, including their size,
/// so `maxwell_outline` remains the single design field for the cross-section.
fn maxwell_outline_label(outline: ConductorOutline) -> String {
    match outline {
        ConductorOutline::Circle { radius } => format!("circle:{radius}"),
        ConductorOutline::Square { half_extent } => format!("square:{half_extent}"),
    }
}

fn restored_maxwell_outline(token: &str) -> Result<ConductorOutline, String> {
    let (kind, size) = token
        .split_once(':')
        .ok_or_else(|| format!("Maxwell outline is not recognized: {token}"))?;
    let size = size
        .parse::<f64>()
        .map_err(|_| format!("Maxwell outline size is not a number: {token}"))?;
    match kind {
        "circle" => Ok(ConductorOutline::Circle { radius: size }),
        "square" => Ok(ConductorOutline::Square { half_extent: size }),
        other => Err(format!("Maxwell outline is not recognized: {other}")),
    }
}

fn maxwell_validation_text(error: &MaxwellError) -> String {
    match error {
        MaxwellError::InvalidScene(message)
        | MaxwellError::Unsupported(message)
        | MaxwellError::Unavailable(message) => message.clone(),
    }
}

fn default_material_case() -> String {
    material_case_label(MaterialScene::default().case).to_string()
}

fn default_material_background_epsilon_r() -> f64 {
    MaterialScene::default().background_epsilon_r
}

fn default_material_background_mu_r() -> f64 {
    MaterialScene::default().background_mu_r
}

fn default_material_material_epsilon_r() -> f64 {
    MaterialScene::default().material_epsilon_r
}

fn default_material_material_mu_r() -> f64 {
    MaterialScene::default().material_mu_r
}

fn default_material_radius() -> f64 {
    MaterialScene::default().radius
}

fn default_material_interface_axis() -> String {
    material_axis_label(MaterialScene::default().interface_axis).to_string()
}

fn default_material_applied() -> [f64; 3] {
    MaterialScene::default().applied
}

fn default_material_view() -> String {
    material_view_label(MaterialScene::default().view).to_string()
}

fn default_maxwell_case() -> String {
    maxwell_case_label(MaxwellScene::default().case).to_string()
}

fn default_maxwell_e0() -> f64 {
    MaxwellScene::default().e0
}

fn default_maxwell_b0() -> f64 {
    MaxwellScene::default().b0
}

fn default_maxwell_separation() -> f64 {
    MaxwellScene::default().separation
}

fn default_maxwell_radius() -> f64 {
    MaxwellScene::default().radius
}

fn default_maxwell_outer_radius() -> f64 {
    MaxwellScene::default().outer_radius
}

fn default_maxwell_charge() -> f64 {
    MaxwellScene::default().charge
}

fn default_maxwell_charge_height() -> f64 {
    MaxwellScene::default().charge_height
}

fn default_maxwell_outline() -> String {
    maxwell_outline_label(MaxwellScene::default().outline)
}

fn default_maxwell_applied() -> [f64; 2] {
    MaxwellScene::default().applied
}

fn default_maxwell_elements() -> u32 {
    MaxwellScene::default().elements
}

fn default_ode_source() -> String {
    ode_source_label(OdeScene::default().source).to_string()
}

fn default_ode_dimension() -> u8 {
    OdeScene::default().dimension as u8
}

fn default_ode_equations() -> [String; 3] {
    OdeScene::default().equations
}

fn default_ode_seeds() -> Vec<[f64; 3]> {
    OdeScene::default()
        .seeds
        .into_iter()
        .map(|seed| seed.position)
        .collect()
}

fn default_ode_duration() -> f64 {
    OdeScene::default().duration
}

fn default_ode_chart() -> String {
    ode_chart_label(OdeChart::default()).to_string()
}

fn default_ode_variables() -> [bool; 3] {
    [true, true, false]
}

/// `value` when finite, `fallback` otherwise.
pub fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// The `arrows` field's serde default: older records predate the Show arrows
/// toggle, and arrows were always drawn then.
fn default_arrows() -> bool {
    true
}

/// Older records omit both reference toggles; each missing field stays visible
/// independently (a plain bool serde default would instead hide it).
fn default_reference_visible() -> bool {
    true
}

/// The `arrow_opacity` serde default (ticket 36): records written before the
/// control existed were fully opaque.
fn default_arrow_opacity() -> f32 {
    crate::viewmodel::MAX_ARROW_OPACITY
}

/// The `plane_dim_opacity` serde default (ticket 36): the historical off-plane
/// alpha, reused directly so the two cannot drift.
fn default_plane_dim_opacity() -> f32 {
    crate::plane::DIM_ALPHA
}

/// The `proportional_exponent` serde default (ticket 57): records written
/// before the contrast existed used the linear mapping.
fn default_proportional_exponent() -> f32 {
    crate::viewmodel::DEFAULT_PROPORTIONAL_EXPONENT
}

/// The `source_softening` serde default: the evaluator's historical `0.1`.
fn default_source_softening() -> f64 {
    0.1
}

/// The induction scene defaults (design §8): a valid, unsampled current loop.
fn default_induction_case() -> String {
    "current_loop".to_string()
}

fn default_induction_center() -> [f64; 3] {
    [0.0, 0.0, 0.0]
}

fn default_induction_axis() -> [f64; 3] {
    [0.0, 0.0, 1.0]
}

fn default_induction_radius() -> f64 {
    1.0
}

fn default_induction_turns() -> u32 {
    1
}

fn default_induction_amplitude() -> f64 {
    1.0
}

fn default_induction_frequency() -> f64 {
    0.25
}

fn induction_case_label(case: InductionCase) -> &'static str {
    match case {
        InductionCase::CurrentLoop => "current_loop",
        InductionCase::Solenoid => "solenoid",
    }
}

/// Decode the persisted object style field by field: a valid token goes through
/// [`ObjectStyle`]'s lenient decoder (which repairs an unknown name to `Solid`),
/// while any other stored shape — for example a number — consumes its own value
/// and becomes `Solid` with one warning. Neither case can fail the envelope.
fn deserialize_object_style<'de, D>(deserializer: D) -> Result<ObjectStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum StyleOrSolid {
        Style(ObjectStyle),
        Invalid(serde::de::IgnoredAny),
    }

    Ok(match StyleOrSolid::deserialize(deserializer)? {
        StyleOrSolid::Style(style) => style,
        StyleOrSolid::Invalid(_) => {
            log::warn!("persisted object style is unreadable; using the Solid default");
            ObjectStyle::Solid
        }
    })
}

/// Decode the persisted up axis field by field: a valid token goes through
/// [`UpAxis`]'s lenient decoder (which repairs an unknown name to `Z`), while
/// any other stored shape — for example a number — consumes its own value and
/// becomes `Z` with one warning. Neither case can fail the envelope.
fn deserialize_up_axis<'de, D>(deserializer: D) -> Result<UpAxis, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum AxisOrZ {
        Axis(UpAxis),
        Invalid(serde::de::IgnoredAny),
    }

    Ok(match AxisOrZ::deserialize(deserializer)? {
        AxisOrZ::Axis(axis) => axis,
        AxisOrZ::Invalid(_) => {
            log::warn!("persisted up axis is unreadable; using the Z default");
            UpAxis::Z
        }
    })
}

/// Decode the Presets-panel expansion leniently: a missing record keeps the
/// all-expanded default, and a record of the wrong shape is swallowed instead
/// of failing the whole settings envelope.
fn deserialize_preset_ui<'de, D>(deserializer: D) -> Result<PresetUiRecord, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum RecordOrDefault {
        Record(PresetUiRecord),
        Invalid(serde::de::IgnoredAny),
    }

    Ok(match RecordOrDefault::deserialize(deserializer)? {
        RecordOrDefault::Record(record) => record,
        RecordOrDefault::Invalid(_) => PresetUiRecord::default(),
    })
}

/// Decode the source list element by element so one wrong-typed or corrupt
/// record cannot fail the whole settings envelope. A skipped element becomes
/// the empty default record, which [`SourceRecord::to_source`] rejects; the
/// restore path still sees the original length and can raise its notice.
fn deserialize_source_entries<'de, D>(deserializer: D) -> Result<Vec<SourceRecord>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum LenientRecord {
        Record(SourceRecord),
        Invalid(serde::de::IgnoredAny),
    }

    let records = Vec::<LenientRecord>::deserialize(deserializer)?;
    Ok(records
        .into_iter()
        .map(|record| match record {
            LenientRecord::Record(record) => record,
            LenientRecord::Invalid(_) => SourceRecord::default(),
        })
        .collect())
}

/// Decode an accuracy opt-in leniently: only a literal `true` enables it, and
/// every other value (wrong type, corrupt, missing) becomes false without
/// failing the whole settings envelope.
fn deserialize_bool_or_false<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    /// Matches a bool, or swallows any other value as `false`.
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum BoolOrFalse {
        Bool(bool),
        Other(serde::de::IgnoredAny),
    }

    Ok(match BoolOrFalse::deserialize(deserializer)? {
        BoolOrFalse::Bool(value) => value,
        BoolOrFalse::Other(_) => false,
    })
}

/// The `particle_size` serde default (ticket 30): records written before the
/// size control used the base grid-spacing fraction.
fn default_particle_size() -> f32 {
    crate::viewmodel::DEFAULT_PARTICLE_SIZE
}

fn default_particle_opacity() -> f32 {
    DEFAULT_PARTICLE_OPACITY
}

fn default_particle_fixed_color() -> [u8; 4] {
    DEFAULT_PARTICLE_FIXED_COLOR
}

fn default_particle_trail_fade() -> f32 {
    DEFAULT_TRAIL_FADE
}

fn default_particle_trail_width() -> f32 {
    DEFAULT_TRAIL_WIDTH
}

fn default_particle_loop_on_stagnation() -> bool {
    true
}

fn default_particle_stagnation_seconds() -> f32 {
    DEFAULT_STAGNATION_SECONDS
}

/// Decode a ticket-60 `left_click_action`, falling back to the default for an
/// unknown variant instead of failing the whole settings envelope.
///
/// The derived enum decoder rejects an unknown unit variant; a corrupt or
/// future record must still load every other setting. RON gives the variant
/// name through the enum access; matching it by hand keeps the fallback local.
fn deserialize_left_click_action<'de, D>(deserializer: D) -> Result<LeftClickAction, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{EnumAccess, VariantAccess, Visitor};

    struct VariantName(String);

    impl<'de> serde::Deserialize<'de> for VariantName {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct NameVisitor;

            impl<'de> Visitor<'de> for NameVisitor {
                type Value = VariantName;

                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a left-click action name")
                }

                fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(VariantName(value.to_owned()))
                }
            }

            deserializer.deserialize_identifier(NameVisitor)
        }
    }

    struct ActionVisitor;

    impl<'de> Visitor<'de> for ActionVisitor {
        type Value = LeftClickAction;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a left-click action")
        }

        fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
        where
            A: EnumAccess<'de>,
        {
            let (name, variant) = data.variant::<VariantName>()?;
            variant.unit_variant()?;
            Ok(match name.0.as_str() {
                "AddParticles" => LeftClickAction::AddParticles,
                _ => LeftClickAction::SeedStreamline,
            })
        }
    }

    deserializer.deserialize_enum(
        "LeftClickAction",
        &["SeedStreamline", "AddParticles"],
        ActionVisitor,
    )
}

/// Decode the ticket-14 `normalize` boolean, which was stored as a bare
/// `bool`, into the `Option` the legacy-aware [`ViewRecord`] carries. Ticket-33
/// records omit the key entirely, so this only runs for old settings.
fn deserialize_legacy_normalize<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    bool::deserialize(deserializer).map(Some)
}

/// Load the settings envelope, migrating a ticket-07 bare `AppStyle` record and
/// falling back to defaults for absent or corrupt storage. Never panics.
pub fn load(storage: Option<&dyn eframe::Storage>) -> AppSettings {
    let Some(storage) = storage else {
        return AppSettings::default();
    };
    if let Some(settings) = eframe::get_value::<AppSettings>(storage, eframe::APP_KEY) {
        return settings.sanitized();
    }
    if let Some(style) = eframe::get_value::<AppStyle>(storage, eframe::APP_KEY) {
        log::warn!("persisted settings use the ticket-07 shape; migrating appearance only");
        return AppSettings {
            style,
            ..AppSettings::default()
        };
    }
    if storage.get_string(eframe::APP_KEY).is_some() {
        log::warn!("persisted settings are corrupt; falling back to defaults");
    }
    AppSettings::default()
}

/// Persist the settings envelope under eframe's app key.
pub fn save(storage: &mut dyn eframe::Storage, settings: &AppSettings) {
    eframe::set_value(storage, eframe::APP_KEY, settings);
}

/// Serialize the plane wave as plain arrays, keeping `glam` out of the stored
/// schema. Missing/corrupt members decode as zeros and are then dropped by the
/// state restore's slice A validation.
mod wave_spec_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::maxwell_wave::WaveSpec;

    #[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
    #[serde(default)]
    struct WaveSpecWire {
        amplitude: f64,
        wavelength: f64,
        direction: [f32; 3],
        polarization: [f32; 3],
        phase: f64,
    }

    impl From<WaveSpec> for WaveSpecWire {
        fn from(spec: WaveSpec) -> Self {
            Self {
                amplitude: spec.amplitude,
                wavelength: spec.wavelength,
                direction: spec.direction.to_array(),
                polarization: spec.polarization.to_array(),
                phase: spec.phase,
            }
        }
    }

    impl From<WaveSpecWire> for WaveSpec {
        fn from(spec: WaveSpecWire) -> Self {
            Self {
                amplitude: spec.amplitude,
                wavelength: spec.wavelength,
                direction: glam::Vec3::from_array(spec.direction),
                polarization: glam::Vec3::from_array(spec.polarization),
                phase: spec.phase,
            }
        }
    }

    pub fn serialize<S: Serializer>(
        wave: &Option<WaveSpec>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        wave.map(WaveSpecWire::from).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<WaveSpec>, D::Error> {
        Ok(Option::<WaveSpecWire>::deserialize(deserializer)?.map(WaveSpec::from))
    }
}
