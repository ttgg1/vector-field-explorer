// Moved out of `src/persistence.rs`'s inline module (ticket-14 review I-4).
// Covers the single APP_KEY envelope, ticket-07 migration, defaults, hostile
// values, view-state restoration and the style clamp (review I-3).

use eframe::Storage as _;
use glam::Vec3;

use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::persistence::{AppSettings, ViewRecord, load, save};
use vector_field_explorer::particles::{MAX_PARTICLES, ParticleAppearance, ParticleColorMode};
use vector_field_explorer::plane::DIM_ALPHA;
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::ui::style_panel::{
    AppStyle, MAX_FONT_SIZE, MAX_ROUNDING, MAX_SPACING, MIN_ROUNDING,
};
use vector_field_explorer::viewmodel::{
    ArrowLengthMode, DEFAULT_PARTICLE_SIZE, DEFAULT_PROPORTIONAL_EXPONENT, LeftClickAction,
    MAX_ARROW_OPACITY, MAX_DOMAIN_EXTENT, MAX_MAGNITUDE_SCALE, MAX_PROPORTIONAL_EXPONENT,
    MIN_DOMAIN_EXTENT, MIN_MAGNITUDE_SCALE, STREAMLINE_MAX_LEN, State, StreamlineSeedMode,
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

/// The old serde spellings a pre-ticket-64 record can carry. `Line` meant the
/// flat ribbon and `Pyramid` meant the cone; both must still decode.
#[derive(serde::Serialize)]
enum OldArrowStyle {
    Pyramid,
    Line,
}

/// The exact `ViewRecord` shape a build before ticket 61 wrote: it carries the
/// 3D `arrow_style` and geometry but no `arrow_style_2d`. Writing this through
/// real eframe storage proves the new field loads from its serde default while
/// the old `Pyramid` spelling decodes as Cone.
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
    arrow_style: OldArrowStyle,
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
            arrow_style: OldArrowStyle::Pyramid,
            arrow_params: ArrowParams::default(),
            streamlines: false,
            particles: false,
            speed: 1.0,
        },
    }
}

/// A pre-ticket-64 record whose two style keys use the old `Line` spelling.
/// `ViewRecord`'s container default fills every other missing field.
#[derive(serde::Serialize)]
struct OldLineViewRecord {
    arrow_style: OldArrowStyle,
    arrow_style_2d: OldArrowStyle,
}

#[derive(serde::Serialize)]
struct OldLineSettings {
    style: AppStyle,
    view: OldLineViewRecord,
}

