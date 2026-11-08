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

use crate::colormap::Colormap;
use crate::ui::layout::LayoutState;
use crate::ui::sidebar::{self, Controls};
use crate::ui::style_panel::AppStyle;
use crate::viewmodel::State;
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
    pub normalize: bool,
    pub colormap: Colormap,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    pub streamlines: bool,
    pub particles: bool,
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
            normalize: state.normalize,
            colormap: state.colormap,
            auto_fit: state.auto_fit,
            color_min: state.color_min,
            color_max: state.color_max,
            streamlines: state.streamlines,
            particles: state.particles,
            speed: state.speed,
        }
    }

    /// Apply the record through the viewmodel setters (they own clamping,
    /// swapping and the dirty flag), then refresh the widget mirror with
    /// [`sidebar::apply`]. Non-finite numbers fall back to the defaults so a
    /// corrupt record cannot poison the sampling grid.
    pub fn restore(&self, state: &mut State, controls: &mut Controls) {
        let defaults = Controls::new();
        state.set_mode(self.mode_3d);
        state.set_formula(self.formula.clone());

        controls.density = self.density;
        controls.arrow_scale = finite_or(self.arrow_scale, defaults.arrow_scale);
        controls.normalize = self.normalize;
        controls.colormap = self.colormap;
        controls.auto_fit = self.auto_fit;
        controls.color_min = finite_or(self.color_min, defaults.color_min);
        controls.color_max = finite_or(self.color_max, defaults.color_max);
        controls.streamlines = self.streamlines;
        controls.particles = self.particles;
        controls.speed = finite_or(self.speed, defaults.speed);
        controls.domain_min = vec3_or(self.domain_min, defaults.domain_min);
        controls.domain_max = vec3_or(self.domain_max, defaults.domain_max);
        sidebar::apply(state, controls);
    }
}

/// `value` when finite, `fallback` otherwise.
pub fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// A finite `[f32; 3]`, or `fallback` if any component is corrupt.
fn vec3_or(value: [f32; 3], fallback: Vec3) -> Vec3 {
    if value.iter().all(|v| v.is_finite()) {
        Vec3::from_array(value)
    } else {
        fallback
    }
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
