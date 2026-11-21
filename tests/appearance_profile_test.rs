//! Issue #85 slice A1: the pure appearance-profile model.
//!
//! A profile bundles a viewport style, the layer/family overrides and the
//! colour mapping. Documents are strict and versioned; applying a profile is
//! atomic and must not touch field inputs, sources, camera or layout.

use eframe::egui::{Color32, Rgba};
use vector_field_explorer::appearance_profile::{
    AppearanceProfile, DEFAULT_PROFILE_NAME, MAX_PROFILE_NAME_BYTES, PROFILE_FORMAT_VERSION,
    ProfileDocument, builtins,
};
use vector_field_explorer::color_mapping::{ColorPalette, ColorQuantity};
use vector_field_explorer::color_ramp::{ColorRamp, ColorStop, RampKind, Rgb, ScaleMode};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::layer_style::LayerStyles;
use vector_field_explorer::viewmodel::State;
use vector_field_explorer::viewport_style::{MAX_LINE_WIDTH, ViewportStyle};

fn profile_named(name: &str) -> AppearanceProfile {
    let mut profile = builtins()
        .into_iter()
        .find(|profile| profile.name == name)
        .expect("the named builtin exists");
    profile.name = name.to_owned();
    profile
}

fn sample_ramp() -> ColorRamp {
    ColorRamp {
        stops: vec![
            ColorStop {
                position: 0.0,
                color: Rgb::from_hex("#001122").expect("valid hex"),
            },
            ColorStop {
                position: 0.5,
                color: Rgb::from_hex("#44aa88").expect("valid hex"),
            },
            ColorStop {
                position: 1.0,
                color: Rgb::from_hex("#ffeebb").expect("valid hex"),
            },
        ],
        reversed: false,
        kind: RampKind::Sequential,
    }
}

fn sample_document() -> ProfileDocument {
    ProfileDocument {
        version: PROFILE_FORMAT_VERSION,
        profiles: builtins(),
    }
}

/// The shipped looks in display order: the original nine, then seaborn
/// categorical, seaborn sequential and the popular editor schemes.
const EXPECTED_BUILTIN_ORDER: [&str; 24] = [
    "Dark laboratory",
    "White paper",
    "High contrast",
    "Tokyo Night",
    "Catppuccin Mocha",
    "Catppuccin Latte",
    "Nord",
    "Gruvbox Dark",
    "Solarized Light",
    "Seaborn Deep",
    "Seaborn Muted",
    "Seaborn Pastel",
    "Seaborn Bright",
    "Seaborn Dark",
    "Seaborn Colorblind",
    "Seaborn Rocket",
    "Seaborn Mako",
    "Seaborn Crest",
    "Seaborn Flare",
    "Dracula",
    "One Dark",
    "Monokai",
    "Solarized Dark",
    "Gruvbox Light",
];

fn ramp_hexes(profile: &AppearanceProfile) -> Vec<String> {
    profile
        .color
        .ramp
        .as_ref()
        .expect("the profile ships a ramp")
        .stops
        .iter()
        .map(|stop| stop.color.to_hex())
        .collect()
}

fn ramp_positions(profile: &AppearanceProfile) -> Vec<f32> {
    profile
        .color
        .ramp
        .as_ref()
        .expect("the profile ships a ramp")
        .stops
        .iter()
        .map(|stop| stop.position)
        .collect()
}

#[test]
fn builtin_profiles_all_validate_and_have_unique_names() {
    let profiles = builtins();
    assert_eq!(profiles.len(), 24, "twenty-four shipped looks");
    assert_eq!(
        profiles.first().map(|profile| profile.name.as_str()),
        Some(DEFAULT_PROFILE_NAME),
        "the dark laboratory default leads the list"
    );

    let mut names = std::collections::HashSet::new();
    for profile in &profiles {
        profile
            .validate()
            .unwrap_or_else(|error| panic!("{} must validate: {error}", profile.name));
        assert!(
            names.insert(profile.name.clone()),
            "duplicate builtin name {:?}",
            profile.name
        );
    }

    let order: Vec<&str> = profiles
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert_eq!(
        order, EXPECTED_BUILTIN_ORDER,
        "existing looks stay first, then the new groups in order"
    );
}

