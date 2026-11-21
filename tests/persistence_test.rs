// Moved out of `src/persistence.rs`'s inline module (ticket-14 review I-4).
// Covers the single APP_KEY envelope, ticket-07 migration, defaults, hostile
// values, view-state restoration and the style clamp (review I-3).

use eframe::Storage as _;
use glam::{DVec3, Vec3};

use vector_field_explorer::VfApp;
use vector_field_explorer::appearance_profile::{AppearanceProfile, builtins};
use vector_field_explorer::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::color_mapping::ColorQuantity;
use vector_field_explorer::color_ramp::{ColorRamp, RampKind, Rgb, ScaleMode};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::custom_density::{
    DensityFamily, DensitySpec, DensitySupport, SupportKind, TimeLaw,
};
use vector_field_explorer::field_probe::{
    CirculationLoop, GaussianSurface, IntegralStatus, LoopKind, MAX_LOOPS, MAX_SURFACES,
    SurfaceKind,
};
use vector_field_explorer::induction::{InductionCase, InductionScene};
use vector_field_explorer::interaction::{
    Bindings, CameraFeel, ChordModifiers, DEFAULT_PROFILE_NAME, InteractionAction,
    InteractionProfile, KeyChord, MAX_USER_PROFILES as MAX_INTERACTION_PROFILES,
    ProfileLibrary as InteractionProfileLibrary,
};
use vector_field_explorer::layer_style::LayerStyles;
use vector_field_explorer::object_scene::ObjectStyle;
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::particles::{MAX_PARTICLES, ParticleAppearance, ParticleColorMode};
use vector_field_explorer::persistence::{
    AppSettings, DensitySpecRecord, PresetUiRecord, ProbeLoopRecord, ProbeSurfaceRecord,
    ViewRecord, load, save,
};
use vector_field_explorer::plane::{DIM_ALPHA, PlaneAxis, PlaneFrame};
use vector_field_explorer::plot::PlotKind;
use vector_field_explorer::preset_library::PresetGroup;
use vector_field_explorer::profile_library::{MAX_USER_PROFILES, ProfileLibrary};
use vector_field_explorer::scalar::ScalarContext;
use vector_field_explorer::scalar_analysis::{
    ComparisonSeries, ComparisonSet, GradientSpec, MAX_SERIES_LABEL_BYTES, SeriesId, SeriesRole,
    SeriesStyle,
};
use vector_field_explorer::scalar_view::{LineProbe, PointProbe, ScalarSample};
use vector_field_explorer::scene_document::SceneDocument;
use vector_field_explorer::seed_layout::{SeedAnchor, SeedLayout, SeedPoint};
use vector_field_explorer::seed_layout_library::{
    DEFAULT_LAYOUT_NAME as DEFAULT_SEED_LAYOUT_NAME, MAX_USER_LAYOUTS as MAX_USER_SEED_LAYOUTS,
    SeedLayoutLibrary, SeedLayoutProfile,
};
use vector_field_explorer::source_scene::{
    FieldMode, SourceDisplay, SourceKindTag, SourceRecord, SourceScene,
};
use vector_field_explorer::sources::{FieldFamily, Source, SourceKind};
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::ui::style_panel::{
    AppStyle, MAX_FONT_SIZE, MAX_ROUNDING, MAX_SPACING, MIN_ROUNDING,
};
use vector_field_explorer::viewmodel::{
    ArrowLengthMode, DEFAULT_PARTICLE_SIZE, DEFAULT_PROPORTIONAL_EXPONENT, LeftClickAction,
    MAX_ARROW_OPACITY, MAX_DENSITIES, MAX_DOMAIN_EXTENT, MAX_MAGNITUDE_SCALE,
    MAX_PROPORTIONAL_EXPONENT, MIN_DOMAIN_EXTENT, MIN_MAGNITUDE_SCALE, STREAMLINE_MAX_LEN, State,
    StreamlineSeedMode,
};
use vector_field_explorer::viewport_style::{MAX_LINE_WIDTH, ViewportStyle};

use crate::support::storage::{MemoryStorage, from_ron, ron_string};

#[test]
fn old_record_defaults_vector_and_preserves_projection() {
    for mode in [false, true] {
        let mut legacy = legacy_settings(false);
        legacy.view.mode_3d = mode;
        let mut storage = MemoryStorage::default();
        eframe::set_value(&mut storage, eframe::APP_KEY, &legacy);
        let mut state = State::new(!mode);
        state.set_plot_kind(PlotKind::Curve2D);
        load(Some(&storage))
            .view
            .restore(&mut state, &mut Controls::new());
        assert_eq!(state.plot.kind, PlotKind::VectorField);
        assert_eq!(state.mode_3d, mode);
        assert_eq!(state.plot.vector_mode_3d, mode);
    }
}

#[test]
fn plot_round_trip_restores_drafts_not_cache() {
    let mut state = State::new(true);
    state.set_formula("(x, y, z)".into());
    state.set_scalar_source(ScalarContext::Curve, "x+2".into());
    state.set_scalar_source(ScalarContext::Surface, "x+y+3".into());
    state.set_plot_samples(127);
    state.set_plot_resolution(23);
    state.set_plot_width(0.025);
    state.set_plot_auto_height(false);
    state.set_plot_height_range(-4.0, 7.0);
    state.set_plot_kind(PlotKind::Curve2D);
    state.plot_scene();
    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: ViewRecord::capture(&state),
            ..Default::default()
        },
    );
    let raw = storage.get_string(eframe::APP_KEY).unwrap();
    for key in [
        "plot_kind:Curve2D",
        "curve_source:",
        "surface_source:",
        "curve_samples:127",
        "surface_resolution:23",
        "plot_line_width:",
        "plot_auto_height:false",
        "plot_height_min:",
        "plot_height_max:",
        "last_vector_mode_3d:",
    ] {
        assert!(raw.contains(key), "missing {key}: {raw}");
    }
    for key in ["accepted:", "samples:Some", "mesh", "cache", "camera:"] {
        assert!(!raw.contains(key), "unexpected {key}: {raw}");
    }
    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.formula, "(x, y, z)");
    let mut restored = State::new(false);
    loaded.view.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.plot.kind, PlotKind::Curve2D);
    assert_eq!(restored.plot.curve.source, "x+2");
    assert_eq!(restored.plot.surface.source, "x+y+3");
    assert_eq!(restored.plot.settings.curve_samples, 127);
    assert_eq!(restored.plot.settings.surface_resolution, 23);
    assert_eq!(restored.plot.settings.line_width, 0.025);
    assert!(!restored.plot.settings.auto_height);
    assert_eq!(
        (
            restored.plot.settings.height_min,
            restored.plot.settings.height_max
        ),
        (-4.0, 7.0)
    );
    assert!(restored.plot.curve.samples.is_none());
    assert!(restored.plot.surface.samples.is_none());
    restored.set_plot_kind(PlotKind::VectorField);
    assert!(restored.mode_3d);
    assert_eq!(restored.formula, "(x, y, z)");
}

#[test]
fn invalid_restored_draft_has_no_plot() {
    let mut state = State::new(false);
    state.set_plot_kind(PlotKind::Curve2D);
    state.plot_scene();
    state.set_active_source(")(".into());
    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: ViewRecord::capture(&state),
            ..Default::default()
        },
    );
    let mut restored = State::new(false);
    restored.set_plot_kind(PlotKind::Curve2D);
    restored.plot_scene();
    load(Some(&storage))
        .view
        .restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.plot.curve.source, ")(");
    assert!(restored.plot.curve.error.is_some());
    assert!(restored.plot.curve.accepted.is_none());
    assert!(restored.plot.curve.samples.is_none());
    assert!(restored.plot_scene().bounds.is_none());
}

#[test]
fn contradictory_projection_and_corrupt_controls() {
    let mut storage = MemoryStorage::default();
    let style = ron_string(&AppStyle::default());
    storage.set_string(eframe::APP_KEY, format!("(style:{style},view:(plot_kind:Surface3D,mode_3d:false,last_vector_mode_3d:Some(false),curve_samples:0,surface_resolution:999,plot_line_width:NaN,plot_auto_height:false,plot_height_min:inf,plot_height_max:-inf))"));
    let mut state = State::new(true);
    load(Some(&storage))
        .view
        .restore(&mut state, &mut Controls::new());
    assert_eq!(state.plot.kind, PlotKind::Surface3D);
    assert!(state.mode_3d);
    assert!(!state.plot.vector_mode_3d);
    assert_eq!(state.plot.settings.curve_samples, 2);
    assert_eq!(state.plot.settings.surface_resolution, 96);
    assert_eq!(state.plot.settings.line_width, 0.01);
    assert_eq!(
        (
            state.plot.settings.height_min,
            state.plot.settings.height_max
        ),
        (0.0, 1.0)
    );
    state.set_plot_kind(PlotKind::VectorField);
    assert!(!state.mode_3d);
    storage.set_string(
        eframe::APP_KEY,
        format!("(style:{style},view:(plot_kind:UnknownKind,density:12))"),
    );
    assert_eq!(load(Some(&storage)), AppSettings::default());
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
    assert_eq!(
        loaded.view.speed, 2.7,
        "the obsolete key is still readable from an old envelope"
    );

    let mut state = State::new(false);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);
    assert_eq!(
        STREAMLINE_MAX_LEN, 120,
        "streamlines keep a fixed point budget, not the old speed"
    );
    assert!(state.set_formula("(-y, x)".to_string()));
    assert!(state.set_streamlines(true));
    assert!(
        !state.snapshot().lines.is_empty(),
        "streamlines still trace at the fixed budget after an old speed loads",
    );

    let mut rewritten = MemoryStorage::default();
    save(&mut rewritten, &loaded);
    let raw = rewritten
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        !raw.contains(",speed:") && !raw.contains("(speed:"),
        "new records never write the obsolete key:\n{raw}"
    );
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

/// A user profile carrying the named builtin appearance under a test name.
fn appearance_profile(name: &str) -> AppearanceProfile {
    let mut profile = builtins()
        .into_iter()
        .find(|profile| profile.name == "High contrast")
        .expect("the named builtin exists");
    profile.name = name.to_owned();
    profile
}

fn interaction_chord(key: eframe::egui::Key) -> KeyChord {
    KeyChord {
        key,
        modifiers: ChordModifiers::default(),
    }
}

fn interaction_profile_named(name: &str) -> InteractionProfile {
    InteractionProfile {
        name: name.to_owned(),
        bindings: Bindings::default_bindings(),
        camera_feel: CameraFeel::default(),
    }
}

/// A valid active record with one replaced chord and non-default camera feel.
fn custom_interaction() -> InteractionProfile {
    let mut bindings = Bindings::default_bindings();
    bindings
        .assign(
            InteractionAction::ToggleControls,
            interaction_chord(eframe::egui::Key::J),
        )
        .expect("J is a free chord");
    InteractionProfile {
        name: "Custom".to_owned(),
        bindings,
        camera_feel: CameraFeel {
            orbit_sensitivity: 2.5,
            zoom_speed: 0.5,
            invert_yaw: true,
            invert_pitch: false,
            invert_zoom: true,
        },
    }
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
            appearance_expanded: false,
            controls_width: 300.0,
            presets_width: 210.0,
            appearance_width: 240.0,
            interaction_expanded: false,
            interaction_width: 240.0,
            sources_expanded: false,
            sources_width: 280.0,
            induction_expanded: false,
            induction_width: 280.0,
            ode_expanded: false,
            ode_width: 300.0,
            materials_expanded: false,
            materials_width: 300.0,
            maxwell_expanded: false,
            maxwell_width: 300.0,
            help_visible: false,
            #[cfg(not(target_arch = "wasm32"))]
            cas_expanded: true,
            #[cfg(not(target_arch = "wasm32"))]
            cas_width: 333.0,
        },
        view: ViewRecord::capture(&state),
        parameters: ParameterSet::default(),
        viewport_style: ViewportStyle::white_paper(),
        layer_styles: LayerStyles::default(),
        preset_ui: PresetUiRecord::default(),
        object_style: ObjectStyle::Solid,
        appearance_profiles: Vec::new(),
        interaction: InteractionProfile::default(),
        interaction_profiles: Vec::new(),
        seed_layouts: Vec::new(),
        up_axis: Default::default(),
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
        (app.controls.show_grid, app.controls.show_coordinate_system),
        (false, false)
    );
}

#[test]
fn object_style_defaults_to_solid_when_the_field_is_missing() {
    let raw = ron_string(&AppSettings::default());
    assert!(
        raw.contains("object_style:Solid"),
        "the current envelope writes the style:\n{raw}"
    );
    let without_field = raw.replace(",object_style:Solid", "");
    assert!(!without_field.contains("object_style"));
    let loaded: AppSettings = from_ron(&without_field);
    assert_eq!(loaded.object_style, ObjectStyle::Solid);
}

