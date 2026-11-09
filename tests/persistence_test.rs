// Moved out of `src/persistence.rs`'s inline module (ticket-14 review I-4).
// Covers the single APP_KEY envelope, ticket-07 migration, defaults, hostile
// values, view-state restoration and the style clamp (review I-3).

use eframe::Storage as _;
use glam::Vec3;

use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::persistence::{AppSettings, ViewRecord, load, save};
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::ui::style_panel::{
    AppStyle, MAX_FONT_SIZE, MAX_ROUNDING, MAX_SPACING, MIN_ROUNDING,
};
use vector_field_explorer::viewmodel::{
    ArrowLengthMode, MAX_DOMAIN_EXTENT, MAX_MAGNITUDE_SCALE, MIN_DOMAIN_EXTENT,
    MIN_MAGNITUDE_SCALE, State,
};
use vector_field_explorer::VfApp;

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

/// The ticket-14 `ViewRecord` shape, before ticket 33 replaced the `normalize`
/// bool with an arrow-length mode and added `magnitude_scale`. Serializing this
/// into real eframe storage makes `load` see exactly what an old build wrote,
/// including the two missing new fields.
#[derive(serde::Serialize)]
struct LegacyViewRecord {
    formula: String,
    mode_3d: bool,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    density: u32,
    arrow_scale: f32,
    normalize: bool,
    colormap: Colormap,
    auto_fit: bool,
    color_min: f32,
    color_max: f32,
    streamlines: bool,
    particles: bool,
    speed: f32,
}

#[derive(serde::Serialize)]
struct LegacySettings {
    style: AppStyle,
    layout: LayoutState,
    view: LegacyViewRecord,
}

/// The exact `ViewRecord` shape a build before ticket 61 wrote: it carries the
/// 3D `arrow_style` and geometry but no `arrow_style_2d`. Writing this through
/// real eframe storage proves the new field loads from its serde default while
/// the existing 3D selection survives.
#[derive(serde::Serialize)]
struct Pre2DViewRecord {
    formula: String,
    mode_3d: bool,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    density: u32,
    arrow_scale: f32,
    arrow_length_mode: ArrowLengthMode,
    magnitude_scale: f32,
    colormap: Colormap,
    auto_fit: bool,
    color_min: f32,
    color_max: f32,
    arrows: bool,
    arrow_style: ArrowStyle,
    arrow_params: ArrowParams,
    streamlines: bool,
    particles: bool,
    speed: f32,
}

#[derive(serde::Serialize)]
struct Pre2DSettings {
    style: AppStyle,
    layout: LayoutState,
    view: Pre2DViewRecord,
}

fn pre_2d_style_settings() -> Pre2DSettings {
    Pre2DSettings {
        style: AppStyle::default(),
        layout: LayoutState::default(),
        view: Pre2DViewRecord {
            formula: "(x, y, z)".to_string(),
            mode_3d: true,
            domain_min: [-1.0, -1.0, -1.0],
            domain_max: [1.0, 1.0, 1.0],
            density: 12,
            arrow_scale: 1.5,
            arrow_length_mode: ArrowLengthMode::Uniform,
            magnitude_scale: 1.0,
            colormap: Colormap::Turbo,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            arrows: true,
            arrow_style: ArrowStyle::Pyramid,
            arrow_params: ArrowParams::default(),
            streamlines: false,
            particles: false,
            speed: 1.0,
        },
    }
}

fn legacy_settings(normalize: bool) -> LegacySettings {
    LegacySettings {
        style: AppStyle::default(),
        layout: LayoutState::default(),
        view: LegacyViewRecord {
            formula: "(x, y)".to_string(),
            mode_3d: false,
            domain_min: [-1.0, -1.0, -1.0],
            domain_max: [1.0, 1.0, 1.0],
            density: 24,
            arrow_scale: 2.0,
            normalize,
            colormap: Colormap::Turbo,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            streamlines: false,
            particles: false,
            speed: 1.0,
        },
    }
}