#[test]
fn representative_new_builtin_ramps_are_pinned() {
    // Categorical: all six seaborn palette colours, evenly spaced.
    let deep = profile_named("Seaborn Deep");
    assert_eq!(
        ramp_hexes(&deep),
        [
            "#4c72b0", "#dd8452", "#55a868", "#c44e52", "#8172b3", "#937860",
        ]
    );
    assert_eq!(ramp_positions(&deep), [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
    let deep_ramp = deep.color.ramp.expect("Seaborn Deep ships a ramp");
    assert_eq!(deep_ramp.kind, RampKind::Sequential);

    // Sequential: the documented dark-to-light teaching approximation.
    let rocket = profile_named("Seaborn Rocket");
    assert_eq!(
        ramp_hexes(&rocket),
        ["#03051a", "#781c81", "#c33d5b", "#ef8f59", "#fae6c6"]
    );
    assert_eq!(ramp_positions(&rocket), [0.0, 0.25, 0.5, 0.75, 1.0]);

    // Popular editor scheme: four stops with the matching axes.
    let dracula = profile_named("Dracula");
    assert_eq!(
        ramp_hexes(&dracula),
        ["#ff79c6", "#bd93f9", "#8be9fd", "#50fa7b"]
    );
    assert_eq!(ramp_positions(&dracula), [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]);
    assert_eq!(
        [
            dracula.viewport.axis_x_color,
            dracula.viewport.axis_y_color,
            dracula.viewport.axis_z_color,
        ],
        [
            Rgba::from(Color32::from_rgb(0xff, 0x55, 0x55)).to_array(),
            Rgba::from(Color32::from_rgb(0x50, 0xfa, 0x7b)).to_array(),
            Rgba::from(Color32::from_rgb(0x8b, 0xe9, 0xfd)).to_array(),
        ],
        "Dracula keeps the listed axes colours"
    );
}

#[test]
fn default_profile_matches_the_default_appearance() {
    let profile = profile_named(DEFAULT_PROFILE_NAME);
    assert_eq!(profile.viewport, ViewportStyle::default());
    assert_eq!(profile.layer_styles, LayerStyles::default());
    assert_eq!(profile.color.colormap, Colormap::Viridis);
    assert_eq!(profile.color.ramp, None);
    assert_eq!(profile.color.mode, ScaleMode::Linear);
    assert_eq!(profile.color.quantity, ColorQuantity::Magnitude);
}

#[test]
fn profile_round_trips_through_json() {
    let document = sample_document();
    let json = document.to_json().expect("a valid document serializes");
    assert!(!json.contains('\n'), "compact output has no newlines");
    assert!(!json.contains(": "), "compact output has no spaces");
    assert!(json.starts_with(&format!("{{\"version\":{PROFILE_FORMAT_VERSION}")));
    assert!(
        json.contains("\"name\":\"Dark laboratory\",\"viewport\""),
        "field order is pinned: {json}"
    );

    let parsed = ProfileDocument::from_json(&json).expect("the export imports");
    assert_eq!(parsed, document);
}

#[test]
fn unknown_or_future_version_is_rejected_before_parsing_profiles() {
    // The profile body is deliberately invalid: the version must be reported
    // before serde can look at it.
    let future = r#"{"version":2,"profiles":[{"name":5}]}"#;
    let error = ProfileDocument::from_json(future).expect_err("version 2 is rejected");
    assert!(
        error.starts_with("invalid appearance profiles:"),
        "version failures use the semantic prefix: {error}"
    );
    assert!(
        error.contains("version 2"),
        "names the found version: {error}"
    );
    assert!(
        !error.contains("invalid appearance profile JSON"),
        "the profile body was never parsed: {error}"
    );

    let unknown = r#"{"version":1,"profiles":[],"surprise":true}"#;
    let error = ProfileDocument::from_json(unknown).expect_err("unknown fields are rejected");
    assert!(
        error.starts_with("invalid appearance profile JSON:"),
        "serde failures use the JSON prefix: {error}"
    );
}

#[test]
fn import_is_atomic_and_rejects_duplicate_names() {
    let mut document = sample_document();
    document.profiles.truncate(2);
    document.profiles[1].name = document.profiles[0].name.clone();
    let json = serde_json::to_string(&document).expect("the raw document serializes");

    let error = ProfileDocument::from_json(&json).expect_err("duplicate names are rejected");
    assert!(
        error.starts_with("invalid appearance profiles:"),
        "the semantic prefix is used: {error}"
    );
    assert!(
        error.to_lowercase().contains("duplicate"),
        "the error names the duplicate: {error}"
    );

    // Re-importing a corrected copy still succeeds, so the failure left no
    // partial document behind.
    document.profiles[1].name = "Another look".to_owned();
    let json = serde_json::to_string(&document).expect("the corrected document serializes");
    let parsed = ProfileDocument::from_json(&json).expect("atomic import leaves no residue");
    assert_eq!(parsed.profiles.len(), 2);
}

#[test]
fn apply_changes_only_appearance_state() {
    let mut state = State::new(false);
    assert!(state.set_formula("cos(x) + sin(y)".to_owned()));
    let formula = state.formula.clone();
    let parameters = state.parameters.clone();
    let sources = state.sources.clone();
    let domain = state.domain;
    let mode_3d = state.mode_3d;
    let time = state.time;

    let profile = profile_named("Tokyo Night");
    profile.apply(&mut state).expect("the builtin applies");

    assert_eq!(state.viewport_style, profile.viewport);
    assert_eq!(state.layer_styles, profile.layer_styles);
    assert_eq!(state.colormap, profile.color.colormap);
    assert_eq!(state.color_ramp, profile.color.ramp);
    assert_eq!(state.color_mode, profile.color.mode);
    assert_eq!(state.color_quantity, profile.color.quantity);

    // Presentation only: field inputs, sources and session state are untouched.
    assert_eq!(state.formula, formula);
    assert_eq!(state.parameters, parameters);
    assert_eq!(state.sources, sources);
    assert_eq!(state.domain, domain);
    assert_eq!(state.mode_3d, mode_3d);
    assert_eq!(state.time, time);
}

#[test]
fn apply_is_atomic_and_rolls_back_on_invalid_input() {
    let mut state = State::new(false);
    state.viewport_style = ViewportStyle::white_paper();
    state.colormap = Colormap::Mono;
    state.color_ramp = Some(sample_ramp());
    state.color_mode = ScaleMode::SignedLog { half_width: 0.25 };
    state.color_quantity = ColorQuantity::ComponentX;
    state.layer_styles.layers[0].opacity = Some(0.5);
    state.dirty = false;

    let viewport = state.viewport_style;
    let layer_styles = state.layer_styles;
    let colormap = state.colormap;
    let ramp = state.color_ramp.clone();
    let color_mode = state.color_mode;
    let quantity = state.color_quantity;

    let mut invalid = profile_named("Nord");
    invalid.name = "   ".to_owned();
    let error = invalid
        .apply(&mut state)
        .expect_err("an invalid name is rejected");
    assert!(!error.is_empty());

    assert_eq!(state.viewport_style, viewport, "viewport is untouched");
    assert_eq!(
        state.layer_styles, layer_styles,
        "layer styles are untouched"
    );
    assert_eq!(state.colormap, colormap, "colormap is untouched");
    assert_eq!(state.color_ramp, ramp, "ramp is untouched");
    assert_eq!(state.color_mode, color_mode, "mode is untouched");
    assert_eq!(state.color_quantity, quantity, "quantity is untouched");
    assert!(!state.dirty, "a rejected apply does not dirty the scene");

    // A second invalid input proves the rollback path is not order-dependent.
    let mut invalid = profile_named("Nord");
    invalid.viewport.grid_width = f32::NAN;
    invalid
        .apply(&mut state)
        .expect_err("an invalid viewport is rejected");
    assert_eq!(state.viewport_style, viewport);
    assert_eq!(state.color_ramp, ramp);
}

#[test]
fn sanitized_repairs_non_finite_and_out_of_range_fields() {
    let mut profile = profile_named("Nord");
    profile.name = "  bad\u{7}name  ".to_owned();
    profile.viewport.grid_color = [f32::NAN, 0.2, 0.3, 1.0];
    profile.viewport.box_width = 1.0;
    profile.layer_styles.layers[0].width = Some(f32::NAN);
    profile.layer_styles.families[1].opacity = Some(2.0);
    profile.color.ramp = Some(ColorRamp {
        stops: Vec::new(),
        reversed: false,
        kind: RampKind::Sequential,
    });
    profile.color.mode = ScaleMode::SignedLog {
        half_width: f32::NAN,
    };

    let sanitized = profile.sanitized();
    assert!(
        sanitized.validate().is_ok(),
        "sanitized output is always valid: {:?}",
        sanitized.validate()
    );
    assert_eq!(sanitized.name, "badname");
    assert!(sanitized.name.len() <= MAX_PROFILE_NAME_BYTES);
    assert_eq!(
        sanitized.viewport.grid_color,
        ViewportStyle::default().grid_color,
        "a non-finite colour resets to the default"
    );
    assert_eq!(
        sanitized.viewport.box_width, MAX_LINE_WIDTH,
        "a finite out-of-range width clamps to its bound"
    );
    assert_eq!(
        sanitized.layer_styles.layers[0].width, None,
        "a non-finite layer width becomes unset"
    );
    assert_eq!(
        sanitized.layer_styles.families[1].opacity,
        Some(1.0),
        "a finite out-of-range opacity clamps"
    );
    assert_eq!(
        sanitized.color.ramp,
        Some(ColorRamp::default()),
        "an invalid ramp falls back to the default ramp"
    );
    assert_eq!(
        sanitized.color.mode,
        ScaleMode::Linear,
        "an invalid signed-log mode falls back to linear"
    );

    let mut long = profile_named("Nord");
    long.name = "x".repeat(MAX_PROFILE_NAME_BYTES + 10);
    let sanitized_long = long.sanitized();
    assert_eq!(sanitized_long.name.len(), MAX_PROFILE_NAME_BYTES);
    assert!(sanitized_long.validate().is_ok());

    let mut blank = profile_named("Nord");
    blank.name = " \u{1} ".to_owned();
    assert_eq!(
        blank.sanitized().name,
        DEFAULT_PROFILE_NAME,
        "a name that repairs to empty takes the default"
    );
}

#[test]
fn theme_profiles_convert_srgb_colours_to_the_stored_linear_values() {
    let tokyo = profile_named("Tokyo Night");

    // `ViewportStyle` stores linear RGB. egui's `Color32` -> `Rgba` conversion
    // is the shipped sRGB-to-linear oracle, so equality pins the formula.
    let background = Rgba::from(Color32::from_rgb(0x1a, 0x1b, 0x26));
    assert_eq!(
        tokyo.viewport.background,
        [background.r(), background.g(), background.b()],
        "background #1a1b26 is stored as linear RGB"
    );
    assert_ne!(
        tokyo.viewport.background[0],
        f32::from(0x1a_u8) / 255.0,
        "the stored value is not the raw sRGB fraction"
    );

    let axis = Rgba::from(Color32::from_rgb(0xf7, 0x76, 0x8e));
    assert_eq!(
        tokyo.viewport.axis_x_color,
        [axis.r(), axis.g(), axis.b(), 1.0],
        "axis #f7768e is stored as linear RGB with alpha 1"
    );

    let ramp = tokyo.color.ramp.expect("Tokyo Night ships a ramp");
    let colors: Vec<Rgb> = ramp.stops.iter().map(|stop| stop.color).collect();
    assert_eq!(
        colors,
        vec![
            Rgb::from_hex("#7aa2f7").expect("valid hex"),
            Rgb::from_hex("#bb9af7").expect("valid hex"),
            Rgb::from_hex("#7dcfff").expect("valid hex"),
        ],
        "ramp stops stay sRGB bytes"
    );
}

#[test]
fn apply_bumps_the_generation_when_only_the_colormap_differs() {
    let mut profile = profile_named(DEFAULT_PROFILE_NAME);
    profile.color.colormap = Colormap::Turbo;
    profile.color.ramp = None;

    let mut state = State::new(false);
    assert_eq!(state.colormap, Colormap::Viridis);
    assert_eq!(state.color_ramp, None);
    assert_eq!(
        state.plot.generation, 0,
        "a fresh state has not built a scene yet"
    );

    profile.apply(&mut state).expect("the profile applies");

    assert_eq!(state.colormap, Colormap::Turbo);
    assert_eq!(
        state.plot.generation, 1,
        "a new palette invalidates geometry baked with the old colours"
    );
}

#[test]
fn apply_bumps_the_generation_when_only_the_ramp_differs() {
    let mut profile = profile_named(DEFAULT_PROFILE_NAME);
    profile.color.ramp = Some(sample_ramp());

    let mut state = State::new(false);
    assert_eq!(state.colormap, Colormap::Viridis);
    assert_eq!(state.color_ramp, None);
    assert_eq!(state.plot.generation, 0);

    profile.apply(&mut state).expect("the profile applies");

    assert_eq!(state.color_ramp, profile.color.ramp);
    assert_eq!(
        state.colormap, profile.color.colormap,
        "the colormap field is unchanged"
    );
    assert_eq!(
        state.plot.generation, 1,
        "a new ramp is one colour change, not two"
    );
}

#[test]
fn apply_without_a_colour_change_leaves_the_generation_untouched() {
    let mut state = State::new(false);
    let default = profile_named(DEFAULT_PROFILE_NAME);
    default
        .apply(&mut state)
        .expect("the default profile applies");
    assert_eq!(
        state.plot.generation, 0,
        "applying the current appearance is a no-op"
    );

    // A second apply of a non-default profile must not invalidate again.
    let tokyo = profile_named("Tokyo Night");
    tokyo.apply(&mut state).expect("Tokyo Night applies");
    let generation = state.plot.generation;
    assert!(generation > 0, "the first colour change bumped it");
    tokyo.apply(&mut state).expect("reapplying is a no-op");
    assert_eq!(
        state.plot.generation, generation,
        "an identical reapply leaves the generation alone"
    );
}

#[test]
fn capture_round_trips_the_live_appearance() {
    let mut source = State::new(false);
    source.viewport_style = ViewportStyle::high_contrast();
    source.viewport_style.grid_width = MAX_LINE_WIDTH / 2.0;
    source.layer_styles.layers[0].opacity = Some(0.5);
    source.layer_styles.families[1].width = Some(1.25);
    source.colormap = Colormap::Mono;
    source.color_ramp = Some(sample_ramp());
    source.color_mode = ScaleMode::SignedLog { half_width: 0.25 };
    source.color_quantity = ColorQuantity::ComponentX;

    let profile = AppearanceProfile::capture("Live look", &source);
    assert_eq!(profile.name, "Live look");

    let mut target = State::new(false);
    profile
        .apply(&mut target)
        .expect("the captured profile applies");

    assert_eq!(target.viewport_style, source.viewport_style);
    assert_eq!(target.layer_styles, source.layer_styles);
    assert_eq!(target.colormap, source.colormap);
    assert_eq!(target.color_ramp, source.color_ramp);
    assert_eq!(target.color_mode, source.color_mode);
    assert_eq!(target.color_quantity, source.color_quantity);
}

#[test]
fn captured_profiles_validate_and_double_apply_is_idempotent() {
    let mut source = State::new(false);
    source.viewport_style = ViewportStyle::white_paper();
    source.layer_styles.layers[2].color = Some(Rgb::from_hex("#ff0000").expect("valid hex"));
    source.colormap = Colormap::Turbo;
    source.color_mode = ScaleMode::SignedLog { half_width: 0.5 };
    source.color_quantity = ColorQuantity::ComponentY;

    let profile = AppearanceProfile::capture("Captured", &source);
    profile
        .validate()
        .unwrap_or_else(|error| panic!("a captured profile must validate: {error}"));

    let mut target = State::new(false);
    profile
        .apply(&mut target)
        .expect("the first apply succeeds");
    let once = (
        target.viewport_style,
        target.layer_styles,
        target.colormap,
        target.color_ramp.clone(),
        target.color_mode,
        target.color_quantity,
        target.plot.generation,
    );

    profile
        .apply(&mut target)
        .expect("the second apply succeeds");
    let twice = (
        target.viewport_style,
        target.layer_styles,
        target.colormap,
        target.color_ramp.clone(),
        target.color_mode,
        target.color_quantity,
        target.plot.generation,
    );

    assert_eq!(once, twice, "applying a captured profile twice equals once");
}

#[test]
fn ramp_overrides_the_colormap_field() {
    let mut profile = profile_named(DEFAULT_PROFILE_NAME);
    profile.color.colormap = Colormap::Mono;
    profile.color.ramp = Some(sample_ramp());

    let mut state = State::new(false);
    profile.apply(&mut state).expect("the profile applies");

    // Both fields are kept, but the effective palette is the custom ramp.
    assert_eq!(state.colormap, Colormap::Mono);
    assert_eq!(state.color_ramp, profile.color.ramp);
    assert_eq!(
        state.color_mapping().palette,
        ColorPalette::Custom(sample_ramp()),
        "a present ramp overrides the colormap"
    );
}