#[test]
fn wireframe_object_style_round_trips_through_storage() {
    let settings = AppSettings {
        object_style: ObjectStyle::Wireframe,
        viewport_style: ViewportStyle::white_paper(),
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("object_style:Wireframe"),
        "the style is written as its stable token:\n{raw}"
    );
    let loaded = load(Some(&storage));
    assert_eq!(loaded.object_style, ObjectStyle::Wireframe);

    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(app.state.object_style(), ObjectStyle::Wireframe);
}

#[test]
fn corrupt_object_style_falls_back_to_solid_without_losing_the_envelope() {
    let settings = AppSettings {
        style: AppStyle {
            theme_dark: false,
            accent: eframe::egui::Color32::from_rgb(12, 34, 56),
            rounding: 5.0,
            spacing: 8.0,
            font_size: 13.0,
        },
        object_style: ObjectStyle::Wireframe,
        ..AppSettings::default()
    };
    let raw = ron_string(&settings).replace("object_style:Wireframe", "object_style:Glowy");
    let mut storage = MemoryStorage::default();
    storage.set_string(eframe::APP_KEY, raw);
    let loaded = load(Some(&storage));
    assert_eq!(loaded.object_style, ObjectStyle::Solid);
    assert_eq!(
        loaded.style, settings.style,
        "only the corrupt style field falls back; the rest of the envelope survives"
    );
}

#[test]
fn non_string_object_style_is_repaired_without_losing_the_envelope() {
    let settings = AppSettings {
        style: AppStyle {
            theme_dark: false,
            accent: eframe::egui::Color32::from_rgb(90, 12, 34),
            rounding: 6.0,
            spacing: 9.0,
            font_size: 14.0,
        },
        object_style: ObjectStyle::Wireframe,
        ..AppSettings::default()
    };
    let raw = ron_string(&settings).replace("object_style:Wireframe", "object_style:5");
    let mut storage = MemoryStorage::default();
    storage.set_string(eframe::APP_KEY, raw);
    let loaded = load(Some(&storage));
    assert_eq!(loaded.object_style, ObjectStyle::Solid);
    assert_eq!(
        loaded.style, settings.style,
        "a non-string style field is repaired to Solid; the rest of the envelope survives"
    );
}

#[test]
fn object_style_setter_and_restore_own_the_dirty_flag() {
    let mut state = State::new(false);
    assert_eq!(state.object_style(), ObjectStyle::Solid);
    state.dirty = false;

    assert!(!state.set_object_style(ObjectStyle::Solid));
    assert!(!state.dirty, "an equal style is a no-op");
    assert!(state.set_object_style(ObjectStyle::Wireframe));
    assert!(state.dirty, "a real change marks the scene dirty once");
    assert_eq!(state.object_style(), ObjectStyle::Wireframe);

    state.dirty = false;
    state.restore_object_style(ObjectStyle::Solid);
    assert_eq!(state.object_style(), ObjectStyle::Solid);
    assert!(!state.dirty, "startup restore never dirties the scene");
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
            (app.controls.show_grid, app.controls.show_coordinate_system),
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
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
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
    assert_eq!(
        state.magnitude_scale, 1.0,
        "the missing magnitude scale defaults to 1"
    );
    assert_eq!(
        state.arrow_scale, 2.0,
        "the rest of the legacy record still loads"
    );
    assert_eq!(
        controls.arrow_length_mode,
        ArrowLengthMode::Uniform,
        "the mirror follows"
    );
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
    assert_eq!(
        state.arrow_style,
        ArrowStyle::Cone,
        "older records use the cone"
    );
    assert_eq!(
        state.arrow_style_2d,
        ArrowStyle2D::Flat,
        "older records use the 2D flat ribbon"
    );
    assert_eq!(
        state.arrow_params,
        ArrowParams::default(),
        "older records use default geometry"
    );
    assert_eq!(controls.arrow_style, ArrowStyle::Cone, "the mirror follows");
    assert_eq!(
        controls.arrow_style_2d,
        ArrowStyle2D::Flat,
        "the 2D mirror follows"
    );
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
    assert_eq!(
        state.arrow_style,
        ArrowStyle::Cone,
        "restore keeps the decoded 3D selection"
    );
    assert_eq!(
        state.arrow_style_2d,
        ArrowStyle2D::Flat,
        "restore defaults the 2D selection"
    );
    assert_eq!(
        controls.arrow_style,
        ArrowStyle::Cone,
        "the 3D mirror follows"
    );
    assert_eq!(
        controls.arrow_style_2d,
        ArrowStyle2D::Flat,
        "the 2D mirror follows"
    );
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
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
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

    let settings = AppSettings {
        view: record,
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        !raw.contains("normalize"),
        "the legacy key is no longer written:\n{raw}"
    );

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
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("proportional_exponent:2.5"),
        "the contrast is written:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.proportional_exponent, 2.5);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded
        .view
        .restore(&mut restored_state, &mut restored_controls);
    assert_eq!(restored_state.proportional_exponent, 2.5);
    assert_eq!(
        restored_controls.proportional_exponent, 2.5,
        "the mirror follows"
    );

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
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("arrow_opacity:0.35"),
        "the global opacity is written:\n{raw}"
    );
    assert!(
        raw.contains("plane_dim_opacity:0.65"),
        "the off-plane opacity is written:\n{raw}"
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.arrow_opacity, 0.35);
    assert_eq!(loaded.view.plane_dim_opacity, 0.65);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded
        .view
        .restore(&mut restored_state, &mut restored_controls);
    assert_eq!(restored_state.arrow_opacity, 0.35);
    assert_eq!(restored_state.plane_dim_opacity, 0.65);
    assert_eq!(restored_controls.arrow_opacity, 0.35, "the mirror follows");
    assert_eq!(restored_controls.plane_dim_opacity, 0.65);

    // The pre-ticket-36 shape has neither key at all.
    let legacy = load_legacy(false);
    assert_eq!(
        legacy.view.arrow_opacity, MAX_ARROW_OPACITY,
        "an older record loads fully opaque"
    );
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
    assert_eq!(
        state.plane_dim_opacity, DIM_ALPHA,
        "Infinity falls back to the dim alpha"
    );
    assert_eq!(
        controls.arrow_opacity, 1.0,
        "the mirror follows the fallback"
    );
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
    #[cfg(not(target_arch = "wasm32"))]
    {
        assert!(!loaded.layout.cas_expanded);
        assert_eq!(loaded.layout.cas_width, 260.0);
    }
    assert_eq!(
        loaded.style, style,
        "the ticket-07 appearance survives the migration"
    );
    assert_eq!(
        loaded.layout,
        LayoutState::default(),
        "new layout fields start at defaults"
    );
    assert_eq!(
        loaded.view,
        ViewRecord::default(),
        "new view fields start at defaults"
    );
}

#[test]
fn old_five_field_layout_envelope_preserves_style_and_view() {
    let expected = sample_settings();
    let raw = format!(
        "(style:{},layout:(controls_expanded:false,presets_expanded:true,controls_width:333.0,presets_width:211.0,help_visible:false),view:{})",
        ron_string(&expected.style),
        ron_string(&expected.view)
    );
    let mut storage = MemoryStorage::default();
    storage.set_string(eframe::APP_KEY, raw);
    let loaded = load(Some(&storage));
    assert_eq!(loaded.style, expected.style);
    assert_eq!(loaded.view, expected.view);
    assert!(!loaded.layout.controls_expanded && loaded.layout.presets_expanded);
    assert!(!loaded.layout.help_visible);
    assert_eq!(
        (loaded.layout.controls_width, loaded.layout.presets_width),
        (333.0, 211.0)
    );
    #[cfg(not(target_arch = "wasm32"))]
    {
        assert!(!loaded.layout.cas_expanded);
        assert_eq!(loaded.layout.cas_width, 260.0);
    }
    save(&mut storage, &loaded);
    assert_eq!(load(Some(&storage)), loaded);
}

#[test]
fn corrupt_widths_are_sanitized_on_load() {
    let mut settings = sample_settings();
    settings.layout.controls_width = f32::NAN;
    settings.layout.presets_width = 99_999.0;
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert!(
        loaded.layout.controls_width.is_finite(),
        "a NaN width is replaced"
    );
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
    assert_eq!(
        loaded.style.font_size, 13.0,
        "an infinite font size falls back to the default"
    );
    assert_eq!(
        loaded.style.spacing, MAX_SPACING,
        "huge spacing clamps to the max"
    );
    assert_eq!(
        loaded.style.rounding, MIN_ROUNDING,
        "negative rounding clamps to the min"
    );

    let mut settings = sample_settings();
    settings.style.font_size = f32::NAN;
    settings.style.spacing = f32::NEG_INFINITY;
    settings.style.rounding = f32::NAN;
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.style.font_size, 13.0,
        "a NaN font size falls back to the default"
    );
    assert_eq!(
        loaded.style.spacing, 8.0,
        "an infinite spacing falls back to the default"
    );
    assert_eq!(
        loaded.style.rounding, 6.0,
        "a NaN rounding falls back to the default"
    );
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
    assert_eq!(
        restored_state.arrow_opacity, state.arrow_opacity,
        "arrow opacity"
    );
    assert_eq!(
        restored_state.plane_dim_opacity, state.plane_dim_opacity,
        "off-plane opacity",
    );
    assert_eq!(
        restored_controls.arrow_opacity, state.arrow_opacity,
        "the mirror follows opacity"
    );
    assert_eq!(
        restored_controls.plane_dim_opacity, state.plane_dim_opacity,
        "the mirror follows the off-plane opacity",
    );
    assert_eq!(
        restored_state.arrow_length_mode, state.arrow_length_mode,
        "arrow length mode"
    );
    assert_eq!(
        restored_state.magnitude_scale, state.magnitude_scale,
        "magnitude scale"
    );
    assert_eq!(
        restored_state.proportional_exponent, state.proportional_exponent,
        "proportional contrast",
    );
    assert_eq!(restored_state.colormap, state.colormap, "colormap");
    assert_eq!(restored_state.auto_fit, state.auto_fit, "auto-fit");
    assert_eq!(restored_state.color_min, state.color_min, "color min");
    assert_eq!(restored_state.color_max, state.color_max, "color max");
    assert_eq!(restored_state.streamlines, state.streamlines, "streamlines");
    assert_eq!(
        restored_state.seed_mode, state.seed_mode,
        "streamline seed mode"
    );
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
    assert_eq!(
        restored_state.arrow_style,
        ArrowStyle::Cylinder,
        "arrow style"
    );
    assert_eq!(
        restored_state.arrow_style_2d,
        ArrowStyle2D::Streamline,
        "2D arrow style"
    );
    assert_eq!(restored_state.arrow_params.sides, 8, "arrow sides");
    assert_eq!(
        restored_state.arrow_params.head_length, 0.4,
        "arrow head length"
    );
    assert_eq!(
        restored_controls.arrows, restored_state.arrows,
        "the mirror follows the toggle"
    );
    assert_eq!(
        restored_controls.arrow_style, restored_state.arrow_style,
        "the mirror follows the style",
    );
    assert_eq!(
        restored_controls.arrow_style_2d, restored_state.arrow_style_2d,
        "the mirror follows the 2D style",
    );
    assert_eq!(
        restored_controls.arrow_params, restored_state.arrow_params,
        "the mirror follows the params",
    );
    assert_eq!(
        restored_controls.seed_mode, state.seed_mode,
        "the mirror follows the seed mode"
    );
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
    assert_eq!(
        controls.density, state.density,
        "the captured mirror stays in sync"
    );
    assert!(
        restored_state.dirty,
        "a restored view asks for one scene rebuild"
    );
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
    assert_eq!(
        controls.seed_mode,
        StreamlineSeedMode::Auto,
        "the mirror follows"
    );
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
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("seed_mode:Click"),
        "the seed mode is written with its stable spelling:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.seed_mode, StreamlineSeedMode::Click);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded
        .view
        .restore(&mut restored_state, &mut restored_controls);
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
    assert_eq!(
        controls.left_click_action,
        LeftClickAction::SeedStreamline,
        "the mirror follows"
    );
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
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("left_click_action:AddParticles"),
        "the action is written with its stable spelling:\n{raw}",
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.left_click_action, LeftClickAction::AddParticles);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded
        .view
        .restore(&mut restored_state, &mut restored_controls);
    assert_eq!(
        restored_state.left_click_action,
        LeftClickAction::AddParticles
    );
    assert_eq!(
        restored_controls.left_click_action,
        LeftClickAction::AddParticles
    );
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
    assert!(
        state.arrow_scale.is_finite(),
        "a NaN arrow scale falls back"
    );
    assert!(
        state.magnitude_scale.is_finite(),
        "a NaN magnitude scale falls back"
    );
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
    assert!(
        state.domain.min.x < state.domain.max.x,
        "the domain stays a real box"
    );
    assert_eq!(
        state.arrow_params.head_length, 0.3,
        "a NaN head length falls back"
    );
    assert_eq!(
        state.arrow_params.head_width, 0.12,
        "an infinite head width falls back"
    );
    assert_eq!(
        state.arrow_params.shaft_radius, 0.01,
        "a negative radius clamps to the min"
    );
    assert_eq!(
        state.arrow_params.sides, 16,
        "an absurd side count clamps to the max"
    );
    assert_eq!(
        state.particle_size, DEFAULT_PARTICLE_SIZE,
        "a NaN particle size resets to the default",
    );
    assert_eq!(
        state.particle_count, MAX_PARTICLES as u32,
        "an absurd particle count clamps to the cap",
    );
    assert_eq!(
        controls.particle_size, state.particle_size,
        "the mirror follows the size"
    );
    assert_eq!(
        controls.particle_count, state.particle_count,
        "the mirror follows the count"
    );
}