fn load_legacy(normalize: bool) -> AppSettings {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &legacy_settings(normalize));
    load(Some(&storage))
}

fn configured() -> (State, Controls) {
    let mut state = State::new(false);
    let mut controls = Controls::new();
    state.set_mode(true);
    state.set_formula("(x, y, z)".to_string());
    controls.density = 12;
    controls.arrow_scale = 2.5;
    controls.arrow_length_mode = ArrowLengthMode::Magnitude;
    controls.magnitude_scale = 2.5;
    controls.colormap = Colormap::Mono;
    controls.auto_fit = false;
    controls.color_min = -2.0;
    controls.color_max = 3.0;
    controls.streamlines = true;
    controls.particles = true;
    controls.speed = 2.2;
    controls.arrows = false;
    controls.arrow_style = ArrowStyle::Cylinder;
    controls.arrow_style_2d = ArrowStyle2D::Line;
    controls.arrow_params = ArrowParams {
        head_length: 0.4,
        head_width: 0.2,
        shaft_radius: 0.08,
        sides: 8,
    };
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
fn a_legacy_normalize_true_migrates_to_uniform_without_fail_fast() {
    let settings = load_legacy(true);
    let mut state = State::new(false);
    let mut controls = Controls::new();
    settings.view.restore(&mut state, &mut controls);
    assert_eq!(
        state.arrow_length_mode,
        ArrowLengthMode::Uniform,
        "old normalize = true means Uniform",
    );
    assert_eq!(state.magnitude_scale, 1.0, "the missing magnitude scale defaults to 1");
    assert_eq!(state.arrow_scale, 2.0, "the rest of the legacy record still loads");
    assert_eq!(controls.arrow_length_mode, ArrowLengthMode::Uniform, "the mirror follows");
    assert_eq!(controls.magnitude_scale, 1.0);
}

#[test]
fn a_legacy_normalize_false_migrates_to_proportional() {
    let settings = load_legacy(false);
    let mut state = State::new(false);
    let mut controls = Controls::new();
    settings.view.restore(&mut state, &mut controls);
    assert_eq!(
        state.arrow_length_mode,
        ArrowLengthMode::Proportional,
        "old normalize = false means Proportional",
    );
    assert_eq!(state.magnitude_scale, 1.0);
    assert_eq!(controls.arrow_length_mode, ArrowLengthMode::Proportional);
}

#[test]
fn a_legacy_record_without_arrow_style_fields_gets_the_defaults() {
    let settings = load_legacy(false);
    let mut state = State::new(false);
    let mut controls = Controls::new();
    settings.view.restore(&mut state, &mut controls);
    assert!(state.arrows, "older records show arrows");
    assert_eq!(state.arrow_style, ArrowStyle::Cone, "older records use the cone");
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Line, "older records use the 2D line");
    assert_eq!(state.arrow_params, ArrowParams::default(), "older records use default geometry");
    assert_eq!(controls.arrow_style, ArrowStyle::Cone, "the mirror follows");
    assert_eq!(controls.arrow_style_2d, ArrowStyle2D::Line, "the 2D mirror follows");
    assert_eq!(controls.arrow_params, ArrowParams::default());
}

#[test]
fn a_pre_ticket_61_record_keeps_its_3d_style_and_defaults_the_2d_style() {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &pre_2d_style_settings());

    let loaded = load(Some(&storage));

    assert_eq!(loaded.view.arrow_style, ArrowStyle::Pyramid, "the existing 3D style survives");
    assert_eq!(
        loaded.view.arrow_style_2d,
        ArrowStyle2D::Line,
        "the missing 2D field loads from its serde default",
    );

    let mut state = State::new(true);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);
    assert_eq!(state.arrow_style, ArrowStyle::Pyramid, "restore keeps the 3D selection");
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Line, "restore defaults the 2D selection");
    assert_eq!(controls.arrow_style, ArrowStyle::Pyramid, "the 3D mirror follows");
    assert_eq!(controls.arrow_style_2d, ArrowStyle2D::Line, "the 2D mirror follows");
}

