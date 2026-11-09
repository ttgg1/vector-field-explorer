// Moved out of `src/persistence.rs`'s inline module (ticket-14 review I-4).
// Covers the single APP_KEY envelope, ticket-07 migration, defaults, hostile
// values, view-state restoration and the style clamp (review I-3).

use eframe::Storage as _;
use glam::Vec3;

use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::persistence::{AppSettings, ViewRecord, load, save};
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::ui::style_panel::{
    AppStyle, MAX_FONT_SIZE, MAX_ROUNDING, MAX_SPACING, MIN_ROUNDING,
};
use vector_field_explorer::viewmodel::{MAX_DOMAIN_EXTENT, MIN_DOMAIN_EXTENT, State};

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
    let (state, _controls) = configured();
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
        view: ViewRecord::capture(&state),
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
fn corrupt_style_values_are_clamped_on_load() {
    // Review I-3: a hand-edited record bypasses the Settings sliders, so load
    // must clamp into the slider ranges (and replace non-finite numbers).
    let mut settings = sample_settings();
    settings.style.font_size = f32::INFINITY;
    settings.style.spacing = 1e9;
    settings.style.rounding = -5.0;
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert_eq!(loaded.style.font_size, 13.0, "an infinite font size falls back to the default");
    assert_eq!(loaded.style.spacing, MAX_SPACING, "huge spacing clamps to the max");
    assert_eq!(loaded.style.rounding, MIN_ROUNDING, "negative rounding clamps to the min");

    let mut settings = sample_settings();
    settings.style.font_size = f32::NAN;
    settings.style.spacing = f32::NEG_INFINITY;
    settings.style.rounding = f32::NAN;
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert_eq!(loaded.style.font_size, 13.0, "a NaN font size falls back to the default");
    assert_eq!(loaded.style.spacing, 8.0, "an infinite spacing falls back to the default");
    assert_eq!(loaded.style.rounding, 6.0, "a NaN rounding falls back to the default");
    assert!(loaded.style.font_size <= MAX_FONT_SIZE);
    assert!(loaded.style.rounding <= MAX_ROUNDING);
}

#[test]
fn every_view_value_round_trips() {
    let (state, controls) = configured();
    let record = ViewRecord::capture(&state);
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
    assert_eq!(controls.density, state.density, "the captured mirror stays in sync");
    assert!(restored_state.dirty, "a restored view asks for one scene rebuild");
}

#[test]
fn default_record_matches_a_fresh_two_d_state() {
    let state = State::new(false);
    assert_eq!(ViewRecord::capture(&state), ViewRecord::default());
    assert_eq!(ViewRecord::default().formula, "(x, y)");
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
    assert!(state.density <= 200, "density is clamped");
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

#[test]
fn restore_recovers_a_reversed_out_of_range_domain_per_axis() {
    // A hand-edited or older record can carry extents outside the editor bound
    // and a non-finite component. Restore must hand each component through the
    // real `sidebar::apply` path: finite values clamp, reversed pairs still
    // swap, and a non-finite component keeps the previous effective value
    // rather than throwing away its finite neighbours.
    let mut state = State::new(false);
    assert!(
        state.set_domain(Vec3::new(-0.5, -0.25, -2.0), Vec3::new(0.5, 0.25, 2.0)),
        "seed a non-default previous box",
    );
    let mut controls = Controls::new();
    let record = ViewRecord {
        domain_min: [500.0, -50.0, f32::NAN],
        domain_max: [-500.0, 50.0, f32::NAN],
        ..Default::default()
    };

    record.restore(&mut state, &mut controls);

    assert_eq!(
        (state.domain.min.x, state.domain.max.x),
        (MIN_DOMAIN_EXTENT, MAX_DOMAIN_EXTENT),
        "out-of-range x clamps to the documented bound, then the reversed pair swaps",
    );
    assert_eq!(
        (state.domain.min.y, state.domain.max.y),
        (-50.0, 50.0),
        "in-range y survives and keeps its direction",
    );
    assert_eq!(
        (state.domain.min.z, state.domain.max.z),
        (-2.0, 2.0),
        "NaN z keeps the previous effective z extent",
    );
    assert_eq!(controls.domain_min, state.domain.min, "the mirror is refreshed");
    assert_eq!(controls.domain_max, state.domain.max, "…for both corners");
}