#[test]
fn restore_clamps_a_huge_magnitude_scale() {
    let record = ViewRecord {
        magnitude_scale: 1.0e9,
        ..Default::default()
    };
    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    assert_eq!(
        state.magnitude_scale, MAX_MAGNITUDE_SCALE,
        "the seam bounds the stored value"
    );
    assert_eq!(
        controls.magnitude_scale, MAX_MAGNITUDE_SCALE,
        "the mirror follows the clamp"
    );
}

#[test]
fn restore_clamps_a_huge_proportional_exponent() {
    let record = ViewRecord {
        proportional_exponent: 1.0e6,
        ..Default::default()
    };
    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    assert_eq!(
        state.proportional_exponent, MAX_PROPORTIONAL_EXPONENT,
        "the seam bounds the stored contrast",
    );
    assert_eq!(
        controls.proportional_exponent, MAX_PROPORTIONAL_EXPONENT,
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
    assert!(
        !state.parse_error.is_empty(),
        "the bad stored formula is reported, not hidden"
    );
    assert!(
        state.field.eval3(Vec3::ZERO).is_some(),
        "a usable field stays in place"
    );
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
    assert_eq!(
        controls.domain_min, state.domain.min,
        "the mirror is refreshed"
    );
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
    assert_eq!(
        app.orbit.target,
        app.state.domain.center(),
        "launch frames the restored box"
    );
    let default = OrbitCamera::default();
    assert_eq!(app.orbit.yaw, default.yaw, "launch keeps the default yaw");
    assert_eq!(
        app.orbit.pitch, default.pitch,
        "launch keeps the default pitch"
    );
    assert_eq!(
        app.orbit.distance, default.distance,
        "launch keeps the default distance"
    );
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
    assert_eq!(
        app.orbit.target.x, centre.x,
        "2D launch follows the box centre in x"
    );
    assert_eq!(
        app.orbit.target.y, centre.y,
        "2D launch follows the box centre in y"
    );
    assert_eq!(
        app.orbit.target.z, 0.0,
        "2D launch leaves the depth target at the default"
    );
}

/// Ticket 47: no saved domain (or an empty storage) keeps the pre-ticket launch
/// camera — the default box centres on the origin.
#[test]
fn a_launch_without_a_saved_domain_still_targets_the_origin() {
    assert_eq!(
        VfApp::new().orbit.target,
        Vec3::ZERO,
        "no storage keeps the origin target"
    );

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
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("particle_size:2.25"),
        "the size is written:\n{raw}"
    );
    assert!(
        raw.contains("particle_count:500"),
        "the count is written:\n{raw}"
    );

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.particle_size, 2.25);
    assert_eq!(loaded.view.particle_count, 500);
    let mut restored_state = State::new(false);
    let mut restored_controls = Controls::new();
    loaded
        .view
        .restore(&mut restored_state, &mut restored_controls);
    assert_eq!(
        restored_state.particle_size, 2.25,
        "the size survives the round trip"
    );
    assert_eq!(
        restored_state.particle_count, 500,
        "the count survives the round trip"
    );
    assert_eq!(
        restored_controls.particle_size, 2.25,
        "the mirror follows the size"
    );
    assert_eq!(
        restored_controls.particle_count, 500,
        "the mirror follows the count"
    );

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
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("particle_opacity:0.35"),
        "opacity is written:\n{raw}"
    );
    assert!(
        raw.contains("particle_color_mode:fixed"),
        "the colour mode uses its snake_case spelling:\n{raw}",
    );
    assert!(
        raw.contains("particle_trail_len:17"),
        "trail length is written:\n{raw}"
    );
    assert!(
        raw.contains("particle_trail_fade:0.25"),
        "fade is written:\n{raw}"
    );
    assert!(
        raw.contains("particle_trail_width:1.75"),
        "width is written:\n{raw}"
    );
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
    loaded
        .view
        .restore(&mut restored_state, &mut restored_controls);
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
    assert_eq!(
        legacy.view.particle_loop_on_stagnation,
        defaults.loop_on_stagnation
    );
    assert_eq!(
        legacy.view.particle_stagnation_seconds,
        defaults.stagnation_seconds
    );
    let mut legacy_state = State::new(false);
    let mut legacy_controls = Controls::new();
    legacy.view.restore(&mut legacy_state, &mut legacy_controls);
    assert_eq!(
        legacy_state.particle_appearance, defaults,
        "older records default trails off"
    );
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
        appearance.fixed_color,
        [9, 8, 7, 255],
        "a translucent stored colour loads opaque, RGB unchanged",
    );
    assert_eq!(
        appearance.trail_len, 32,
        "trail length clamps to the ring size"
    );
    assert_eq!(
        appearance.trail_fade,
        ParticleAppearance::default().trail_fade
    );
    assert_eq!(
        appearance.trail_width, 0.05,
        "a negative width clamps to the min"
    );
    assert_eq!(
        appearance.stagnation_seconds,
        ParticleAppearance::default().stagnation_seconds,
    );
    assert!(!appearance.loop_on_stagnation, "the boolean passes through");
    assert_eq!(
        controls.particle_appearance, appearance,
        "the mirror follows"
    );
}

#[test]
fn streamline_count_round_trips_and_missing_field_defaults_to_auto() {
    let mut state = State::new(false);
    state.set_streamline_count(37);
    let record = ViewRecord::capture(&state);
    assert_eq!(record.streamline_count, 37);

    let mut restored = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut restored, &mut controls);
    assert_eq!(restored.streamline_count, 37);
    assert_eq!(controls.streamline_count, 37);

    let missing: ViewRecord = serde_json::from_str("{}").expect("all fields default");
    assert_eq!(missing.streamline_count, 0);
}

fn source_scene() -> SourceScene {
    let mut scene = SourceScene::default();
    for (index, tag) in SourceKindTag::ALL.into_iter().enumerate() {
        let mut source = tag.default_source();
        let i = index as f64;
        // Every entry differs in position, orientation and its kind-specific
        // numbers, so a lost, swapped or defaulted field cannot pass unnoticed.
        source.center = DVec3::new(1.0 + i, -2.0 - i, 0.5 + i);
        source.axis = DVec3::new(1.0, 2.0 + i, 3.0);
        source.kind = match tag {
            SourceKindTag::PointCharge => SourceKind::PointCharge { charge: -1.5 - i },
            SourceKindTag::FiniteLineCharge => SourceKind::FiniteLineCharge {
                charge: 2.0 + i,
                length: 1.25 + i * 0.25,
            },
            SourceKindTag::ChargedRing => SourceKind::ChargedRing {
                charge: -3.0 - i,
                radius: 0.75 + i * 0.25,
            },
            SourceKindTag::ChargedDisc => SourceKind::ChargedDisc {
                charge: 4.0 + i,
                radius: 1.5 + i * 0.25,
            },
            SourceKindTag::SolidSphere => SourceKind::SolidSphere {
                charge: -5.0 - i,
                radius: 2.0 + i * 0.25,
            },
            SourceKindTag::StraightWire => SourceKind::StraightWire { current: 6.0 + i },
            SourceKindTag::CurrentLoop => SourceKind::CurrentLoop {
                current: -7.0 - i,
                radius: 2.5 + i * 0.25,
            },
            SourceKindTag::Solenoid => SourceKind::Solenoid {
                current: 8.0 + i,
                radius: 0.5 + i * 0.05,
                length: 2.0 + i * 0.5,
                turns: 3 + index as u32,
            },
        };
        scene.add(source).unwrap();
    }
    assert!(scene.set_enabled(2, false), "one entry stays disabled");
    scene.set_display(SourceDisplay::Electric);
    scene.set_softening(0.02).unwrap();
    scene.set_allow_inaccurate(true);
    scene
}

#[test]
fn source_scene_round_trips_through_the_view_record() {
    let mut state = State::new(true);
    state.set_source_scene(source_scene());
    state.set_field_mode(FieldMode::Sources);
    let record = ViewRecord::capture(&state);
    assert_eq!(record.field_mode, FieldMode::Sources);
    assert_eq!(record.source_entries.len(), 8);
    assert_eq!(record.source_display, SourceDisplay::Electric);
    assert_eq!(record.source_softening, 0.02);
    assert!(record.source_allow_inaccurate);

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: record.clone(),
            ..Default::default()
        },
    );
    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.source_entries, record.source_entries);
    assert_eq!(loaded.view.source_entries.len(), 8);

    let mut restored = State::new(true);
    let mut controls = Controls::new();
    loaded.view.restore(&mut restored, &mut controls);
    assert_eq!(restored.field_mode, FieldMode::Sources);
    assert_eq!(restored.sources.entries.len(), 8);
    assert_eq!(restored.sources.display, SourceDisplay::Electric);
    assert_eq!(restored.sources.softening, 0.02);
    assert!(restored.sources.allow_inaccurate);
    assert_eq!(
        restored.sources.entries, state.sources.entries,
        "every restored entry keeps its enabled flag, center, axis and kind parameters",
    );
    assert!(
        !restored.sources.entries[2].enabled,
        "the disabled entry stays disabled",
    );
    assert!(
        restored.sources.entries[0].source.kind != SourceKindTag::PointCharge.default_source().kind,
        "the signed strength survives instead of reverting to the kind default",
    );
    assert!(restored.formula == state.formula);
}

#[test]
fn a_legacy_record_opens_in_formula_with_opt_in_off() {
    let record: ViewRecord = serde_json::from_str("{}").unwrap();
    assert_eq!(record.field_mode, FieldMode::Formula);
    assert!(record.source_entries.is_empty());
    assert_eq!(record.source_display, SourceDisplay::Both);
    assert_eq!(record.source_softening, 0.1);
    assert!(!record.source_allow_inaccurate);
}

#[test]
fn malformed_source_records_are_skipped_without_losing_other_settings() {
    let json = r#"{
        "formula": "(1, 2, 3)",
        "field_mode": "Sources",
        "source_entries": [
            {"kind":"ChargedRing","center":[0,0,0],"axis":[0,0,1],"strength":1,"radius":1},
            {"kind":"NotAKind","center":[0,0,0],"axis":[0,0,1],"strength":1},
            {"kind":"SolidSphere","center":[0,0,0],"axis":[0,0,1],"strength":1,"radius":0}
        ],
        "source_display": "Both",
        "source_softening": 0.1,
        "source_allow_inaccurate": "not-a-bool"
    }"#;
    let record: ViewRecord = serde_json::from_str(json).unwrap();
    let mut state = State::new(true);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    assert_eq!(state.formula, "(1, 2, 3)");
    assert_eq!(state.sources.entries.len(), 1);
    assert!(!state.sources.allow_inaccurate);
    assert!(state.source_notice.is_some());
}

#[test]
fn a_record_without_valid_sources_forces_formula_mode() {
    let record = ViewRecord {
        field_mode: FieldMode::Sources,
        source_entries: vec![SourceRecord {
            kind: "NotAKind".into(),
            ..SourceRecord::default()
        }],
        ..ViewRecord::default()
    };
    let mut state = State::new(false);
    let mut controls = Controls::new();
    record.restore(&mut state, &mut controls);
    assert_eq!(state.field_mode, FieldMode::Formula);
    assert!(state.sources.entries.is_empty());
    assert!(state.source_notice.is_some());
}

#[test]
fn source_records_keep_order_and_skip_invalid_numbers() {
    let mut scene = SourceScene::default();
    for tag in SourceKindTag::ALL {
        let mut source = tag.default_source();
        source.center = DVec3::new(1.0, 2.0, 3.0);
        scene.add(source).unwrap();
    }
    let mut records = scene.to_records();
    assert_eq!(records.len(), 8);
    let mut invalid = records[0].clone();
    invalid.center[0] = f64::NAN;
    records.insert(1, invalid);
    assert_eq!(records.len(), 9, "the crafted non-finite record is present");

    let restored = SourceScene::from_records(&records);
    assert_eq!(
        restored.entries.len(),
        8,
        "a record with a non-finite number is skipped"
    );
    assert_eq!(
        restored.entries[0].source.kind,
        scene.entries[0].source.kind
    );
    assert_eq!(
        restored.entries[7].source.kind,
        scene.entries[7].source.kind
    );
}

