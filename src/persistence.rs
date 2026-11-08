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
    /// Clamp corrupt numbers from storage into usable ranges.
    pub fn sanitized(mut self) -> Self {
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
        let state = State::new(false);
        let controls = Controls::new();
        Self::capture(&state, &controls)
    }
}

impl ViewRecord {
    /// Snapshot the effective view state (the state's clamped/swapped values,
    /// not the widgets' in-flight mirror).
    pub fn capture(state: &State, controls: &Controls) -> Self {
        let _ = controls;
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
        return AppSettings {
            style,
            ..AppSettings::default()
        };
    }
    AppSettings::default()
}

/// Persist the settings envelope under eframe's app key.
pub fn save(storage: &mut dyn eframe::Storage, settings: &AppSettings) {
    eframe::set_value(storage, eframe::APP_KEY, settings);
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage as _;

    /// In-memory stand-in for eframe's disk-backed storage.
    #[derive(Default)]
    struct MemoryStorage {
        values: std::collections::HashMap<String, String>,
    }

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.values.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.values.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.values.remove(key);
        }

        fn flush(&mut self) {}
    }

    fn configured() -> (State, Controls) {
        let mut state = State::new(false);
        let mut controls = Controls::new();
        state.set_mode(true);
        state.set_formula("(x, y, z)".to_string());
        controls.density = 12;
        controls.arrow_scale = 2.5;
        controls.normalize = true;
        controls.colormap = Colormap::Mono;
        controls.auto_fit = false;
        controls.color_min = -2.0;
        controls.color_max = 3.0;
        controls.streamlines = true;
        controls.particles = true;
        controls.speed = 2.2;
        controls.domain_min = Vec3::new(-2.0, -1.0, -0.5);
        controls.domain_max = Vec3::new(2.0, 1.0, 0.5);
        sidebar::apply(&mut state, &mut controls);
        (state, controls)
    }

    fn sample_settings() -> AppSettings {
        let (state, controls) = configured();
        AppSettings {
            style: AppStyle {
                theme_dark: false,
                accent: eframe::egui::Color32::from_rgb(1, 2, 3),
                rounding: 4.0,
                spacing: 9.0,
                font_size: 14.0,
            },
            layout: LayoutState {
                controls_expanded: false,
                presets_expanded: true,
                controls_width: 300.0,
                presets_width: 210.0,
                help_visible: false,
            },
            view: ViewRecord::capture(&state, &controls),
        }
    }

    #[test]
    fn settings_round_trip_through_eframe_storage() {
        let settings = sample_settings();
        let mut storage = MemoryStorage::default();
        save(&mut storage, &settings);
        assert_eq!(load(Some(&storage)), settings);
    }

    #[test]
    fn absent_storage_falls_back_to_defaults() {
        assert_eq!(load(None), AppSettings::default());
        let empty = MemoryStorage::default();
        assert_eq!(load(Some(&empty)), AppSettings::default());
    }

    #[test]
    fn corrupt_storage_falls_back_to_defaults() {
        let mut storage = MemoryStorage::default();
        storage.set_string(eframe::APP_KEY, "this is not ron".to_owned());
        assert_eq!(load(Some(&storage)), AppSettings::default());
    }

    #[test]
    fn a_ticket_07_style_only_record_still_loads_its_style() {
        // The old key held a bare AppStyle. It must fail the envelope decode
        // (style is a required field) and be migrated, not silently dropped.
        let style = AppStyle {
            theme_dark: false,
            accent: eframe::egui::Color32::from_rgb(9, 8, 7),
            rounding: 3.0,
            spacing: 7.0,
            font_size: 12.0,
        };
        let mut storage = MemoryStorage::default();
        eframe::set_value(&mut storage, eframe::APP_KEY, &style);
        let loaded = load(Some(&storage));
        assert_eq!(loaded.style, style, "the ticket-07 appearance survives the migration");
        assert_eq!(loaded.layout, LayoutState::default(), "new layout fields start at defaults");
        assert_eq!(loaded.view, ViewRecord::default(), "new view fields start at defaults");
    }

    #[test]
    fn corrupt_widths_are_sanitized_on_load() {
        let mut settings = sample_settings();
        settings.layout.controls_width = f32::NAN;
        settings.layout.presets_width = 99_999.0;
        let mut storage = MemoryStorage::default();
        save(&mut storage, &settings);
        let loaded = load(Some(&storage));
        assert!(loaded.layout.controls_width.is_finite(), "a NaN width is replaced");
        assert!(loaded.layout.presets_width.is_finite());
    }

    #[test]
    fn every_view_value_round_trips() {
        let (state, controls) = configured();
        let record = ViewRecord::capture(&state, &controls);
        let mut restored_state = State::new(false);
        let mut restored_controls = Controls::new();
        record.restore(&mut restored_state, &mut restored_controls);

        assert_eq!(restored_state.formula, state.formula, "formula");
        assert_eq!(restored_state.mode_3d, state.mode_3d, "2D/3D mode");
        assert_eq!(restored_state.domain, state.domain, "domain");
        assert_eq!(restored_state.density, state.density, "density");
        assert_eq!(restored_state.arrow_scale, state.arrow_scale, "arrow scale");
        assert_eq!(restored_state.normalize, state.normalize, "normalize");
        assert_eq!(restored_state.colormap, state.colormap, "colormap");
        assert_eq!(restored_state.auto_fit, state.auto_fit, "auto-fit");
        assert_eq!(restored_state.color_min, state.color_min, "color min");
        assert_eq!(restored_state.color_max, state.color_max, "color max");
        assert_eq!(restored_state.streamlines, state.streamlines, "streamlines");
        assert_eq!(restored_state.particles, state.particles, "particles");
        assert_eq!(restored_state.speed, state.speed, "speed");
        // The widget mirror is refreshed so the first sidebar frame agrees.
        assert_eq!(restored_controls.density, state.density);
        assert_eq!(restored_controls.domain_min, state.domain.min);
        assert_eq!(restored_controls.domain_max, state.domain.max);
        assert!(restored_state.dirty, "a restored view asks for one scene rebuild");
    }

    #[test]
    fn default_record_matches_a_fresh_two_d_state() {
        let state = State::new(false);
        let controls = Controls::new();
        assert_eq!(ViewRecord::capture(&state, &controls), ViewRecord::default());
        assert_eq!(ViewRecord::default().formula, crate::viewmodel::FORMULA_2D);
        assert!(!ViewRecord::default().mode_3d);
    }

    #[test]
    fn restore_clamps_and_sanitizes_a_hostile_record() {
        let record = ViewRecord {
            density: 99_999,
            arrow_scale: f32::NAN,
            speed: f32::INFINITY,
            color_min: f32::NAN,
            color_max: f32::NEG_INFINITY,
            domain_min: [f32::NAN, 0.0, 0.0],
            domain_max: [f32::NAN, 0.0, 0.0],
            ..Default::default()
        };
        let mut state = State::new(false);
        let mut controls = Controls::new();
        record.restore(&mut state, &mut controls);
        assert!(state.density <= crate::viewmodel::MAX_DENSITY_2D, "density is clamped");
        assert!(state.arrow_scale.is_finite(), "a NaN arrow scale falls back");
        assert!(state.speed.is_finite(), "an infinite speed falls back");
        assert!(state.color_min.is_finite() && state.color_max.is_finite());
        assert!(state.domain.min.is_finite() && state.domain.max.is_finite());
        assert!(state.domain.min.x < state.domain.max.x, "the domain stays a real box");
    }

    #[test]
    fn a_bad_formula_in_the_record_keeps_the_last_good_field() {
        let record = ViewRecord {
            formula: ")(".to_string(),
            ..Default::default()
        };
        let mut state = State::new(false);
        let mut controls = Controls::new();
        record.restore(&mut state, &mut controls);
        assert!(!state.parse_error.is_empty(), "the bad stored formula is reported, not hidden");
        assert!(state.field.eval3(Vec3::ZERO).is_some(), "a usable field stays in place");
    }
}