fn old_line_settings() -> OldLineSettings {
    OldLineSettings {
        style: AppStyle::default(),
        view: OldLineViewRecord {
            arrow_style: OldArrowStyle::Line,
            arrow_style_2d: OldArrowStyle::Line,
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

#[test]
fn a_legacy_speed_envelope_loads_and_streamlines_keep_the_fixed_budget() {
    #[derive(serde::Serialize)]
    struct SpeedOnlyView {
        speed: f32,
    }

    #[derive(serde::Serialize)]
    struct SpeedOnlySettings {
        style: AppStyle,
        view: SpeedOnlyView,
    }

    let mut storage = MemoryStorage::default();
    eframe::set_value(
        &mut storage,
        eframe::APP_KEY,
        &SpeedOnlySettings {
            style: AppStyle::default(),
            view: SpeedOnlyView { speed: 2.7 },
        },
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.speed, 2.7, "the obsolete key is still readable from an old envelope");

    let mut state = State::new(false);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);
    assert_eq!(STREAMLINE_MAX_LEN, 120, "streamlines keep a fixed point budget, not the old speed");
    assert!(state.set_formula("(-y, x)".to_string()));
    assert!(state.set_streamlines(true));
    assert!(
        !state.snapshot().lines.is_empty(),
        "streamlines still trace at the fixed budget after an old speed loads",
    );

    let mut rewritten = MemoryStorage::default();
    save(&mut rewritten, &loaded);
    let raw = rewritten.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(!raw.contains("speed"), "new records never write the obsolete key:\n{raw}");
}

fn configured() -> (State, Controls) {
    let mut state = State::new(false);
    let mut controls = Controls::new();
    state.set_mode(true);
    state.set_formula("(x, y, z)".to_string());
    controls.density = 12;
    controls.arrow_scale = 2.5;
    controls.arrow_opacity = 0.4;
    controls.plane_dim_opacity = 0.65;
    controls.arrow_length_mode = ArrowLengthMode::Magnitude;
    controls.magnitude_scale = 2.5;
    controls.proportional_exponent = 2.5;
    controls.colormap = Colormap::Mono;
    controls.auto_fit = false;
    controls.color_min = -2.0;
    controls.color_max = 3.0;
    controls.streamlines = true;
    controls.seed_mode = StreamlineSeedMode::Click;
    controls.left_click_action = LeftClickAction::AddParticles;
    controls.particles = true;
    controls.arrows = false;
    controls.show_grid = false;
    controls.show_coordinate_system = false;
    controls.arrow_style = ArrowStyle::Cylinder;
    controls.arrow_style_2d = ArrowStyle2D::Streamline;
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
    assert!(!settings.view.show_grid, "capture keeps the disabled grid");
    assert!(
        !settings.view.show_coordinate_system,
        "capture keeps the disabled axes"
    );
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("show_grid:false"),
        "the grid preference is written:\n{raw}"
    );
    assert!(
        raw.contains("show_coordinate_system:false"),
        "the axes preference is written:\n{raw}"
    );
    assert_eq!(load(Some(&storage)), settings);
    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        (app.state.show_grid, app.state.show_coordinate_system),
        (false, false)
    );
    assert_eq!(
        (app.sidebar.show_grid, app.sidebar.show_coordinate_system),
        (false, false)
    );
}

#[test]
fn each_reference_preference_survives_storage_restore_and_constructor_independently() {
    for (grid, coordinate_system) in [(false, true), (true, false)] {
        let mut state = State::new(false);
        assert_eq!(state.set_show_grid(grid), !grid);
        assert_eq!(
            state.set_show_coordinate_system(coordinate_system),
            !coordinate_system
        );
        let settings = AppSettings {
            view: ViewRecord::capture(&state),
            ..AppSettings::default()
        };
        let mut storage = MemoryStorage::default();
        save(&mut storage, &settings);
        let loaded = load(Some(&storage));
        assert_eq!(
            loaded.view.show_grid, grid,
            "grid record must not swap flags"
        );
        assert_eq!(
            loaded.view.show_coordinate_system, coordinate_system,
            "axes record must not swap flags"
        );

        let mut restored = State::new(false);
        let mut controls = Controls::new();
        loaded.view.restore(&mut restored, &mut controls);
        assert_eq!(
            (restored.show_grid, restored.show_coordinate_system),
            (grid, coordinate_system)
        );
        assert_eq!(
            (controls.show_grid, controls.show_coordinate_system),
            (grid, coordinate_system)
        );

        let app = VfApp::new_with_storage(Some(&storage));
        assert_eq!(
            (app.state.show_grid, app.state.show_coordinate_system),
            (grid, coordinate_system)
        );
        assert_eq!(
            (app.sidebar.show_grid, app.sidebar.show_coordinate_system),
            (grid, coordinate_system)
        );
    }
}

#[test]
fn a_partial_reference_record_defaults_only_the_missing_coordinate_flag() {
    #[derive(serde::Serialize)]
    struct PartialView {
        show_grid: bool,
    }
    #[derive(serde::Serialize)]
    struct PartialSettings {
        style: AppStyle,
        view: PartialView,
    }

    let mut storage = MemoryStorage::default();
    eframe::set_value(
        &mut storage,
        eframe::APP_KEY,
        &PartialSettings {
            style: AppStyle::default(),
            view: PartialView { show_grid: false },
        },
    );
    let loaded = load(Some(&storage));
    assert!(!loaded.view.show_grid);
    assert!(
        loaded.view.show_coordinate_system,
        "a missing bool must default to true, not false"
    );
    let mut state = State::new(false);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);
    assert_eq!(
        (state.show_grid, state.show_coordinate_system),
        (false, true)
    );
    assert_eq!(
        (controls.show_grid, controls.show_coordinate_system),
        (false, true)
    );
}