#[test]
fn new_records_write_and_reload_the_2d_arrow_style() {
    let settings = sample_settings();
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(
        raw.contains("arrow_style:Cylinder"),
        "the 3D style is written with its stable spelling:\n{raw}",
    );
    assert!(
        raw.contains("arrow_style_2d:Line"),
        "the 2D style is written with its stable spelling:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.arrow_style_2d, ArrowStyle2D::Line);
    assert_eq!(loaded.view.arrow_style, ArrowStyle::Cylinder);
}

#[test]
fn new_records_round_trip_mode_and_scales_and_stop_writing_the_legacy_bool() {
    let mut state = State::new(false);
    assert!(state.set_arrow_length_mode(ArrowLengthMode::Magnitude));
    assert!(state.set_arrow_scale(1.5));
    assert!(state.set_magnitude_scale(0.25));
    let record = ViewRecord::capture(&state);
    assert_eq!(record.arrow_length_mode, ArrowLengthMode::Magnitude);
    assert_eq!(record.magnitude_scale, 0.25);
    assert_eq!(record.normalize, None, "new records carry no legacy bool");

    let settings = AppSettings { view: record, ..AppSettings::default() };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(!raw.contains("normalize"), "the legacy key is no longer written:\n{raw}");

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.arrow_length_mode, ArrowLengthMode::Magnitude);
    assert_eq!(loaded.view.magnitude_scale, 0.25);
    assert_eq!(loaded.view.arrow_scale, 1.5);
    assert_eq!(loaded.view.normalize, None);
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
    assert_eq!(restored_state.arrow_length_mode, state.arrow_length_mode, "arrow length mode");
    assert_eq!(restored_state.magnitude_scale, state.magnitude_scale, "magnitude scale");
    assert_eq!(restored_state.colormap, state.colormap, "colormap");
    assert_eq!(restored_state.auto_fit, state.auto_fit, "auto-fit");
    assert_eq!(restored_state.color_min, state.color_min, "color min");
    assert_eq!(restored_state.color_max, state.color_max, "color max");
    assert_eq!(restored_state.streamlines, state.streamlines, "streamlines");
    assert_eq!(restored_state.particles, state.particles, "particles");
    assert_eq!(restored_state.speed, state.speed, "speed");
    assert!(!restored_state.arrows, "the Show arrows toggle");
    assert_eq!(restored_state.arrow_style, ArrowStyle::Cylinder, "arrow style");
    assert_eq!(restored_state.arrow_style_2d, ArrowStyle2D::Line, "2D arrow style");
    assert_eq!(restored_state.arrow_params.sides, 8, "arrow sides");
    assert_eq!(restored_state.arrow_params.head_length, 0.4, "arrow head length");
    assert_eq!(restored_controls.arrows, restored_state.arrows, "the mirror follows the toggle");
    assert_eq!(
        restored_controls.arrow_style,
        restored_state.arrow_style,
        "the mirror follows the style",
    );
    assert_eq!(
        restored_controls.arrow_style_2d,
        restored_state.arrow_style_2d,
        "the mirror follows the 2D style",
    );
    assert_eq!(
        restored_controls.arrow_params,
        restored_state.arrow_params,
        "the mirror follows the params",
    );
    // The widget mirror is refreshed so the first sidebar frame agrees.
    assert_eq!(restored_controls.density, state.density);
    assert_eq!(restored_controls.arrow_length_mode, state.arrow_length_mode);
    assert_eq!(restored_controls.magnitude_scale, state.magnitude_scale);
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
        magnitude_scale: f32::NAN,
        arrow_params: ArrowParams {
            head_length: f32::NAN,
            head_width: f32::INFINITY,
            shaft_radius: -4.0,
            sides: 999,
        },
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
    assert!(state.magnitude_scale.is_finite(), "a NaN magnitude scale falls back");
    assert!(
        (MIN_MAGNITUDE_SCALE..=MAX_MAGNITUDE_SCALE).contains(&state.magnitude_scale),
        "the magnitude scale stays inside its documented bounds",
    );
    assert!(state.speed.is_finite(), "an infinite speed falls back");
    assert!(state.color_min.is_finite() && state.color_max.is_finite());
    assert!(state.domain.min.is_finite() && state.domain.max.is_finite());
    assert!(state.domain.min.x < state.domain.max.x, "the domain stays a real box");
    assert_eq!(state.arrow_params.head_length, 0.3, "a NaN head length falls back");
    assert_eq!(state.arrow_params.head_width, 0.12, "an infinite head width falls back");
    assert_eq!(state.arrow_params.shaft_radius, 0.01, "a negative radius clamps to the min");
    assert_eq!(state.arrow_params.sides, 16, "an absurd side count clamps to the max");
}