#[test]
fn global_reset_clears_source_scene_and_sampling_cache() {
    let mut app = VfApp::new();
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    app.state.set_source_scene(scene);
    app.state.set_field_mode(FieldMode::Sources);
    app.state.refresh_source_sampling();
    assert!(app.state.source_sampling().is_some());

    app.test_harness().reset_all();

    assert!(app.state.sources.entries.is_empty());
    assert!(app.state.source_sampling().is_none());
    assert!(app.state.source_notice.is_none());
    assert_eq!(
        app.state.field_mode,
        FieldMode::Formula,
        "global reset returns to the formula adapter",
    );
}

#[test]
fn induction_scene_round_trips_and_legacy_defaults_to_formula() {
    let scene = InductionScene {
        case: InductionCase::Solenoid,
        center: DVec3::new(0.5, -1.0, 2.0),
        axis: DVec3::Y,
        radius: 0.5,
        length: 2.0,
        turns: 8,
        amplitude: 2.0,
        frequency: 0.125,
        display: SourceDisplay::Magnetic,
        allow_inaccurate: true,
    };
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Induction);
    assert!(state.set_induction_scene(scene.clone()));

    let record = ViewRecord::capture(&state);
    assert_eq!(record.field_mode, FieldMode::Induction);
    assert_eq!(record.induction_case, "solenoid");
    assert_eq!(record.induction_center, [0.5, -1.0, 2.0]);
    assert_eq!(record.induction_axis, [0.0, 1.0, 0.0]);
    assert_eq!(record.induction_radius, 0.5);
    assert_eq!(record.induction_length, 2.0);
    assert_eq!(record.induction_turns, 8);
    assert_eq!(record.induction_amplitude, 2.0);
    assert_eq!(record.induction_frequency, 0.125);
    assert_eq!(record.induction_display, SourceDisplay::Magnetic);
    assert!(record.induction_allow_inaccurate);

    let json = serde_json::to_string(&record).unwrap();
    let back: ViewRecord = serde_json::from_str(&json).unwrap();
    let mut restored = State::new(true);
    let mut controls = Controls::new();
    back.restore(&mut restored, &mut controls);
    assert_eq!(restored.field_mode, FieldMode::Induction);
    assert_eq!(restored.induction, scene);
    assert!(restored.induction_error.is_none());

    let legacy: ViewRecord = serde_json::from_str("{}").unwrap();
    let mut legacy_state = State::new(false);
    legacy.restore(&mut legacy_state, &mut Controls::new());
    assert_eq!(legacy_state.field_mode, FieldMode::Formula);
    assert_eq!(legacy_state.induction, InductionScene::default());
    assert!(legacy_state.source_notice.is_none());
    assert!(
        legacy_state.induction_error.is_none(),
        "an absent induction payload is valid and silent",
    );
}

#[test]
fn malformed_induction_fields_fall_back_and_report_the_error() {
    let text = r#"{
        "field_mode": "Induction",
        "induction_case": "bogus",
        "induction_center": [0.0, 0.0, 0.0],
        "induction_axis": [0.0, 0.0, 2.0],
        "induction_radius": -1.0,
        "induction_frequency": -2.0
    }"#;
    let record: ViewRecord = serde_json::from_str(text).unwrap();
    let mut state = State::new(true);
    record.restore(&mut state, &mut Controls::new());
    assert_eq!(state.induction, InductionScene::default());
    assert!(
        state
            .induction_error
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("induction"),
        "the induction panel/status slot carries the reason: {:?}",
        state.induction_error,
    );
    assert!(state.source_notice.is_none());
}

#[test]
fn global_reset_clears_induction_cache_and_restores_defaults() {
    let mut app = VfApp::new();
    app.state.set_field_mode(FieldMode::Induction);
    let mut scene = app.state.induction.clone();
    scene.radius = 1.75;
    assert!(app.state.set_induction_scene(scene));
    app.test_harness().advance_simulation();
    assert!(app.test_harness().induction_prepared_arc().is_some());

    app.test_harness().reset_all();

    assert_eq!(app.state.field_mode, FieldMode::Formula);
    assert_eq!(app.state.induction, InductionScene::default());
    assert!(app.state.induction_prepared.is_none());
    assert!(app.state.induction_sampling.is_none());
    assert!(app.state.induction_error.is_none());
}

fn sample_parameters() -> ParameterSet {
    ParameterSet {
        version: 1,
        entries: vec![
            Parameter {
                name: "gain".to_string(),
                value: 2.5,
                min: Some(0.0),
                max: Some(5.0),
                step: 0.5,
            },
            Parameter {
                name: "offset".to_string(),
                value: -1.0,
                min: None,
                max: None,
                step: 0.25,
            },
        ],
    }
}

#[test]
fn old_envelope_without_parameters_loads_the_empty_default_set() {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &legacy_settings(false));

    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.parameters,
        ParameterSet::default(),
        "an envelope without the key gets the default set"
    );
    assert_eq!(loaded.parameters.version, 1);
    assert!(loaded.parameters.entries.is_empty());
    assert_eq!(loaded.view.formula, "(x, y)", "the old view still loads");
    assert_eq!(loaded.view.density, 24);
    assert_eq!(loaded.view.arrow_scale, 2.0);
}

#[test]
fn valid_parameters_round_trip_through_save_and_load() {
    let mut settings = sample_settings();
    settings.parameters = sample_parameters();

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(raw.contains("parameters:"), "the set is written:\n{raw}");

    let loaded = load(Some(&storage));
    assert_eq!(loaded.parameters, settings.parameters);
    assert_eq!(loaded, settings, "no other field changes in the round trip");
}

#[test]
fn corrupt_parameters_fall_back_to_the_default_set_without_losing_other_fields() {
    let mut settings = sample_settings();
    settings.parameters = ParameterSet {
        version: 2,
        entries: sample_parameters().entries,
    };

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));

    assert_eq!(
        loaded.parameters,
        ParameterSet::default(),
        "an unsupported version falls back to the empty default set"
    );
    assert_eq!(loaded.style, settings.style, "the appearance survives");
    assert_eq!(loaded.layout, settings.layout, "the layout survives");
    assert_eq!(loaded.view, settings.view, "the view survives");
}

#[test]
fn restoring_parameters_does_not_dirty_the_scene() {
    let mut settings = sample_settings();
    settings.parameters = sample_parameters();

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));

    // Startup assigns the loaded set directly; unlike a parameter edit, a
    // restore must not request a scene rebuild on its own.
    let mut state = State::new(false);
    state.dirty = false;
    state.parameters = loaded.parameters.clone();
    assert!(
        !state.dirty,
        "assigning restored parameters leaves dirty alone"
    );
    assert_eq!(state.parameter_value("gain"), Some(2.5));

    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        app.state.parameters, settings.parameters,
        "the app constructor restores the persisted set"
    );
}

#[test]
fn stored_parameters_bind_the_restored_formula() {
    for (mode_3d, formula) in [
        (false, "(gain*x, offset*y)"),
        (true, "(gain*x, offset*y, gain*z)"),
    ] {
        let mut settings = sample_settings();
        settings.parameters = sample_parameters();
        settings.view.formula = formula.to_string();
        settings.view.mode_3d = mode_3d;
        settings.view.last_vector_mode_3d = Some(mode_3d);

        let mut storage = MemoryStorage::default();
        save(&mut storage, &settings);

        // Startup restores the parameter set before the formula is parsed, so
        // the stored formula binds the stored values.
        let app = VfApp::new_with_storage(Some(&storage));
        assert_eq!(
            app.state.parse_error, "",
            "{formula}: the restored formula binds the restored set"
        );
        assert_eq!(
            app.state.parameters, settings.parameters,
            "{formula}: the stored set is restored"
        );
        let value = app
            .state
            .field
            .eval3(Vec3::new(2.0, 2.0, 2.0))
            .expect("the restored field evaluates");
        assert!(
            (value.x - 5.0).abs() < 1e-6,
            "{formula}: u = {}, expected 5",
            value.x
        );
        assert!(
            (value.y + 2.0).abs() < 1e-6,
            "{formula}: v = {}, expected -2",
            value.y
        );
        if mode_3d {
            assert!(
                (value.z - 5.0).abs() < 1e-6,
                "{formula}: w = {}, expected 5",
                value.z
            );
        }
    }
}

#[test]
fn old_envelope_without_viewport_style_keeps_everything_and_defaults_the_record() {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &legacy_settings(false));
    let loaded = load(Some(&storage));

    assert_eq!(
        loaded.viewport_style,
        ViewportStyle::default(),
        "an old envelope loads the historical dark-laboratory look"
    );
    assert_eq!(loaded.view.formula, "(x, y)", "the view survives");
    assert_eq!(loaded.view.density, 24);
    assert_eq!(loaded.view.arrow_scale, 2.0);
    assert_eq!(loaded.style, AppStyle::default(), "the style survives");
    assert_eq!(loaded.layout, LayoutState::default(), "the layout survives");
    assert_eq!(
        loaded.parameters,
        ParameterSet::default(),
        "a missing parameter set defaults without failing the envelope"
    );

    // The new save path writes the record back.
    let mut rewritten = MemoryStorage::default();
    save(&mut rewritten, &loaded);
    let raw = rewritten
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("viewport_style:"),
        "new records write the viewport style:\n{raw}"
    );
}

#[test]
fn viewport_style_round_trips_through_storage() {
    let style = ViewportStyle::white_paper();
    let settings = AppSettings {
        viewport_style: style,
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let loaded = load(Some(&storage));
    assert_eq!(loaded.viewport_style, style);

    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        app.state.viewport_style, style,
        "the app constructor restores the viewport style"
    );
}

#[test]
fn corrupt_viewport_style_falls_back_without_discarding_other_sections() {
    let mut settings = sample_settings();
    settings.viewport_style.grid_color = [f32::NAN, 0.2, 0.3, 1.0];
    settings.viewport_style.grid_width = 9.0;
    let expected_style = settings.style.clone();
    let expected_layout = settings.layout.clone();
    let expected_view = settings.view.clone();

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));

    assert_eq!(loaded.style, expected_style, "the appearance survives");
    assert_eq!(loaded.layout, expected_layout, "the layout survives");
    assert_eq!(loaded.view, expected_view, "the view survives");
    let default = ViewportStyle::default();
    assert_eq!(
        loaded.viewport_style.grid_color, default.grid_color,
        "a non-finite colour resets to the default record"
    );
    assert_eq!(
        loaded.viewport_style.grid_width, MAX_LINE_WIDTH,
        "a finite out-of-range width clamps"
    );
}

#[test]
fn restoring_viewport_style_does_not_dirty_the_scene() {
    let mut settings = sample_settings();
    settings.viewport_style = ViewportStyle::high_contrast();

    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));

    // Startup assigns the loaded record directly; unlike a user commit, a
    // restore must not request a scene rebuild on its own.
    let mut state = State::new(false);
    state.dirty = false;
    state.viewport_style = loaded.viewport_style;
    assert!(
        !state.dirty,
        "assigning the restored viewport style leaves dirty alone"
    );

    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        app.state.viewport_style, settings.viewport_style,
        "the app constructor restores the persisted record"
    );
}

#[test]
fn settings_drop_duplicate_or_excess_user_profiles_on_load() {
    let mut profiles = vec![appearance_profile("  Nord  "), appearance_profile("Nord")];
    for index in 0..MAX_USER_PROFILES {
        profiles.push(appearance_profile(&format!("Look {index}")));
    }
    let settings = AppSettings {
        appearance_profiles: profiles,
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.appearance_profiles.len(),
        MAX_USER_PROFILES,
        "the excess is truncated after the duplicate is dropped"
    );
    assert_eq!(
        loaded.appearance_profiles.first().map(|p| p.name.as_str()),
        Some("Nord"),
        "the first sanitized record is kept"
    );
    assert_eq!(
        loaded.appearance_profiles.last().map(|p| p.name.as_str()),
        Some("Look 30"),
        "the cap keeps the earliest entries in order"
    );
    let names: std::collections::HashSet<&str> = loaded
        .appearance_profiles
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert_eq!(
        names.len(),
        MAX_USER_PROFILES,
        "every surviving name is unique"
    );
}