/// Ticket 53: compact mode and the bottom-sheet state are transient. The one
/// saved envelope must keep carrying only the ticket-14 layout knobs.
#[test]
fn compact_mode_and_sheet_are_never_written_to_storage() {
    let mut storage = MemoryStorage::default();
    save(&mut storage, &sample_settings());
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    for key in ["compact", "sheet", "layout_mode", "sheet_state"] {
        assert!(
            !raw.to_ascii_lowercase().contains(key),
            "the persisted envelope must not carry `{key}`:\n{raw}"
        );
    }
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
    assert!(settings.view.show_grid, "legacy records omit the grid flag");
    assert!(
        settings.view.show_coordinate_system,
        "legacy records omit the axes flag"
    );
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
    assert_eq!(
        (state.show_grid, state.show_coordinate_system),
        (true, true)
    );
    assert_eq!(
        (controls.show_grid, controls.show_coordinate_system),
        (true, true)
    );
}

#[test]
fn a_legacy_record_without_arrow_style_fields_gets_the_defaults() {
    let settings = load_legacy(false);
    let mut state = State::new(false);
    let mut controls = Controls::new();
    settings.view.restore(&mut state, &mut controls);
    assert!(state.arrows, "older records show arrows");
    assert_eq!(state.arrow_style, ArrowStyle::Cone, "older records use the cone");
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Flat, "older records use the 2D flat ribbon");
    assert_eq!(state.arrow_params, ArrowParams::default(), "older records use default geometry");
    assert_eq!(controls.arrow_style, ArrowStyle::Cone, "the mirror follows");
    assert_eq!(controls.arrow_style_2d, ArrowStyle2D::Flat, "the 2D mirror follows");
    assert_eq!(controls.arrow_params, ArrowParams::default());
}

#[test]
fn a_pre_ticket_61_record_decodes_old_pyramid_and_defaults_the_2d_style() {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &pre_2d_style_settings());

    let loaded = load(Some(&storage));

    assert_eq!(
        loaded.view.arrow_style,
        ArrowStyle::Cone,
        "the old Pyramid spelling decodes as Cone",
    );
    assert_eq!(
        loaded.view.arrow_style_2d,
        ArrowStyle2D::Flat,
        "the missing 2D field loads from its serde default",
    );

    let mut state = State::new(true);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);
    assert_eq!(state.arrow_style, ArrowStyle::Cone, "restore keeps the decoded 3D selection");
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Flat, "restore defaults the 2D selection");
    assert_eq!(controls.arrow_style, ArrowStyle::Cone, "the 3D mirror follows");
    assert_eq!(controls.arrow_style_2d, ArrowStyle2D::Flat, "the 2D mirror follows");
}