#[test]
fn restore_clamps_a_huge_magnitude_scale() {
    let record = ViewRecord { magnitude_scale: 1.0e9, ..Default::default() };
    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    assert_eq!(state.magnitude_scale, MAX_MAGNITUDE_SCALE, "the seam bounds the stored value");
    assert_eq!(controls.magnitude_scale, MAX_MAGNITUDE_SCALE, "the mirror follows the clamp");
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

/// Store `settings` under the real `APP_KEY` and build the app from it, so the
/// launch camera is exercised through the same storage path a real launch uses.
fn app_from(settings: &AppSettings) -> VfApp {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, settings);
    VfApp::new_with_storage(Some(&storage))
}

/// Ticket 47: a restored off-origin 3D box is framed on the first frame, not
/// only after its first edit, and the rest of the default pose is untouched.
#[test]
fn a_restored_off_origin_3d_domain_is_framed_on_launch() {
    let mut state = State::new(true);
    assert!(
        state.set_domain(Vec3::new(5.0, -2.0, 0.0), Vec3::new(7.0, 2.0, 2.0)),
        "seed an off-origin 3D box",
    );
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };

    let app = app_from(&settings);

    assert!(app.state.mode_3d, "the saved projection is restored");
    assert_eq!(app.orbit.target, app.state.domain.center(), "launch frames the restored box");
    let default = OrbitCamera::default();
    assert_eq!(app.orbit.yaw, default.yaw, "launch keeps the default yaw");
    assert_eq!(app.orbit.pitch, default.pitch, "launch keeps the default pitch");
    assert_eq!(app.orbit.distance, default.distance, "launch keeps the default distance");
}

/// Ticket 47: in 2D only the screen-plane axes follow the restored box; the
/// depth target stays at the default, exactly like a live 2D domain edit.
#[test]
fn a_restored_off_origin_2d_domain_is_framed_in_x_and_y_only_on_launch() {
    let mut state = State::new(false);
    assert!(
        state.set_domain(Vec3::new(5.0, -2.0, 0.0), Vec3::new(7.0, 2.0, 2.0)),
        "seed an off-origin 2D box",
    );
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };

    let app = app_from(&settings);

    assert!(!app.state.mode_3d, "the saved 2D projection is restored");
    let centre = app.state.domain.center();
    assert_eq!(app.orbit.target.x, centre.x, "2D launch follows the box centre in x");
    assert_eq!(app.orbit.target.y, centre.y, "2D launch follows the box centre in y");
    assert_eq!(app.orbit.target.z, 0.0, "2D launch leaves the depth target at the default");
}

/// Ticket 47: no saved domain (or an empty storage) keeps the pre-ticket launch
/// camera — the default box centres on the origin.
#[test]
fn a_launch_without_a_saved_domain_still_targets_the_origin() {
    assert_eq!(VfApp::new().orbit.target, Vec3::ZERO, "no storage keeps the origin target");

    let empty = MemoryStorage::default();
    assert_eq!(
        VfApp::new_with_storage(Some(&empty)).orbit.target,
        Vec3::ZERO,
        "empty storage keeps the origin target",
    );
}