#[test]
fn restoring_the_envelope_does_not_apply_profiles_or_dirty_the_state() {
    let stored = builtins()
        .into_iter()
        .find(|profile| profile.name == "High contrast")
        .expect("the named builtin exists");
    let settings = AppSettings {
        appearance_profiles: vec![stored.clone()],
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.appearance_profiles,
        vec![stored.clone()],
        "the envelope keeps the user profiles as records"
    );

    // Loading the envelope must not apply the stored profile or request a
    // scene rebuild.
    let app = VfApp::new_with_storage(Some(&storage));
    let baseline = VfApp::new_with_storage(None);
    assert_eq!(
        app.state.viewport_style, baseline.state.viewport_style,
        "a stored profile is never applied at startup"
    );
    assert_eq!(app.state.layer_styles, baseline.state.layer_styles);
    assert_eq!(app.state.colormap, baseline.state.colormap);
    assert_eq!(app.state.color_ramp, baseline.state.color_ramp);
    assert_eq!(app.state.color_mode, baseline.state.color_mode);
    assert_eq!(app.state.color_quantity, baseline.state.color_quantity);
    assert_eq!(
        app.state.dirty, baseline.state.dirty,
        "restoring profiles adds no scene rebuild"
    );
}

#[test]
fn new_with_storage_loads_the_user_profile_library_without_applying_it() {
    let stored = appearance_profile("Saved look");
    let settings = AppSettings {
        appearance_profiles: vec![stored.clone()],
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        app.profiles.user(),
        [stored],
        "the loaded library is kept unchanged"
    );
    assert_eq!(
        app.state.viewport_style,
        ViewportStyle::default(),
        "loading a profile does not apply it"
    );
}

#[test]
fn user_profile_library_round_trips_through_an_app_save() {
    let stored = appearance_profile("Saved look");
    let mut app = VfApp::new();
    app.profiles = ProfileLibrary::new(vec![stored.clone()]).expect("the profile is valid");
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);

    let loaded = load(Some(&storage)).sanitized();
    assert_eq!(
        loaded.appearance_profiles,
        vec![stored],
        "the save path writes the live library"
    );
}

#[test]
fn app_save_captures_the_viewport_style() {
    let mut app = VfApp::new();
    app.state
        .commit_viewport_style(ViewportStyle::high_contrast())
        .expect("the named look is valid");
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);

    assert_eq!(
        load(Some(&storage)).viewport_style,
        ViewportStyle::high_contrast(),
        "the save path captures the committed record"
    );
}

#[test]
fn legacy_records_default_the_color_mapping_fields() {
    let loaded = load_legacy(false);
    assert_eq!(loaded.view.color_ramp, None);
    assert_eq!(loaded.view.color_mode, ScaleMode::Linear);
    assert_eq!(loaded.view.color_quantity, ColorQuantity::Magnitude);

    let mut state = State::new(false);
    loaded.view.restore(&mut state, &mut Controls::new());
    assert!(state.color_ramp.is_none());
    assert_eq!(state.color_mode, ScaleMode::Linear);
    assert_eq!(state.color_quantity, ColorQuantity::Magnitude);
}

#[test]
fn color_mapping_fields_round_trip_through_storage() {
    let mut state = State::new(false);
    assert!(state.set_color_ramp(Some(ColorRamp::default())));
    assert!(state.set_color_mode(ScaleMode::SignedLog { half_width: 0.5 }));
    assert!(state.set_color_quantity(ColorQuantity::ComponentY));

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: ViewRecord::capture(&state),
            ..Default::default()
        },
    );
    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.color_ramp, Some(ColorRamp::default()));
    assert_eq!(
        loaded.view.color_mode,
        ScaleMode::SignedLog { half_width: 0.5 }
    );
    assert_eq!(loaded.view.color_quantity, ColorQuantity::ComponentY);

    let mut restored = State::new(false);
    loaded.view.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.color_ramp, Some(ColorRamp::default()));
    assert_eq!(
        restored.color_mode,
        ScaleMode::SignedLog { half_width: 0.5 }
    );
    assert_eq!(restored.color_quantity, ColorQuantity::ComponentY);
}

#[test]
fn restoring_direction_colouring_into_3d_falls_back_to_magnitude() {
    let record = ViewRecord {
        formula: "(x, y, z)".to_owned(),
        mode_3d: true,
        last_vector_mode_3d: Some(true),
        color_quantity: ColorQuantity::Direction2d,
        ..ViewRecord::default()
    };

    let mut state = State::new(true);
    record.restore(&mut state, &mut Controls::new());

    assert!(state.is_3d(), "the record restores the 3D view");
    assert_eq!(
        state.color_quantity,
        ColorQuantity::Magnitude,
        "direction colouring cannot survive into a 3D view"
    );
}

#[test]
fn corrupt_color_mapping_fields_fall_back_without_losing_the_record() {
    let invalid_ramp = ColorRamp {
        stops: Vec::new(),
        reversed: false,
        kind: RampKind::Sequential,
    };
    let record = ViewRecord {
        formula: "(x, y)".to_owned(),
        color_ramp: Some(invalid_ramp),
        color_mode: ScaleMode::SignedLog { half_width: -1.0 },
        color_quantity: ColorQuantity::ComponentY,
        ..ViewRecord::default()
    };

    let mut state = State::new(false);
    record.restore(&mut state, &mut Controls::new());

    assert!(state.color_ramp.is_none(), "an invalid ramp disappears");
    assert_eq!(
        state.color_mode,
        ScaleMode::Linear,
        "an invalid mode resets"
    );
    assert_eq!(
        state.color_quantity,
        ColorQuantity::ComponentY,
        "the rest of the record survives"
    );
    assert_eq!(state.formula, "(x, y)");
}

// Issue #90 slice B: the library's group expansion persists with the other UI
// preferences, defaults to all-expanded on old/corrupt records, and startup
// applies it without reporting a layout change.

#[test]
fn preset_group_expansion_round_trips_through_storage() {
    let expected = [true, false, true, false];
    let settings = AppSettings {
        preset_ui: PresetUiRecord { expanded: expected },
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let loaded = load(Some(&storage));
    assert_eq!(loaded.preset_ui.expanded, expected);

    let app = VfApp::new_with_storage(Some(&storage));
    assert!(app.presets.expanded(PresetGroup::Classic));
    assert!(!app.presets.expanded(PresetGroup::Physics));
    assert!(app.presets.expanded(PresetGroup::Sources));
    assert!(!app.presets.expanded(PresetGroup::Gallery));
}

#[test]
fn old_settings_default_every_preset_group_to_expanded() {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &legacy_settings(false));

    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.preset_ui.expanded, [true; 4],
        "older envelopes have no preset_ui record"
    );
}

#[test]
fn corrupt_preset_ui_records_fall_back_to_all_expanded() {
    #[derive(serde::Serialize)]
    struct WrongLength {
        expanded: [bool; 2],
    }
    #[derive(serde::Serialize)]
    struct WrongType {
        expanded: String,
    }
    #[derive(serde::Serialize)]
    struct Envelope<T> {
        style: AppStyle,
        preset_ui: T,
    }

    let mut checked = 0;
    macro_rules! check {
        ($corrupt:expr) => {{
            let mut storage = MemoryStorage::default();
            eframe::set_value(&mut storage, eframe::APP_KEY, &$corrupt);
            let loaded = load(Some(&storage));
            assert_eq!(
                loaded.preset_ui.expanded, [true; 4],
                "a corrupt record must not fail the envelope or narrow the panel"
            );
            assert_eq!(loaded.style, AppStyle::default(), "style still loads");
            checked += 1;
        }};
    }
    check!(Envelope {
        style: AppStyle::default(),
        preset_ui: WrongLength {
            expanded: [false, false],
        },
    });
    check!(Envelope {
        style: AppStyle::default(),
        preset_ui: WrongType {
            expanded: "not an array".to_string(),
        },
    });
    assert_eq!(checked, 2);
}

#[test]
fn startup_applies_preset_expansion_without_reporting_a_layout_change() {
    let settings = AppSettings {
        preset_ui: PresetUiRecord {
            expanded: [false, true, false, true],
        },
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let mut app = VfApp::new_with_storage(Some(&storage));
    assert!(!app.presets.expanded(PresetGroup::Classic));
    assert!(app.presets.expanded(PresetGroup::Physics));
    assert!(!app.presets.expanded(PresetGroup::Sources));
    assert!(app.presets.expanded(PresetGroup::Gallery));
    assert!(
        !app.presets.take_layout_changed(),
        "restoring expansion must not look like a user layout edit"
    );
}

#[test]
fn interaction_settings_round_trip_through_the_envelope() {
    let active = custom_interaction();
    let settings = AppSettings {
        interaction: active.clone(),
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.interaction, active,
        "replaced chords and camera feel survive the envelope"
    );
    assert!(loaded.interaction.validate().is_ok());
}

#[test]
fn older_envelopes_without_interaction_fields_load_the_defaults() {
    #[derive(serde::Serialize)]
    struct OlderEnvelope {
        style: AppStyle,
        layout: LayoutState,
        view: ViewRecord,
        parameters: ParameterSet,
        viewport_style: ViewportStyle,
        layer_styles: LayerStyles,
        appearance_profiles: Vec<AppearanceProfile>,
        preset_ui: PresetUiRecord,
    }

    let current = AppSettings::default();
    let older = OlderEnvelope {
        style: current.style.clone(),
        layout: current.layout.clone(),
        view: current.view.clone(),
        parameters: current.parameters.clone(),
        viewport_style: current.viewport_style,
        layer_styles: current.layer_styles,
        appearance_profiles: Vec::new(),
        preset_ui: current.preset_ui.clone(),
    };
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &older);

    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.interaction,
        InteractionProfile::default(),
        "an older envelope loads the documented chords and neutral feel"
    );
    assert!(loaded.interaction_profiles.is_empty());
}

#[test]
fn sanitizing_repairs_an_invalid_active_interaction_record() {
    // A partial table with a duplicate chord: most actions are absent and
    // ToggleControls/TogglePresets share J.
    let bindings: Bindings = serde_json::from_str(
        r#"{
            "toggle_controls": {"key": "J", "modifiers": {"command": false, "shift": false, "alt": false}},
            "toggle_presets": {"key": "J", "modifiers": {"command": false, "shift": false, "alt": false}}
        }"#,
    )
    .expect("the partial table is parseable");
    let interaction = InteractionProfile {
        name: "Active".to_owned(),
        bindings,
        camera_feel: CameraFeel {
            orbit_sensitivity: 99.0,
            zoom_speed: f32::NAN,
            ..CameraFeel::default()
        },
    };
    assert!(
        interaction.validate().is_err(),
        "the fixture must start invalid"
    );

    let repaired = AppSettings {
        interaction,
        ..AppSettings::default()
    }
    .sanitized()
    .interaction;

    assert_eq!(
        repaired.camera_feel.orbit_sensitivity, 4.0,
        "an out-of-range multiplier clamps"
    );
    assert_eq!(
        repaired.camera_feel.zoom_speed, 1.0,
        "a non-finite multiplier resets"
    );
    assert_eq!(
        repaired.bindings.chord(InteractionAction::ToggleControls),
        Some(interaction_chord(eframe::egui::Key::J)),
        "the first owner keeps the custom chord"
    );
    assert_eq!(
        repaired.bindings.chord(InteractionAction::TogglePresets),
        Some(KeyChord {
            key: eframe::egui::Key::P,
            modifiers: ChordModifiers {
                command: true,
                shift: false,
                alt: false,
            },
        }),
        "a duplicate chord returns to the documented default"
    );
    assert!(
        InteractionAction::REMAPPABLE
            .iter()
            .all(|action| repaired.bindings.chord(*action).is_some()),
        "every absent binding is restored"
    );
    assert!(
        repaired.validate().is_ok(),
        "the repaired active record validates"
    );
}

#[test]
fn sanitizing_drops_builtin_duplicate_and_excess_user_interaction_profiles() {
    let builtin_named = InteractionProfile {
        name: DEFAULT_PROFILE_NAME.to_owned(),
        ..InteractionProfile::default()
    };
    let first = interaction_profile_named("  Alpha  ");
    let duplicate = interaction_profile_named("Alpha");
    let mut profiles = vec![builtin_named, first, duplicate];
    for index in 0..MAX_INTERACTION_PROFILES + 2 {
        profiles.push(interaction_profile_named(&format!("Look {index}")));
    }

    let loaded = AppSettings {
        interaction_profiles: profiles,
        ..AppSettings::default()
    }
    .sanitized();

    assert_eq!(
        loaded.interaction_profiles.len(),
        MAX_INTERACTION_PROFILES,
        "the library is truncated after the built-in and duplicate are dropped"
    );
    assert_eq!(
        loaded.interaction_profiles.first().map(|p| p.name.as_str()),
        Some("Alpha"),
        "the first sanitized record is kept"
    );
    assert!(
        loaded
            .interaction_profiles
            .iter()
            .all(|profile| profile.name != DEFAULT_PROFILE_NAME),
        "the built-in Default is never stored"
    );
    let names: std::collections::HashSet<&str> = loaded
        .interaction_profiles
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert_eq!(
        names.len(),
        MAX_INTERACTION_PROFILES,
        "every surviving name is unique"
    );
}