#[test]
fn a_pre_ticket_64_record_decodes_old_line_to_flat_in_both_sets() {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &old_line_settings());

    let loaded = load(Some(&storage));

    assert_eq!(
        loaded.view.arrow_style,
        ArrowStyle::Flat,
        "the old 3D Line spelling keeps the flat ribbon",
    );
    assert_eq!(
        loaded.view.arrow_style_2d,
        ArrowStyle2D::Flat,
        "the old 2D Line spelling keeps the flat ribbon",
    );

    let mut state = State::new(true);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);
    assert_eq!(state.arrow_style, ArrowStyle::Flat);
    assert_eq!(state.arrow_style_2d, ArrowStyle2D::Flat);
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
        raw.contains("arrow_style_2d:Streamline"),
        "the 2D Line style is written with its stable `Streamline` spelling:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.arrow_style_2d, ArrowStyle2D::Streamline);
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
fn proportional_exponent_round_trips_and_defaults_when_absent() {
    let mut state = State::new(false);
    assert!(state.set_proportional_exponent(2.5));
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(
        raw.contains("proportional_exponent:2.5"),
        "the contrast is written:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.proportional_exponent, 2.5);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded.view.restore(&mut restored_state, &mut restored_controls);
    assert_eq!(restored_state.proportional_exponent, 2.5);
    assert_eq!(restored_controls.proportional_exponent, 2.5, "the mirror follows");

    // The pre-ticket-57 shape has no `proportional_exponent` key at all.
    let legacy = load_legacy(false);
    assert_eq!(
        legacy.view.proportional_exponent, DEFAULT_PROPORTIONAL_EXPONENT,
        "an older record defaults to linear",
    );
    let mut legacy_state = State::new(false);
    let mut legacy_controls = Controls::new();
    legacy.view.restore(&mut legacy_state, &mut legacy_controls);
    assert_eq!(legacy_state.proportional_exponent, 1.0);
    assert_eq!(legacy_controls.proportional_exponent, 1.0);
}

#[test]
fn arrow_opacities_round_trip_and_default_when_absent() {
    let mut state = State::new(false);
    assert!(state.set_arrow_opacity(0.35));
    assert!(state.set_plane_dim_opacity(0.65));
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(raw.contains("arrow_opacity:0.35"), "the global opacity is written:\n{raw}");
    assert!(raw.contains("plane_dim_opacity:0.65"), "the off-plane opacity is written:\n{raw}");

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.arrow_opacity, 0.35);
    assert_eq!(loaded.view.plane_dim_opacity, 0.65);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded.view.restore(&mut restored_state, &mut restored_controls);
    assert_eq!(restored_state.arrow_opacity, 0.35);
    assert_eq!(restored_state.plane_dim_opacity, 0.65);
    assert_eq!(restored_controls.arrow_opacity, 0.35, "the mirror follows");
    assert_eq!(restored_controls.plane_dim_opacity, 0.65);

    // The pre-ticket-36 shape has neither key at all.
    let legacy = load_legacy(false);
    assert_eq!(legacy.view.arrow_opacity, MAX_ARROW_OPACITY, "an older record loads fully opaque");
    assert_eq!(
        legacy.view.plane_dim_opacity, DIM_ALPHA,
        "…and its plane dim defaults to 0.2",
    );
    let mut legacy_state = State::new(false);
    let mut legacy_controls = Controls::new();
    legacy.view.restore(&mut legacy_state, &mut legacy_controls);
    assert_eq!(legacy_state.arrow_opacity, 1.0);
    assert_eq!(legacy_state.plane_dim_opacity, DIM_ALPHA);
    assert_eq!(legacy_controls.arrow_opacity, 1.0);
    assert_eq!(legacy_controls.plane_dim_opacity, DIM_ALPHA);
}

