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

use crate::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use crate::colormap::Colormap;
use crate::particles::{
    DEFAULT_PARTICLE_FIXED_COLOR, DEFAULT_PARTICLE_OPACITY, DEFAULT_STAGNATION_SECONDS,
    DEFAULT_TRAIL_FADE, DEFAULT_TRAIL_WIDTH, MAX_PARTICLES, ParticleAppearance,
    ParticleColorMode,
};
use crate::ui::layout::LayoutState;
use crate::ui::sidebar::{self, Controls};
use crate::ui::style_panel::AppStyle;
use crate::viewmodel::{ArrowLengthMode, State, StreamlineSeedMode};
use glam::Vec3;

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
}

impl AppSettings {
    /// Clamp corrupt numbers from storage into usable ranges: style into the
    /// Settings slider bounds, layout widths into the draggable range.
    pub fn sanitized(mut self) -> Self {
        self.style = self.style.sanitized();
        self.layout = self.layout.sanitized();
        self
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
    #[serde(default, skip_serializing, deserialize_with = "deserialize_legacy_normalize")]
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
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
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
    /// Snapshot the effective view state (the state's clamped/swapped values,
    /// not the widgets' in-flight mirror).
    pub fn capture(state: &State) -> Self {
        Self {
            formula: state.formula.clone(),
            mode_3d: state.mode_3d,
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
            auto_fit: state.auto_fit,
            color_min: state.color_min,
            color_max: state.color_max,
            arrows: state.arrows,
            arrow_style: state.arrow_style,
            arrow_style_2d: state.arrow_style_2d,
            arrow_params: state.arrow_params,
            streamlines: state.streamlines,
            seed_mode: state.seed_mode,
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
        let defaults = Controls::new();
        state.set_mode(self.mode_3d);
        state.set_formula(self.formula.clone());

        controls.density = self.density;
        controls.arrow_scale = finite_or(self.arrow_scale, defaults.arrow_scale);
        controls.arrow_opacity = finite_or(self.arrow_opacity, defaults.arrow_opacity);
        controls.plane_dim_opacity =
            finite_or(self.plane_dim_opacity, defaults.plane_dim_opacity);
        // Legacy records carried a `normalize` bool instead of a mode; it wins
        // when present so old settings land in the matching mode, and new
        // records (where it is absent) use the stored three-way mode.
        controls.arrow_length_mode =
            self.normalize.map(ArrowLengthMode::from_normalize).unwrap_or(self.arrow_length_mode);
        controls.magnitude_scale = finite_or(self.magnitude_scale, defaults.magnitude_scale);
        controls.proportional_exponent =
            finite_or(self.proportional_exponent, defaults.proportional_exponent);
        controls.colormap = self.colormap;
        controls.auto_fit = self.auto_fit;
        controls.color_min = finite_or(self.color_min, defaults.color_min);
        controls.color_max = finite_or(self.color_max, defaults.color_max);
        controls.arrows = self.arrows;
        controls.arrow_style = self.arrow_style;
        controls.arrow_style_2d = self.arrow_style_2d;
        controls.arrow_params = self.arrow_params.sanitized();
        controls.streamlines = self.streamlines;
        controls.seed_mode = self.seed_mode;
        controls.particles = self.particles;
        controls.particle_size = finite_or(self.particle_size, defaults.particle_size);
        controls.particle_count = self.particle_count.min(MAX_PARTICLES as u32);
        controls.particle_appearance = ParticleAppearance {
            opacity: finite_or(
                self.particle_opacity,
                defaults.particle_appearance.opacity,
            ),
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
        sidebar::apply(state, controls);
    }
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