#[test]
fn new_with_storage_restores_interaction_settings_without_dirtying_or_applying() {
    let active = custom_interaction();
    let stored = interaction_profile_named("Saved interaction");
    let settings = AppSettings {
        interaction: active.clone(),
        interaction_profiles: vec![stored.clone()],
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let app = VfApp::new_with_storage(Some(&storage));
    let baseline = VfApp::new_with_storage(None);
    assert_eq!(app.interaction, active, "the active record is restored");
    assert_eq!(
        app.interaction_profiles.user(),
        [stored],
        "the user library is restored"
    );
    assert_eq!(
        app.state.viewport_style, baseline.state.viewport_style,
        "a stored interaction profile is never applied at startup"
    );
    assert_eq!(app.state.layer_styles, baseline.state.layer_styles);
    assert_eq!(app.state.colormap, baseline.state.colormap);
    assert_eq!(app.state.color_ramp, baseline.state.color_ramp);
    assert_eq!(app.state.color_mode, baseline.state.color_mode);
    assert_eq!(app.state.color_quantity, baseline.state.color_quantity);
    assert_eq!(
        app.state.dirty, baseline.state.dirty,
        "restoring interaction settings adds no scene rebuild"
    );
}

#[test]
fn user_interaction_profiles_round_trip_through_an_app_save() {
    let stored = interaction_profile_named("Saved interaction");
    let mut app = VfApp::new();
    app.interaction_profiles =
        InteractionProfileLibrary::new(vec![stored.clone()]).expect("the profile is valid");
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);

    let loaded = load(Some(&storage)).sanitized();
    assert_eq!(
        loaded.interaction_profiles,
        vec![stored],
        "the save path writes the live interaction library"
    );
}

// Issue #87 slice S1: the explicit seed layout is part of the view record.

#[test]
fn view_record_round_trips_the_seed_layout() {
    let mut state = State::new(true);
    let center = state.scene_domain().center();
    let layout = SeedLayout {
        seeds: vec![
            SeedPoint {
                position: center,
                anchor: None,
            },
            SeedPoint {
                position: center + Vec3::new(0.25, -0.5, 0.75),
                anchor: Some(SeedAnchor {
                    axis: PlaneAxis::Z,
                    index: 3,
                    u: 0.25,
                    v: -0.5,
                }),
            },
        ],
    };
    state
        .commit_seed_layout(layout.clone())
        .expect("the layout is inside the scene domain");
    let record = ViewRecord::capture(&state);
    assert_eq!(record.seed_layout, layout, "capture reads the state layout");

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: record,
            ..Default::default()
        },
    );
    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.view.seed_layout, layout,
        "the envelope round-trips seeds"
    );

    let mut restored = State::new(false);
    loaded.view.restore(&mut restored, &mut Controls::new());
    assert_eq!(
        restored.seed_layout, layout,
        "restore assigns the stored layout"
    );
}

#[test]
fn older_records_without_a_seed_layout_load_empty() {
    #[derive(serde::Serialize)]
    struct PreSeedLayoutView {
        formula: String,
    }

    #[derive(serde::Serialize)]
    struct PreSeedLayoutSettings {
        style: AppStyle,
        view: PreSeedLayoutView,
    }

    let mut storage = MemoryStorage::default();
    eframe::set_value(
        &mut storage,
        eframe::APP_KEY,
        &PreSeedLayoutSettings {
            style: AppStyle::default(),
            view: PreSeedLayoutView {
                formula: "(x, y)".to_string(),
            },
        },
    );

    let loaded = load(Some(&storage));
    assert!(
        loaded.view.seed_layout.is_empty(),
        "an older record defaults to the empty layout"
    );
    let mut state = State::new(false);
    loaded.view.restore(&mut state, &mut Controls::new());
    assert!(
        state.seed_layout.is_empty(),
        "restore keeps the automatic grid"
    );
}

#[test]
fn corrupt_stored_seed_layout_is_sanitised_on_restore() {
    let source = State::new(false);
    let domain = source.scene_domain();
    let center = domain.center();
    let valid_free = SeedPoint {
        position: center,
        anchor: None,
    };
    let valid_anchored = SeedPoint {
        // The default view is 2D, so its scene domain collapses z to the
        // domain midpoint; keep the planted position on that plane.
        position: center + Vec3::new(0.25, -0.5, 0.0),
        anchor: Some(SeedAnchor {
            axis: PlaneAxis::Z,
            index: 1,
            u: 0.25,
            v: -0.5,
        }),
    };
    let record = ViewRecord {
        seed_layout: SeedLayout {
            seeds: vec![
                SeedPoint {
                    position: Vec3::new(f32::NAN, center.y, center.z),
                    anchor: None,
                },
                valid_free,
                SeedPoint {
                    position: Vec3::new(1.0e9, center.y, center.z),
                    anchor: None,
                },
                SeedPoint {
                    position: center,
                    anchor: Some(SeedAnchor {
                        axis: PlaneAxis::X,
                        index: 0,
                        u: f32::INFINITY,
                        v: 0.0,
                    }),
                },
                // Inside the 3D domain box but off the collapsed 2D scene
                // plane: the scene domain is the authority, so this must drop.
                SeedPoint {
                    position: center + Vec3::new(-0.4, 0.3, 0.75),
                    anchor: None,
                },
                valid_anchored,
            ],
        },
        ..ViewRecord::capture(&source)
    };
    assert!(
        record.seed_layout.validate(&domain).is_err(),
        "the stored fixture is genuinely corrupt"
    );

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: record,
            ..Default::default()
        },
    );
    let loaded = load(Some(&storage));
    assert!(
        loaded.view.seed_layout.validate(&domain).is_err(),
        "load does not repair the layout; restore owns sanitising"
    );

    let mut state = State::new(false);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);

    assert_eq!(
        state.seed_layout.seeds,
        vec![valid_free, valid_anchored],
        "invalid seeds drop and the valid seeds survive in insertion order"
    );
    state
        .seed_layout
        .validate(&state.scene_domain())
        .expect("the sanitised layout re-validates against the restored domain");
    // This repo has no log-capture helper, so the assertion pins the
    // contract-visible sanitised outcome. The restore does exercise the
    // dropped-count warning; its text is not asserted.
}

#[test]
fn restore_drops_seed_layout_seeds_outside_the_scene_domain_in_2d() {
    let source = State::new(false);
    let scene_domain = source.scene_domain();
    let center = scene_domain.center();
    let valid = SeedPoint {
        position: center + Vec3::new(0.25, -0.5, 0.0),
        anchor: None,
    };
    // The 2D scene domain collapses z to the domain midpoint; this seed is
    // inside the 3D domain box but off the visible plane.
    let off_plane = SeedPoint {
        position: center + Vec3::new(-0.25, 0.5, 0.75),
        anchor: None,
    };
    let settings = AppSettings {
        view: ViewRecord {
            seed_layout: SeedLayout {
                seeds: vec![off_plane, valid],
            },
            ..ViewRecord::capture(&source)
        },
        ..Default::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));

    let mut state = State::new(false);
    let mut controls = Controls::new();
    loaded.view.restore(&mut state, &mut controls);

    assert_eq!(
        state.seed_layout.seeds,
        vec![valid],
        "restore drops the off-plane seed and keeps the valid one"
    );
    state
        .seed_layout
        .validate(&state.scene_domain())
        .expect("the restored layout re-validates against the 2D scene domain");
    // `ViewRecord::restore` requests its documented single rebuild; consuming
    // it must leave a clean scene with no seed-driven second rebuild.
    assert!(state.dirty, "restore asks for its one scene rebuild");
    assert!(state.snapshot_if_dirty().is_some());
    assert!(!state.dirty, "the scene is clean after the startup rebuild");
    assert!(
        state.snapshot_if_dirty().is_none(),
        "the restored seed layout does not re-dirty the scene"
    );
}

#[test]
fn startup_seed_layout_restore_is_non_dirtying() {
    let source = State::new(false);
    let center = source.scene_domain().center();
    let stored = SeedLayout {
        seeds: vec![
            SeedPoint {
                position: center,
                anchor: None,
            },
            // One invalid seed proves the installed layout is the sanitised
            // record, not merely an unchecked copy.
            SeedPoint {
                position: Vec3::new(1.0e9, center.y, center.z),
                anchor: None,
            },
        ],
    };
    let settings = AppSettings {
        view: ViewRecord {
            seed_layout: stored,
            ..ViewRecord::capture(&source)
        },
        ..Default::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));

    // Pin the startup seam: restoring a layout assigns it directly instead of
    // going through `commit_seed_layout`, so a non-empty sanitised layout does
    // not request a scene rebuild on its own.
    let mut state = State::new(false);
    state.dirty = false;
    state.seed_layout = loaded
        .view
        .seed_layout
        .clone()
        .sanitized(&state.scene_domain());
    assert_eq!(
        state.seed_layout.seeds,
        vec![SeedPoint {
            position: center,
            anchor: None
        }],
        "the startup restore installs the sanitised non-empty layout"
    );
    assert!(
        !state.dirty,
        "assigning the restored seed layout leaves dirty alone"
    );

    // The real constructor installs the same layout. The enclosing view
    // restore requests its documented single rebuild; consuming it leaves the
    // scene clean and the seed layout must not request a second one.
    let mut app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(app.state.seed_layout, state.seed_layout);
    assert!(
        app.state.snapshot_if_dirty().is_some(),
        "the view restore asks for its one scene rebuild"
    );
    assert!(!app.state.dirty, "the startup rebuild leaves a clean scene");
    assert!(
        app.state.snapshot_if_dirty().is_none(),
        "the restored seed layout does not keep redirtying the scene"
    );
}

// Issue #87 slice S3a: the named seed-layout library lives in the envelope.

fn seed_layout_profile(name: &str, seeds: Vec<SeedPoint>) -> SeedLayoutProfile {
    SeedLayoutProfile {
        name: name.to_owned(),
        seeds: SeedLayout { seeds },
    }
}

fn seed_layout_named(name: &str) -> SeedLayoutProfile {
    seed_layout_profile(
        name,
        vec![SeedPoint {
            position: Vec3::ZERO,
            anchor: None,
        }],
    )
}

#[test]
fn seed_layout_profiles_round_trip_through_the_envelope() {
    let profiles = vec![
        seed_layout_named("Alpha"),
        seed_layout_profile(
            "Beta",
            vec![
                SeedPoint {
                    position: Vec3::new(0.25, -0.5, 0.0),
                    anchor: Some(SeedAnchor {
                        axis: PlaneAxis::Z,
                        index: 2,
                        u: 0.25,
                        v: -0.5,
                    }),
                },
                SeedPoint {
                    position: Vec3::new(-0.25, 0.5, 0.0),
                    anchor: None,
                },
            ],
        ),
    ];
    let settings = AppSettings {
        seed_layouts: profiles.clone(),
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.seed_layouts, profiles,
        "the envelope round-trips the user seed layouts"
    );
    assert_eq!(
        SeedLayoutLibrary::new(
            loaded.seed_layouts.clone(),
            &State::new(false).scene_domain()
        )
        .expect("the loaded profiles validate")
        .user(),
        profiles.as_slice(),
        "the loaded profiles build the library unchanged"
    );
}

#[test]
fn startup_restore_of_seed_layouts_is_non_applying_and_non_dirtying() {
    let profiles = vec![
        seed_layout_profile(
            "Alpha",
            vec![SeedPoint {
                position: Vec3::new(0.25, -0.5, 0.0),
                anchor: None,
            }],
        ),
        seed_layout_named("Beta"),
    ];
    let settings = AppSettings {
        seed_layouts: profiles.clone(),
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);

    let mut app = VfApp::new_with_storage(Some(&storage));
    assert_eq!(
        app.seed_layouts.user(),
        profiles.as_slice(),
        "startup builds the library from the envelope"
    );
    assert!(
        app.state.seed_layout.is_empty(),
        "restoring the library never applies a layout"
    );
    assert_eq!(
        app.state.seed_layout,
        SeedLayout::default(),
        "the current explicit seeds stay untouched"
    );
    assert!(
        app.state.snapshot_if_dirty().is_some(),
        "the view restore asks for its one scene rebuild"
    );
    assert!(!app.state.dirty, "the startup rebuild leaves a clean scene");
    assert!(
        app.state.snapshot_if_dirty().is_none(),
        "restoring the seed-layout library never redirties the scene"
    );
}