#[test]
fn corrupt_arrow_opacities_are_sanitized_on_restore() {
    let mut record = ViewRecord {
        arrow_opacity: f32::NAN,
        plane_dim_opacity: f32::INFINITY,
        ..ViewRecord::default()
    };
    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    assert_eq!(state.arrow_opacity, 1.0, "NaN falls back to full opacity");
    assert_eq!(state.plane_dim_opacity, DIM_ALPHA, "Infinity falls back to the dim alpha");
    assert_eq!(controls.arrow_opacity, 1.0, "the mirror follows the fallback");
    assert_eq!(controls.plane_dim_opacity, DIM_ALPHA);

    record.arrow_opacity = 5.0;
    record.plane_dim_opacity = -2.0;
    record.restore(&mut state, &mut controls);
    assert_eq!(state.arrow_opacity, MAX_ARROW_OPACITY, "above one clamps");
    assert_eq!(state.plane_dim_opacity, 0.0, "below zero clamps");
    assert_eq!(controls.arrow_opacity, MAX_ARROW_OPACITY);
    assert_eq!(controls.plane_dim_opacity, 0.0);
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
    let loaded = load(Some(&storage));
    assert!(loaded.view.show_grid);
    assert!(loaded.view.show_coordinate_system);
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
    assert_eq!(
        (record.show_grid, record.show_coordinate_system),
        (false, false)
    );
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    record.restore(&mut restored_state, &mut restored_controls);

    assert_eq!(restored_state.formula, state.formula, "formula");
    assert_eq!(restored_state.mode_3d, state.mode_3d, "2D/3D mode");
    assert_eq!(restored_state.domain, state.domain, "domain");
    assert_eq!(restored_state.density, state.density, "density");
    assert_eq!(restored_state.arrow_scale, state.arrow_scale, "arrow scale");
    assert_eq!(restored_state.arrow_opacity, state.arrow_opacity, "arrow opacity");
    assert_eq!(
        restored_state.plane_dim_opacity, state.plane_dim_opacity,
        "off-plane opacity",
    );
    assert_eq!(restored_controls.arrow_opacity, state.arrow_opacity, "the mirror follows opacity");
    assert_eq!(
        restored_controls.plane_dim_opacity, state.plane_dim_opacity,
        "the mirror follows the off-plane opacity",
    );
    assert_eq!(restored_state.arrow_length_mode, state.arrow_length_mode, "arrow length mode");
    assert_eq!(restored_state.magnitude_scale, state.magnitude_scale, "magnitude scale");
    assert_eq!(
        restored_state.proportional_exponent, state.proportional_exponent,
        "proportional contrast",
    );
    assert_eq!(restored_state.colormap, state.colormap, "colormap");
    assert_eq!(restored_state.auto_fit, state.auto_fit, "auto-fit");
    assert_eq!(restored_state.color_min, state.color_min, "color min");
    assert_eq!(restored_state.color_max, state.color_max, "color max");
    assert_eq!(restored_state.streamlines, state.streamlines, "streamlines");
    assert_eq!(restored_state.seed_mode, state.seed_mode, "streamline seed mode");
    assert_eq!(
        restored_state.left_click_action, state.left_click_action,
        "left-click action",
    );
    assert_eq!(restored_state.particles, state.particles, "particles");
    assert_eq!(
        (
            restored_state.show_grid,
            restored_state.show_coordinate_system
        ),
        (false, false)
    );
    assert_eq!(
        (
            restored_controls.show_grid,
            restored_controls.show_coordinate_system
        ),
        (false, false)
    );
    assert!(!restored_state.arrows, "the Show arrows toggle");
    assert_eq!(restored_state.arrow_style, ArrowStyle::Cylinder, "arrow style");
    assert_eq!(restored_state.arrow_style_2d, ArrowStyle2D::Streamline, "2D arrow style");
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
    assert_eq!(restored_controls.seed_mode, state.seed_mode, "the mirror follows the seed mode");
    assert_eq!(
        restored_controls.left_click_action, state.left_click_action,
        "the mirror follows the left-click action",
    );
    // The widget mirror is refreshed so the first sidebar frame agrees.
    assert_eq!(restored_controls.density, state.density);
    assert_eq!(restored_controls.arrow_length_mode, state.arrow_length_mode);
    assert_eq!(restored_controls.magnitude_scale, state.magnitude_scale);
    assert_eq!(
        restored_controls.proportional_exponent, state.proportional_exponent,
        "the mirror follows the proportional contrast",
    );
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
    assert!(ViewRecord::default().show_grid);
    assert!(ViewRecord::default().show_coordinate_system);
    assert_eq!(
        ViewRecord::default().seed_mode,
        StreamlineSeedMode::Auto,
        "the default view record resets the seed mode to Auto",
    );
    assert_eq!(
        ViewRecord::default().left_click_action,
        LeftClickAction::SeedStreamline,
        "the default view record seeds streamlines on left click",
    );
}

#[test]
fn a_record_without_a_seed_mode_loads_as_auto() {
    // `load_legacy` writes the pre-ticket-50 shape: no `seed_mode` key at all.
    let loaded = load_legacy(false);
    assert_eq!(
        loaded.view.seed_mode,
        StreamlineSeedMode::Auto,
        "an older record defaults the seed mode instead of failing",
    );

    let mut state = State::new(false);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);
    assert_eq!(state.seed_mode, StreamlineSeedMode::Auto);
    assert_eq!(controls.seed_mode, StreamlineSeedMode::Auto, "the mirror follows");
}