#[test]
fn corrupt_seed_layout_profiles_are_sanitised_on_load() {
    let profiles = vec![
        // Empty after trimming: dropped rather than replaced.
        seed_layout_profile("   ", vec![]),
        // The built-in name is never stored.
        seed_layout_named(DEFAULT_SEED_LAYOUT_NAME),
        seed_layout_profile(
            "  Good  ",
            vec![
                SeedPoint {
                    position: Vec3::ZERO,
                    anchor: None,
                },
                // Non-finite and out-of-domain seeds drop.
                SeedPoint {
                    position: Vec3::new(f32::NAN, 0.0, 0.0),
                    anchor: None,
                },
                SeedPoint {
                    position: Vec3::new(99.0, 0.0, 0.0),
                    anchor: None,
                },
                // Off the collapsed 2D scene plane: the stored view domain is
                // the authority, so this is dropped too.
                SeedPoint {
                    position: Vec3::new(0.0, 0.0, 0.75),
                    anchor: None,
                },
            ],
        ),
        // Duplicate after sanitising: the first occurrence wins.
        seed_layout_named("Dup"),
        seed_layout_named(" Dup "),
    ];
    let settings = AppSettings {
        seed_layouts: profiles,
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));

    assert_eq!(
        loaded
            .seed_layouts
            .iter()
            .map(|layout| layout.name.as_str())
            .collect::<Vec<_>>(),
        ["Good", "Dup"],
        "corrupt names are repaired or dropped and the first duplicate wins"
    );
    assert_eq!(
        loaded.seed_layouts[0].seeds.seeds,
        vec![SeedPoint {
            position: Vec3::ZERO,
            anchor: None,
        }],
        "invalid seed positions are dropped against the stored view domain"
    );
    for layout in &loaded.seed_layouts {
        layout
            .validate(&State::new(false).scene_domain())
            .expect("every loaded profile validates");
    }

    // Excess entries are truncated to the cap, keeping the first 16.
    let too_many: Vec<_> = (0..MAX_USER_SEED_LAYOUTS + 4)
        .map(|index| seed_layout_named(&format!("Layout {index}")))
        .collect();
    let settings = AppSettings {
        seed_layouts: too_many,
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.seed_layouts.len(),
        MAX_USER_SEED_LAYOUTS,
        "excess stored layouts are truncated"
    );
    assert_eq!(loaded.seed_layouts[0].name, "Layout 0");
    assert_eq!(
        loaded.seed_layouts[MAX_USER_SEED_LAYOUTS - 1].name,
        format!("Layout {}", MAX_USER_SEED_LAYOUTS - 1)
    );
}

// Issue #92 slice A: scalar probes are durable document state; sampled values
// and line points are runtime-only cache.

#[test]
fn probes_round_trip_through_the_envelope_and_legacy_records_default_to_none() {
    let anchor = SeedAnchor {
        axis: PlaneAxis::Z,
        index: 2,
        u: -0.25,
        v: 0.5,
    };
    let mut point =
        PointProbe::new(Vec3::new(0.25, -0.5, 0.0), Some(anchor)).expect("valid point probe");
    point.value = Some(9.5);
    let mut line = LineProbe::new(
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        None,
        vec![
            ScalarSample {
                position: Vec3::new(-1.0, 0.0, 0.0),
                value: Some(8.5),
            },
            ScalarSample {
                position: Vec3::new(1.0, 0.0, 0.0),
                value: Some(7.5),
            },
        ],
    )
    .expect("valid line probe");
    line.samples[1].value = None;

    let mut state = State::new(false);
    state.probes.push_point(point.clone()).expect("room");
    state.probes.push_line(line.clone()).expect("room");

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: ViewRecord::capture(&state),
            ..Default::default()
        },
    );
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(raw.contains("probes"), "the probe record is stored:\n{raw}");
    for cached in ["9.5", "8.5", "7.5"] {
        assert!(
            !raw.contains(cached),
            "sampled value {cached} must not be persisted:\n{raw}"
        );
    }

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.probes.points.len(), 1);
    assert_eq!(loaded.view.probes.lines.len(), 1);
    assert_eq!(loaded.view.probes.points[0].position, point.position);
    assert_eq!(loaded.view.probes.points[0].anchor, Some(anchor));
    assert_eq!(loaded.view.probes.points[0].value, None);
    assert_eq!(loaded.view.probes.lines[0].start, line.start);
    assert_eq!(loaded.view.probes.lines[0].end, line.end);
    assert!(
        loaded.view.probes.lines[0].samples.is_empty(),
        "line samples are runtime-only"
    );

    let mut restored = State::new(false);
    loaded.view.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.probes.points, loaded.view.probes.points);
    assert_eq!(restored.probes.lines, loaded.view.probes.lines);

    // A legacy envelope without the field restores no probes.
    let legacy = load_legacy(false);
    assert!(legacy.view.probes.is_empty());
    let mut legacy_state = State::new(false);
    legacy.view.restore(&mut legacy_state, &mut Controls::new());
    assert!(legacy_state.probes.is_empty());
}

// Issue #93 slice A: comparison series and the gradient request are durable
// document state; invalid stored values are repaired/dropped on restore.

#[test]
fn comparison_series_and_gradient_round_trip_through_the_envelope() {
    let set = ComparisonSet::from_series(vec![ComparisonSeries::new(
        SeriesId(5),
        "potential".to_owned(),
        "x*x + y*y".to_owned(),
        SeriesStyle {
            color: Rgb([12, 200, 90]),
            dashed: true,
        },
    )]);
    let gradient = GradientSpec {
        role: SeriesRole::ElectricPotential,
        expr: "x*x + y*y".to_owned(),
        frame: PlaneFrame {
            axis: PlaneAxis::Z,
            origin: Vec3::new(0.25, -0.5, 1.0),
            right: Vec3::X,
            up: Vec3::Y,
            normal: Vec3::Z,
            half_u: 1.5,
            half_v: 2.5,
        },
        step: 1e-2,
        max_error: 0.25,
    };

    let mut state = State::new(false);
    state.comparison_series = set.clone();
    state.gradient = Some(gradient.clone());

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: ViewRecord::capture(&state),
            ..Default::default()
        },
    );
    let raw = storage
        .get_string(eframe::APP_KEY)
        .expect("the envelope is saved");
    assert!(
        raw.contains("comparison_series"),
        "the series record is stored:\n{raw}"
    );
    assert!(raw.contains("gradient"), "the gradient record is stored");

    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.comparison_series, set);
    assert_eq!(loaded.view.gradient, Some(gradient.clone()));

    let mut restored = State::new(false);
    loaded.view.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.comparison_series, set);
    assert_eq!(restored.gradient, Some(gradient));

    // A legacy envelope without the fields restores no series and no gradient.
    let legacy = load_legacy(false);
    assert!(legacy.view.comparison_series.is_empty());
    assert!(legacy.view.gradient.is_none());
    let mut legacy_state = State::new(false);
    legacy.view.restore(&mut legacy_state, &mut Controls::new());
    assert!(legacy_state.comparison_series.is_empty());
    assert!(legacy_state.gradient.is_none());
}

#[test]
fn corrupt_series_and_gradient_are_sanitized_on_restore() {
    let record = ViewRecord {
        comparison_series: ComparisonSet::from_series(vec![
            ComparisonSeries::new(
                SeriesId(1),
                String::new(),
                "x".to_owned(),
                SeriesStyle::default(),
            ),
            ComparisonSeries::new(
                SeriesId(1),
                "duplicate".to_owned(),
                "x".to_owned(),
                SeriesStyle::default(),
            ),
            ComparisonSeries::new(
                SeriesId(2),
                "x".repeat(MAX_SERIES_LABEL_BYTES + 4),
                "x".to_owned(),
                SeriesStyle::default(),
            ),
        ]),
        gradient: Some(GradientSpec {
            role: SeriesRole::Scalar,
            expr: "x*x".to_owned(),
            frame: PlaneFrame {
                axis: PlaneAxis::Z,
                origin: Vec3::ZERO,
                right: Vec3::X,
                up: Vec3::Y,
                normal: Vec3::Z,
                half_u: 1.0,
                half_v: 1.0,
            },
            step: 0.0,
            max_error: 0.1,
        }),
        ..ViewRecord::default()
    };

    let mut state = State::new(false);
    record.restore(&mut state, &mut Controls::new());
    assert!(state.comparison_series.len() <= 2);
    assert_eq!(state.comparison_series.validate(), Ok(()));
    assert!(
        state
            .comparison_series
            .iter()
            .all(|series| !series.label.is_empty() && series.label.len() <= MAX_SERIES_LABEL_BYTES),
        "invalid labels are repaired"
    );
    assert!(
        state.gradient.is_none(),
        "an invalid stored gradient is dropped instead of sampled"
    );
}

fn probe_surface(radius: f32, segments: u32) -> GaussianSurface {
    GaussianSurface {
        kind: SurfaceKind::Sphere,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0 * radius),
        segments,
    }
}

fn probe_loop(radius: f32, segments: u32) -> CirculationLoop {
    CirculationLoop {
        kind: LoopKind::Circle,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0 * radius),
        segments,
    }
}

#[test]
fn probe_surfaces_and_loops_round_trip_through_the_envelope_and_scene_document() {
    let mut state = State::new(true);
    let surface = probe_surface(1.0, 16);
    let loop_ = probe_loop(0.75, 24);
    state.add_probe_surface(surface).expect("valid surface");
    state.add_probe_loop(loop_).expect("valid loop");

    let record = ViewRecord::capture(&state);
    assert_eq!(
        record
            .probe_surfaces
            .iter()
            .filter_map(ProbeSurfaceRecord::to_surface)
            .collect::<Vec<_>>(),
        vec![surface],
        "capture flattens the stored surfaces"
    );
    assert_eq!(
        record
            .probe_loops
            .iter()
            .filter_map(ProbeLoopRecord::to_loop)
            .collect::<Vec<_>>(),
        vec![loop_],
        "capture flattens the stored loops"
    );

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: record.clone(),
            ..Default::default()
        },
    );
    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.view.probe_surfaces, record.probe_surfaces,
        "the envelope round-trips surfaces"
    );
    assert_eq!(
        loaded.view.probe_loops, record.probe_loops,
        "the envelope round-trips loops"
    );

    let mut restored = State::new(false);
    loaded.view.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.probe_surfaces(), &[surface]);
    assert_eq!(restored.probe_loops(), &[loop_]);

    let document = SceneDocument::capture(
        "probe scene",
        record.clone(),
        ParameterSet::default(),
        vector_field_explorer::scene_document::SceneCamera {
            position: [0.0, 0.0],
            scale: 1.0,
            yaw: 0.0,
            pitch: 0.0,
            distance: 4.0,
            target: [0.0, 0.0, 0.0],
            fov_y: 1.0,
            ortho_zoom: 1.0,
        },
    );
    let json = document.to_json().expect("the scene serializes");
    let imported = SceneDocument::from_json(&json).expect("the scene parses");
    assert_eq!(
        imported.view.probe_surfaces, record.probe_surfaces,
        "scene documents carry the surfaces"
    );
    assert_eq!(
        imported.view.probe_loops, record.probe_loops,
        "scene documents carry the loops"
    );
}

#[test]
fn restoring_invalid_probes_drops_them_without_dirtying() {
    let source = State::new(false);
    let mut surfaces = Vec::new();
    // Two invalid geometries, then more valid ones than the cap allows.
    surfaces.push(ProbeSurfaceRecord {
        kind: "sphere".to_string(),
        center: [0.0, 0.0, 0.0],
        axis: [0.0, 0.0, 1.0],
        size: [2.0, 2.0, 2.0],
        segments: 2,
    });
    surfaces.push(ProbeSurfaceRecord {
        kind: "not-a-kind".to_string(),
        center: [0.0, 0.0, 0.0],
        axis: [0.0, 0.0, 1.0],
        size: [2.0, 2.0, 2.0],
        segments: 16,
    });
    for _ in 0..MAX_SURFACES + 2 {
        let surface = probe_surface(1.0, 16);
        surfaces.push(ProbeSurfaceRecord::from_surface(&surface));
    }
    let mut loops = vec![ProbeLoopRecord {
        kind: "circle".to_string(),
        center: [0.0, 0.0, 0.0],
        axis: [0.0, 0.0, 1.0],
        size: [2.0, 2.0, 2.0],
        segments: 1,
    }];
    for _ in 0..MAX_LOOPS + 1 {
        let loop_ = probe_loop(1.0, 16);
        loops.push(ProbeLoopRecord::from_loop(&loop_));
    }

    let clean = ViewRecord::capture(&source);
    let record = ViewRecord {
        probe_surfaces: surfaces,
        probe_loops: loops,
        ..clean.clone()
    };
    // The surrounding restore may dirty for unrelated records; the probe
    // assignment itself must add nothing beyond a clean record's restore.
    let mut baseline = State::new(false);
    baseline.dirty = false;
    clean.restore(&mut baseline, &mut Controls::new());
    let baseline_dirty = baseline.dirty;

    let mut state = State::new(false);
    let mut controls = Controls::new();
    state.dirty = false;
    record.restore(&mut state, &mut controls);

    assert_eq!(
        state.probe_surfaces().len(),
        MAX_SURFACES,
        "invalid surfaces drop and the rest truncate to the cap"
    );
    assert_eq!(
        state.probe_loops().len(),
        MAX_LOOPS,
        "invalid loops drop and the rest truncate to the cap"
    );
    assert!(
        state
            .probe_surfaces()
            .iter()
            .all(|surface| surface.validate().is_ok()),
        "only validated surfaces survive"
    );
    assert!(
        state
            .probe_loops()
            .iter()
            .all(|loop_| loop_.validate().is_ok()),
        "only validated loops survive"
    );
    assert_eq!(
        state.dirty, baseline_dirty,
        "a sanitised probe restore adds no dirtying beyond the enclosing restore"
    );
}

fn magnetic_source_scene() -> SourceScene {
    let mut scene = SourceScene::default();
    scene
        .add(Source {
            center: DVec3::ZERO,
            axis: DVec3::Z,
            kind: SourceKind::CurrentLoop {
                current: 1.0,
                radius: 0.5,
            },
        })
        .expect("the loop source is valid");
    scene.set_display(SourceDisplay::Magnetic);
    scene
}

fn scene_camera() -> vector_field_explorer::scene_document::SceneCamera {
    vector_field_explorer::scene_document::SceneCamera {
        position: [0.0, 0.0],
        scale: 1.0,
        yaw: 0.0,
        pitch: 0.0,
        distance: 4.0,
        target: [0.0, 0.0, 0.0],
        fov_y: 1.0,
        ortho_zoom: 1.0,
    }
}

#[test]
fn legacy_envelope_without_probe_family_restores_electric() {
    let loaded = load_legacy(false);
    assert!(
        loaded.view.probe_family.is_empty(),
        "the legacy envelope carries no probe family key"
    );
    let mut state = State::new(false);
    state.set_probe_family(FieldFamily::Magnetic);
    loaded.view.restore(&mut state, &mut Controls::new());
    assert_eq!(
        state.probe_family(),
        FieldFamily::Electric,
        "a missing family falls back to electric"
    );
}

#[test]
fn magnetic_probe_family_round_trips_and_keeps_magnetic_reports_working() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state.set_source_scene(magnetic_source_scene());
    state
        .add_probe_surface(probe_surface(1.0, 32))
        .expect("valid surface");
    assert!(state.set_probe_family(FieldFamily::Magnetic));

    let record = ViewRecord::capture(&state);
    assert_eq!(record.probe_family, "magnetic");

    let mut storage = MemoryStorage::default();
    save(
        &mut storage,
        &AppSettings {
            view: record.clone(),
            ..Default::default()
        },
    );
    let loaded = load(Some(&storage));
    assert_eq!(loaded.view.probe_family, "magnetic");

    let mut restored = State::new(false);
    loaded.view.restore(&mut restored, &mut Controls::new());
    assert_eq!(restored.probe_family(), FieldFamily::Magnetic);
    restored.refresh_probe_reports(true);
    let report = restored
        .probe_surface_report(0)
        .expect("a magnetic restore still builds a report");
    assert_eq!(report.family, FieldFamily::Magnetic);
    assert!(
        matches!(report.status, IntegralStatus::Estimate { .. }),
        "the magnetic report resolves in a magnetic mode: {:?}",
        report.status
    );

    let document = SceneDocument::capture(
        "magnetic probes",
        record,
        ParameterSet::default(),
        scene_camera(),
    );
    let json = document.to_json().expect("the scene serializes");
    let imported = SceneDocument::from_json(&json).expect("the scene parses");
    assert_eq!(imported.view.probe_family, "magnetic");
    let mut from_scene = State::new(false);
    imported.view.restore(&mut from_scene, &mut Controls::new());
    assert_eq!(from_scene.probe_family(), FieldFamily::Magnetic);
}

#[test]
fn corrupt_or_unavailable_probe_family_sanitises_to_electric_without_dirtying() {
    let mut source = State::new(true);
    source.set_field_mode(FieldMode::Sources);
    source.set_source_scene(magnetic_source_scene());
    let clean = ViewRecord::capture(&source);

    let mut baseline = State::new(false);
    baseline.dirty = false;
    clean.clone().restore(&mut baseline, &mut Controls::new());
    let baseline_dirty = baseline.dirty;

    let corrupt = ViewRecord {
        probe_family: "sideways".to_string(),
        ..clean.clone()
    };
    let mut state = State::new(false);
    state.dirty = false;
    corrupt.restore(&mut state, &mut Controls::new());
    assert_eq!(
        state.probe_family(),
        FieldFamily::Electric,
        "corrupt text falls back to electric"
    );
    assert_eq!(
        state.dirty, baseline_dirty,
        "sanitising the family adds no dirtying beyond the enclosing restore"
    );

    let unavailable = ViewRecord {
        probe_family: "magnetic".to_string(),
        field_mode: FieldMode::Induction,
        ..clean
    };
    let mut state = State::new(false);
    state.dirty = false;
    unavailable.restore(&mut state, &mut Controls::new());
    assert_eq!(
        state.probe_family(),
        FieldFamily::Electric,
        "a family the restored mode cannot supply falls back to electric"
    );
    assert_eq!(
        state.dirty, baseline_dirty,
        "an unavailable family adds no dirtying beyond the enclosing restore"
    );
}

fn density_support() -> DensitySupport {
    DensitySupport {
        kind: SupportKind::Box,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat(2.0),
    }
}

fn density_spec(name: &str, family: DensityFamily, formula: &str) -> DensitySpec {
    DensitySpec {
        name: name.to_string(),
        family,
        support: density_support(),
        formula: formula.to_string(),
        samples: 8,
        softening: 0.25,
        time_law: TimeLaw::Steady,
        charge_formula: None,
    }
}

fn volume_density(name: &str, formula: &str) -> DensitySpec {
    density_spec(name, DensityFamily::VolumeCharge, formula)
}

#[test]
fn missing_densities_load_as_an_empty_list_without_dirtying() {
    let source = State::new(true);
    let clean = ViewRecord::capture(&source);
    let mut value = serde_json::to_value(&clean).expect("a view record serializes");
    value
        .as_object_mut()
        .expect("the view record is an object")
        .remove("densities");
    let legacy: ViewRecord = serde_json::from_value(value).expect("a legacy record parses");
    assert!(
        legacy.densities.is_empty(),
        "a missing key loads the empty document list"
    );

    let mut baseline = State::new(true);
    baseline.dirty = false;
    clean.restore(&mut baseline, &mut Controls::new());

    let mut state = State::new(true);
    state.dirty = false;
    legacy.restore(&mut state, &mut Controls::new());
    assert!(state.densities().is_empty());
    assert_eq!(
        state.dirty, baseline.dirty,
        "a missing density list adds no dirtying beyond the enclosing restore"
    );
}

#[test]
fn density_round_trip_preserves_order_and_fields() {
    let specs = vec![
        DensitySpec {
            name: "shell".to_string(),
            family: DensityFamily::SurfaceCharge,
            support: DensitySupport {
                kind: SupportKind::Sphere,
                center: Vec3::new(0.25, -0.5, 1.0),
                axis: Vec3::Y,
                size: Vec3::new(2.5, 2.5, 2.5),
            },
            formula: "x + 2*y".to_string(),
            samples: 11,
            softening: 0.125,
            time_law: TimeLaw::Steady,
            charge_formula: None,
        },
        density_spec("wire", DensityFamily::SteadyCurrent, "0,0,1"),
    ];
    let mut state = State::new(true);
    state
        .commit_densities(specs.clone())
        .expect("the fixture list is valid");

    let records: Vec<DensitySpecRecord> = specs.iter().map(DensitySpecRecord::from_spec).collect();
    let record = ViewRecord::capture(&state);
    assert_eq!(record.densities, records, "capture keeps list order");

    let raw = ron_string(&record);
    let restored: ViewRecord = from_ron(&raw);
    assert_eq!(
        restored.densities, records,
        "the storage codec preserves every field"
    );

    let mut state = State::new(true);
    restored.restore(&mut state, &mut Controls::new());
    assert_eq!(state.densities(), specs.as_slice());
}

#[test]
fn restore_drops_invalid_densities_and_keeps_the_rest_in_order() {
    let valid_a = volume_density("alpha", "1");
    let invalid_geometry = DensitySpec {
        support: DensitySupport {
            kind: SupportKind::Box,
            center: Vec3::ZERO,
            axis: Vec3::Z,
            size: Vec3::new(0.0, 2.0, 2.0),
        },
        ..volume_density("broken", "1")
    };
    let invalid_formula = volume_density("broken formula", "(");
    let valid_b = density_spec("current", DensityFamily::SteadyCurrent, "0,0,1");

    let clean = ViewRecord::capture(&State::new(true));
    let record = ViewRecord {
        densities: [
            valid_a.clone(),
            invalid_geometry,
            invalid_formula,
            valid_b.clone(),
        ]
        .iter()
        .map(DensitySpecRecord::from_spec)
        .collect(),
        ..clean.clone()
    };

    let mut baseline = State::new(true);
    baseline.dirty = false;
    clean.restore(&mut baseline, &mut Controls::new());

    let mut state = State::new(true);
    state.dirty = false;
    record.restore(&mut state, &mut Controls::new());

    assert_eq!(
        state.densities(),
        [valid_a, valid_b].as_slice(),
        "invalid entries drop and the survivors keep their order"
    );
    assert_eq!(
        state.dirty, baseline.dirty,
        "a sanitised density restore adds no dirtying beyond the enclosing restore"
    );
}

#[test]
fn restore_truncates_densities_to_the_document_cap() {
    let clean = ViewRecord::capture(&State::new(true));
    let densities: Vec<DensitySpecRecord> = (0..MAX_DENSITIES + 2)
        .map(|index| {
            DensitySpecRecord::from_spec(&volume_density(&format!("density {index}"), "1"))
        })
        .collect();
    let record = ViewRecord { densities, ..clean };
    let mut state = State::new(true);
    record.restore(&mut state, &mut Controls::new());
    assert_eq!(state.densities().len(), MAX_DENSITIES);
}

#[test]
fn an_unknown_density_token_drops_only_that_entry_and_keeps_the_envelope() {
    let keeper = volume_density("keeper", "1");
    let tail = density_spec("tail", DensityFamily::SteadyCurrent, "0,0,1");
    let mut bad_family = DensitySpecRecord::from_spec(&keeper);
    bad_family.name = "bad family".to_string();
    bad_family.family = "quark_charge".to_string();
    let mut bad_kind = DensitySpecRecord::from_spec(&keeper);
    bad_kind.name = "bad kind".to_string();
    bad_kind.support.kind = "tesseract".to_string();

    let settings = AppSettings {
        view: ViewRecord {
            formula: "(y, -x)".to_string(),
            densities: vec![
                DensitySpecRecord::from_spec(&keeper),
                bad_family,
                bad_kind,
                DensitySpecRecord::from_spec(&tail),
            ],
            ..ViewRecord::capture(&State::new(true))
        },
        ..AppSettings::default()
    };
    let mut storage = MemoryStorage::default();
    save(&mut storage, &settings);
    let loaded = load(Some(&storage));
    assert_eq!(
        loaded.view.formula, "(y, -x)",
        "an unknown density token must not reset the rest of the envelope"
    );
    assert_eq!(
        loaded.view.densities.len(),
        4,
        "the wire list keeps every raw entry for the restore filter"
    );

    let mut state = State::new(true);
    loaded.restore(&mut state, &mut Controls::new());
    assert_eq!(
        state.densities(),
        [keeper, tail].as_slice(),
        "only the unknown-token entries drop, and the survivors keep their order"
    );
}

#[test]
fn parameter_dependent_density_restores_against_its_stored_parameter() {
    let parameters = ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: "gain".to_string(),
            value: 2.0,
            min: None,
            max: None,
            step: 1.0,
        }],
    };
    let density = volume_density("scaled", "gain");
    let mut source = State::new(true);
    source.parameters = parameters.clone();
    source
        .commit_densities(vec![density.clone()])
        .expect("the fixture density binds the stored parameter");

    let settings = AppSettings {
        parameters: parameters.clone(),
        view: ViewRecord::capture(&source),
        ..AppSettings::default()
    };
    let mut state = State::new(true);
    settings.restore(&mut state, &mut Controls::new());
    assert_eq!(
        state.densities(),
        [density].as_slice(),
        "restore validates the density against the restored parameter set"
    );

    let absent = AppSettings {
        parameters: ParameterSet::default(),
        ..settings
    };
    let mut state = State::new(true);
    absent.restore(&mut state, &mut Controls::new());
    assert!(
        state.densities().is_empty(),
        "the same density drops when the restored parameter set cannot bind it"
    );
}