#[test]
fn seed_mode_round_trips_through_storage() {
    let mut state = State::new(false);
    assert!(state.set_seed_mode(StreamlineSeedMode::Click));
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(
        raw.contains("seed_mode:Click"),
        "the seed mode is written with its stable spelling:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.seed_mode, StreamlineSeedMode::Click);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded.view.restore(&mut restored_state, &mut restored_controls);
    assert_eq!(restored_state.seed_mode, StreamlineSeedMode::Click);
    assert_eq!(restored_controls.seed_mode, StreamlineSeedMode::Click);
}

#[test]
fn a_record_without_a_left_click_action_loads_as_seed_streamline() {
    // `load_legacy` writes a pre-ticket-60 shape: no `left_click_action` key.
    let loaded = load_legacy(false);
    assert_eq!(
        loaded.view.left_click_action,
        LeftClickAction::SeedStreamline,
        "an older record defaults the left-click action instead of failing",
    );

    let mut state = State::new(false);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);
    assert_eq!(state.left_click_action, LeftClickAction::SeedStreamline);
    assert_eq!(controls.left_click_action, LeftClickAction::SeedStreamline, "the mirror follows");
}

#[test]
fn an_unknown_left_click_action_loads_as_seed_streamline() {
    /// A bare unknown variant, as a hand-edited/future RON record would carry
    /// it. (A quoted string is a different serde shape and is out of scope.)
    #[derive(serde::Serialize)]
    enum UnknownLeftClickAction {
        Teleport,
    }

    #[derive(serde::Serialize)]
    struct UnknownActionView {
        left_click_action: UnknownLeftClickAction,
        seed_mode: StreamlineSeedMode,
        particle_count: u32,
    }

    #[derive(serde::Serialize)]
    struct UnknownActionSettings {
        style: AppStyle,
        view: UnknownActionView,
    }

    // Non-default neighbours prove the unknown action did not trigger the
    // whole-envelope fallback: an envelope-loss load would return defaults for
    // these too.
    let style = AppStyle {
        theme_dark: false,
        accent: eframe::egui::Color32::from_rgb(9, 8, 7),
        rounding: 4.0,
        spacing: 9.0,
        font_size: 14.0,
    };
    let mut storage = MemoryStorage::default();
    eframe::set_value(
        &mut storage,
        eframe::APP_KEY,
        &UnknownActionSettings {
            style: style.clone(),
            view: UnknownActionView {
                left_click_action: UnknownLeftClickAction::Teleport,
                seed_mode: StreamlineSeedMode::Click,
                particle_count: 321,
            },
        },
    );

    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.view.left_click_action,
        LeftClickAction::SeedStreamline,
        "an unknown spelling falls back instead of failing the envelope",
    );
    assert_eq!(
        loaded.style, style,
        "the non-default style still loads instead of the whole-envelope default",
    );
    assert_eq!(
        loaded.view.seed_mode,
        StreamlineSeedMode::Click,
        "a non-default sibling field still loads",
    );
    assert_eq!(
        loaded.view.particle_count, 321,
        "another non-default sibling still loads",
    );
}

#[test]
fn left_click_action_round_trips_through_storage() {
    let mut state = State::new(false);
    assert!(state.set_left_click_action(LeftClickAction::AddParticles));
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(
        raw.contains("left_click_action:AddParticles"),
        "the action is written with its stable spelling:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.left_click_action, LeftClickAction::AddParticles);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded.view.restore(&mut restored_state, &mut restored_controls);
    assert_eq!(restored_state.left_click_action, LeftClickAction::AddParticles);
    assert_eq!(restored_controls.left_click_action, LeftClickAction::AddParticles);
}

#[test]
fn restore_clamps_and_sanitizes_a_hostile_record() {
    let record = ViewRecord {
        density: 99_999,
        arrow_scale: f32::NAN,
        magnitude_scale: f32::NAN,
        proportional_exponent: f32::NAN,
        particle_size: f32::NAN,
        particle_count: 99_999,
        arrow_params: ArrowParams {
            head_length: f32::NAN,
            head_width: f32::INFINITY,
            shaft_radius: -4.0,
            sides: 999,
        },
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
    assert_eq!(
        state.proportional_exponent, DEFAULT_PROPORTIONAL_EXPONENT,
        "a NaN contrast resets to linear",
    );
    assert!(
        (MIN_MAGNITUDE_SCALE..=MAX_MAGNITUDE_SCALE).contains(&state.magnitude_scale),
        "the magnitude scale stays inside its documented bounds",
    );
    assert!(state.color_min.is_finite() && state.color_max.is_finite());
    assert!(state.domain.min.is_finite() && state.domain.max.is_finite());
    assert!(state.domain.min.x < state.domain.max.x, "the domain stays a real box");
    assert_eq!(state.arrow_params.head_length, 0.3, "a NaN head length falls back");
    assert_eq!(state.arrow_params.head_width, 0.12, "an infinite head width falls back");
    assert_eq!(state.arrow_params.shaft_radius, 0.01, "a negative radius clamps to the min");
    assert_eq!(state.arrow_params.sides, 16, "an absurd side count clamps to the max");
    assert_eq!(
        state.particle_size, DEFAULT_PARTICLE_SIZE,
        "a NaN particle size resets to the default",
    );
    assert_eq!(
        state.particle_count, MAX_PARTICLES as u32,
        "an absurd particle count clamps to the cap",
    );
    assert_eq!(controls.particle_size, state.particle_size, "the mirror follows the size");
    assert_eq!(controls.particle_count, state.particle_count, "the mirror follows the count");
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
fn restore_clamps_a_huge_proportional_exponent() {
    let record = ViewRecord { proportional_exponent: 1.0e6, ..Default::default() };
    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    assert_eq!(
        state.proportional_exponent,
        MAX_PROPORTIONAL_EXPONENT,
        "the seam bounds the stored contrast",
    );
    assert_eq!(
        controls.proportional_exponent,
        MAX_PROPORTIONAL_EXPONENT,
        "the mirror follows the clamp",
    );
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

#[test]
fn particle_size_and_count_round_trip_and_default_when_absent() {
    let mut state = State::new(false);
    assert!(state.set_particle_size(2.25));
    assert!(state.set_particle_count(500));
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(raw.contains("particle_size:2.25"), "the size is written:\n{raw}");
    assert!(raw.contains("particle_count:500"), "the count is written:\n{raw}");

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.particle_size, 2.25);
    assert_eq!(loaded.view.particle_count, 500);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded.view.restore(&mut restored_state, &mut restored_controls);
    assert_eq!(restored_state.particle_size, 2.25, "the size survives the round trip");
    assert_eq!(restored_state.particle_count, 500, "the count survives the round trip");
    assert_eq!(restored_controls.particle_size, 2.25, "the mirror follows the size");
    assert_eq!(restored_controls.particle_count, 500, "the mirror follows the count");

    // A record written before ticket 30 has neither key: defaults apply.
    let legacy = load_legacy(false);
    assert_eq!(
        legacy.view.particle_size, DEFAULT_PARTICLE_SIZE,
        "an older record loads size 1.0",
    );
    assert_eq!(legacy.view.particle_count, 0, "an older record loads Auto");
    let mut legacy_state = State::new(false);
    let mut legacy_controls = Controls::new();
    legacy.view.restore(&mut legacy_state, &mut legacy_controls);
    assert_eq!(legacy_state.particle_size, 1.0);
    assert_eq!(legacy_state.particle_count, 0);
    assert_eq!(legacy_controls.particle_size, 1.0);
    assert_eq!(legacy_controls.particle_count, 0);
}

// --- Ticket 48: particle appearance persistence -------------------------

fn configured_appearance() -> ParticleAppearance {
    ParticleAppearance {
        opacity: 0.35,
        color_mode: ParticleColorMode::Fixed,
        fixed_color: [12, 34, 56, 255],
        trail_len: 17,
        trail_fade: 0.25,
        trail_width: 1.75,
        loop_on_stagnation: false,
        stagnation_seconds: 3.5,
    }
}

#[test]
fn particle_appearance_round_trips_and_defaults_when_absent() {
    let mut state = State::new(false);
    let appearance = configured_appearance();
    assert!(state.set_particle_appearance(appearance));
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(raw.contains("particle_opacity:0.35"), "opacity is written:\n{raw}");
    assert!(
        raw.contains("particle_color_mode:fixed"),
        "the colour mode uses its snake_case spelling:\n{raw}",
    );
    assert!(raw.contains("particle_trail_len:17"), "trail length is written:\n{raw}");
    assert!(raw.contains("particle_trail_fade:0.25"), "fade is written:\n{raw}");
    assert!(raw.contains("particle_trail_width:1.75"), "width is written:\n{raw}");
    assert!(
        raw.contains("particle_loop_on_stagnation:false"),
        "the loop toggle is written:\n{raw}",
    );
    assert!(
        raw.contains("particle_stagnation_seconds:3.5"),
        "the threshold is written:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.particle_opacity, 0.35);
    assert_eq!(loaded.view.particle_color_mode, ParticleColorMode::Fixed);
    assert_eq!(loaded.view.particle_fixed_color, [12, 34, 56, 255]);
    assert_eq!(loaded.view.particle_trail_len, 17);
    assert_eq!(loaded.view.particle_trail_fade, 0.25);
    assert_eq!(loaded.view.particle_trail_width, 1.75);
    assert!(!loaded.view.particle_loop_on_stagnation);
    assert_eq!(loaded.view.particle_stagnation_seconds, 3.5);

    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded.view.restore(&mut restored_state, &mut restored_controls);
    assert_eq!(restored_state.particle_appearance, appearance);
    assert_eq!(restored_controls.particle_appearance, appearance);

    let legacy = load_legacy(false);
    let defaults = ParticleAppearance::default();
    assert_eq!(legacy.view.particle_opacity, defaults.opacity);
    assert_eq!(legacy.view.particle_color_mode, defaults.color_mode);
    assert_eq!(legacy.view.particle_fixed_color, defaults.fixed_color);
    assert_eq!(legacy.view.particle_trail_len, defaults.trail_len);
    assert_eq!(legacy.view.particle_trail_fade, defaults.trail_fade);
    assert_eq!(legacy.view.particle_trail_width, defaults.trail_width);
    assert_eq!(legacy.view.particle_loop_on_stagnation, defaults.loop_on_stagnation);
    assert_eq!(legacy.view.particle_stagnation_seconds, defaults.stagnation_seconds);
    let mut legacy_state = State::new(false);
    let mut legacy_controls = Controls::new();
    legacy.view.restore(&mut legacy_state, &mut legacy_controls);
    assert_eq!(legacy_state.particle_appearance, defaults, "older records default trails off");
    assert_eq!(legacy_controls.particle_appearance, defaults);
}

#[test]
fn a_legacy_view_record_without_particle_colour_pins_the_serialized_default() {
    // Ticket 48 added `particle_fixed_color` after this payload shape. The
    // fallback is asserted literally (not against the constant) so a future
    // brand/default change cannot silently rewrite old persisted records.
    let legacy = load_legacy(false);
    assert_eq!(legacy.view.particle_fixed_color, [0, 150, 136, 255]);
}

#[test]
fn restore_clamps_and_sanitizes_hostile_particle_appearance() {
    let record = ViewRecord {
        particle_opacity: f32::NAN,
        particle_fixed_color: [9, 8, 7, 3],
        particle_trail_len: 99_999,
        particle_trail_fade: f32::INFINITY,
        particle_trail_width: -50.0,
        particle_stagnation_seconds: f32::NEG_INFINITY,
        particle_loop_on_stagnation: false,
        ..Default::default()
    };
    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    let appearance = state.particle_appearance;
    assert_eq!(appearance.opacity, ParticleAppearance::default().opacity);
    assert_eq!(
        appearance.fixed_color, [9, 8, 7, 255],
        "a translucent stored colour loads opaque, RGB unchanged",
    );
    assert_eq!(appearance.trail_len, 32, "trail length clamps to the ring size");
    assert_eq!(appearance.trail_fade, ParticleAppearance::default().trail_fade);
    assert_eq!(appearance.trail_width, 0.05, "a negative width clamps to the min");
    assert_eq!(
        appearance.stagnation_seconds,
        ParticleAppearance::default().stagnation_seconds,
    );
    assert!(!appearance.loop_on_stagnation, "the boolean passes through");
    assert_eq!(controls.particle_appearance, appearance, "the mirror follows");
}
