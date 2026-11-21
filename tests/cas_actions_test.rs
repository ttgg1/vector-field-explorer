//! Issue #170: typed CAS app-control actions.
//!
//! Parse/preview tests pin the fixed verb vocabulary and every rejection.
//! Application tests drive the real `VfApp` through the same setters the UI
//! uses, and assert the one-entry/atomic contract with the app's own document
//! snapshot and edit history.

use eframe::egui;
use glam::Vec3;

use vector_field_explorer::VfApp;
use vector_field_explorer::appearance_profile::AppearanceProfile;
use vector_field_explorer::cas_actions::{self, CameraFraming, CasAction, CasActionError};
use vector_field_explorer::color_mapping::ColorQuantity;
use vector_field_explorer::color_ramp::ScaleMode;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::scalar_view::MAX_PROBES;
use vector_field_explorer::seed_layout::{SeedLayout, SeedPoint};
use vector_field_explorer::seed_layout_library::SeedLayoutProfile;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::cas_panel::{self, CasPanelState};
use vector_field_explorer::viewmodel::State;

use crate::support;

fn frame(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) -> egui::FullOutput {
    frame_height(ctx, app, events, 700.0)
}

fn frame_height(
    ctx: &egui::Context,
    app: &mut VfApp,
    events: Vec<egui::Event>,
    height: f32,
) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, height),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn idle(ctx: &egui::Context, app: &mut VfApp) {
    let _ = frame(ctx, app, Vec::new());
}

fn settled(ctx: &egui::Context, app: &mut VfApp) {
    for _ in 0..4 {
        idle(ctx, app);
    }
}

fn parse(line: &str) -> CasAction {
    cas_actions::parse_action(line).unwrap_or_else(|error| panic!("{line:?}: {error}"))
}

#[test]
fn every_verb_parses_case_insensitively_and_previews() {
    let cases = [
        (
            "formula (y, -x)",
            CasAction::SetFormula {
                formula: "(y, -x)".into(),
            },
            "Formula: (y, -x)",
        ),
        (
            "FORMULA (y, -x, 0)",
            CasAction::SetFormula {
                formula: "(y, -x, 0)".into(),
            },
            "Formula: (y, -x, 0)",
        ),
        (
            "scalar x + y",
            CasAction::SetScalarSource {
                expr: "x + y".into(),
            },
            "Surface source: x + y",
        ),
        (
            "param k=1.5, m=2",
            CasAction::SetParameters {
                values: vec![("k".into(), 1.5), ("m".into(), 2.0)],
            },
            "Parameters: k=1.5, m=2",
        ),
        (
            "domain 2, 2, 1",
            CasAction::SetDomain {
                half_extents: [2.0, 2.0, 1.0],
            },
            "Domain half-extents: 2 × 2 × 1 (centred at the origin)",
        ),
        (
            "density 20",
            CasAction::SetDensity {
                samples_per_axis: 20,
            },
            "Density: 20 samples per axis",
        ),
        (
            "3d",
            CasAction::SetDimension { three_d: true },
            "Switch to 3D",
        ),
        (
            "2D",
            CasAction::SetDimension { three_d: false },
            "Switch to 2D",
        ),
        (
            "colors turbo x signedlog:0.5",
            CasAction::SetColors {
                colormap: Colormap::Turbo,
                quantity: ColorQuantity::ComponentX,
                mode: ScaleMode::SignedLog { half_width: 0.5 },
            },
            "Colours: Turbo, X component, signed log (half width 0.5)",
        ),
        (
            "COLORS MONO MAGNITUDE LINEAR",
            CasAction::SetColors {
                colormap: Colormap::Mono,
                quantity: ColorQuantity::Magnitude,
                mode: ScaleMode::Linear,
            },
            "Colours: Mono, Magnitude, linear",
        ),
        (
            "appearance Paper",
            CasAction::ApplyAppearanceProfile {
                name: "Paper".into(),
            },
            "Appearance profile: Paper",
        ),
        (
            "probe 0.5, 0.5, 0",
            CasAction::AddPointProbe {
                position: [0.5, 0.5, 0.0],
            },
            "Point probe at (0.5, 0.5, 0)",
        ),
        (
            "seeds Mine",
            CasAction::ApplySeedLayout {
                name: "Mine".into(),
            },
            "Seed layout: Mine",
        ),
        (
            "camera isometric",
            CasAction::SetCamera {
                framing: CameraFraming::Isometric,
            },
            "Camera: isometric",
        ),
        (
            "camera FIT",
            CasAction::SetCamera {
                framing: CameraFraming::Fit,
            },
            "Camera: fit",
        ),
        (
            "camera reset",
            CasAction::SetCamera {
                framing: CameraFraming::Reset,
            },
            "Camera: reset",
        ),
        (
            "play",
            CasAction::SetTransport { playing: true },
            "Transport: play",
        ),
        (
            "PAUSE",
            CasAction::SetTransport { playing: false },
            "Transport: pause",
        ),
    ];
    for (line, action, preview) in cases {
        assert_eq!(parse(line), action, "{line}");
        assert_eq!(cas_actions::preview(&action), preview, "{line}");
    }
}

#[test]
fn bad_values_are_rejected_before_any_mutation() {
    for (line, field) in [
        ("formula ", "formula"),
        ("formula (x,)", "formula"),
        ("formula not-a-field", "formula"),
        ("scalar ", "scalar"),
        ("scalar x +", "scalar"),
        ("param", "param"),
        ("param k", "param"),
        ("param k=nan", "param"),
        ("param k=1, k=2", "param"),
        ("domain 2, 2", "domain"),
        ("domain 0, 2, 2", "domain"),
        ("domain 101, 1, 1", "domain"),
        ("domain inf, 1, 1", "domain"),
        ("density 3", "density"),
        ("density 201", "density"),
        ("density 2.5", "density"),
        ("3d now", "dimension"),
        ("colors chartreuse", "colors"),
        ("colors turbo magnitude bogus", "colors"),
        ("colors turbo magnitude signedlog:0", "colors"),
        ("appearance   ", "appearance"),
        ("probe 0.5, 0.5", "probe"),
        ("probe nan, 0, 0", "probe"),
        ("seeds   ", "seeds"),
        ("camera dolly", "camera"),
        ("play now", "play"),
        ("pause now", "pause"),
    ] {
        match cas_actions::parse_action(line) {
            Err(CasActionError::BadValue { field: actual, .. }) => {
                assert_eq!(actual, field, "{line}")
            }
            other => panic!("{line:?} should be a bad {field}, got {other:?}"),
        }
    }
    assert_eq!(cas_actions::parse_action("   "), Err(CasActionError::Empty));
    assert_eq!(
        cas_actions::parse_action("frobnicate 1"),
        Err(CasActionError::UnknownVerb)
    );
    assert_eq!(
        cas_actions::parse_action("x*y"),
        Err(CasActionError::UnknownVerb)
    );
    assert_eq!(
        cas_actions::parse_action(&format!("formula {}", "x".repeat(600))),
        Err(CasActionError::TooLong)
    );
}

#[test]
fn whitespace_past_the_byte_limit_is_empty_not_too_long() {
    let oversized_whitespace = " ".repeat(vector_field_explorer::cas_intent::MAX_INPUT_BYTES + 1);
    assert_eq!(
        cas_actions::parse_action(&oversized_whitespace),
        Err(CasActionError::Empty)
    );
}

#[test]
fn unknown_colormap_echoes_the_original_token() {
    match cas_actions::parse_action("colors TurboBogus magnitude linear") {
        Err(CasActionError::BadValue {
            field: "colors",
            reason,
        }) => assert!(reason.contains("`TurboBogus`"), "{reason}"),
        other => panic!("expected an unknown-colormap rejection, got {other:?}"),
    }
}

#[test]
fn parameter_assignment_arity_is_bounded() {
    let eight = "param a=1,b=2,c=3,d=4,e=5,f=6,g=7,h=8";
    assert!(matches!(
        cas_actions::parse_action(eight),
        Ok(CasAction::SetParameters { values }) if values.len() == 8
    ));
    assert!(matches!(
        cas_actions::parse_action("param a=1,b=2,c=3,d=4,e=5,f=6,g=7,h=8,i=9"),
        Err(CasActionError::BadValue { field: "param", .. })
    ));
}

fn parameter_set() -> ParameterSet {
    ParameterSet {
        version: 1,
        entries: vec![
            Parameter {
                name: "k".into(),
                value: 1.0,
                min: Some(0.0),
                max: Some(10.0),
                step: 0.1,
            },
            Parameter {
                name: "m".into(),
                value: 2.0,
                min: None,
                max: None,
                step: 1.0,
            },
        ],
    }
}

fn prepared_app(ctx: &egui::Context) -> VfApp {
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.state
        .commit_parameters(parameter_set())
        .expect("the fixture parameter set validates");
    settled(ctx, &mut app);
    app
}

#[test]
fn accepted_formula_is_one_dirty_transition_and_one_undo_entry() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    let original = app.state.formula.clone();
    let base = app.history_len();

    let preview = app
        .apply_cas_action(&CasAction::SetFormula {
            formula: "(y, -x, 0)".into(),
        })
        .expect("the formula validates");
    assert_eq!(preview, "Formula: (y, -x, 0)");
    assert_eq!(app.state.formula, "(y, -x, 0)");
    assert!(app.state.dirty, "an accepted action dirties the document");
    assert_eq!(
        app.history_len(),
        base,
        "history records at the frame boundary"
    );
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base + 1, "exactly one undo entry");

    assert!(app.undo_document(&ctx));
    assert_eq!(app.state.formula, original, "one undo restores the formula");
}

#[test]
fn sources_mode_refuses_formula_and_scalar_before_any_mutation() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    app.state.set_field_mode(FieldMode::Sources);
    settled(&ctx, &mut app);
    let before = app.capture_document_for_test();
    let base = app.history_len();
    let dirty = app.state.dirty;

    for action in [
        CasAction::SetFormula {
            formula: "(y, -x, 0)".into(),
        },
        CasAction::SetScalarSource {
            expr: "x + y".into(),
        },
    ] {
        let error = app
            .apply_cas_action(&action)
            .expect_err("Sources mode refuses formula-field edits");
        assert_eq!(
            error,
            CasActionError::UnsupportedHere(
                "CAS edits the formula field; switch to Formula.".into()
            ),
            "{action:?}"
        );
        assert_eq!(app.capture_document_for_test(), before, "{action:?}");
        assert_eq!(app.state.dirty, dirty, "{action:?}");
    }
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base, "a refusal records nothing");
}

#[test]
fn colours_change_three_fields_as_one_entry_and_undo_together() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    let before = app.capture_document_for_test();
    let base = app.history_len();

    app.apply_cas_action(&CasAction::SetColors {
        colormap: Colormap::Turbo,
        quantity: ColorQuantity::ComponentX,
        mode: ScaleMode::SignedLog { half_width: 0.5 },
    })
    .expect("the colour mapping validates");
    assert_eq!(app.state.colormap, Colormap::Turbo);
    assert_eq!(app.state.color_quantity, ColorQuantity::ComponentX);
    assert_eq!(
        app.state.color_mode,
        ScaleMode::SignedLog { half_width: 0.5 }
    );
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base + 1, "three setters, one entry");

    assert!(app.undo_document(&ctx));
    assert_eq!(app.capture_document_for_test(), before);
}

#[test]
fn parameters_update_only_the_committed_set_and_reject_atomically() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    let before = app.capture_document_for_test();
    let base = app.history_len();

    app.apply_cas_action(&CasAction::SetParameters {
        values: vec![("k".into(), 2.5)],
    })
    .expect("the existing parameter accepts a valid value");
    assert_eq!(app.state.parameter_value("k"), Some(2.5));
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base + 1);

    // Unknown names, non-finite values and out-of-bounds values never reach a
    // setter: the whole document and history stay exactly as they were.
    let baseline = app.capture_document_for_test();
    let entries = app.history_len();
    let dirty = app.state.dirty;
    for values in [
        vec![("not_a_parameter".to_owned(), 1.0)],
        vec![("k".to_owned(), f32::NAN)],
        vec![("k".to_owned(), 20.0)],
    ] {
        let error = app
            .apply_cas_action(&CasAction::SetParameters { values })
            .expect_err("the value is rejected");
        assert!(matches!(
            error,
            CasActionError::BadValue { field: "param", .. }
        ));
        assert_eq!(app.capture_document_for_test(), baseline);
        assert_eq!(app.state.dirty, dirty);
    }
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), entries, "a rejection records nothing");
    assert_eq!(app.state.parameter_value("k"), Some(2.5));
    assert!(app.undo_document(&ctx));
    assert_eq!(app.capture_document_for_test(), before);
}

#[test]
fn domain_and_density_reject_atomically_before_any_camera_motion() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    // A 3D formula and 3D view make density 100 exceed the 3D sampling bound.
    app.state.set_formula("(y, -x, z)".into());
    app.state.set_mode(true);
    settled(&ctx, &mut app);
    let before = app.capture_document_for_test();
    let base = app.history_len();
    let dirty = app.state.dirty;

    for action in [
        CasAction::SetDomain {
            half_extents: [0.05, 1.0, 1.0],
        },
        CasAction::SetDomain {
            half_extents: [f32::NAN, 1.0, 1.0],
        },
        CasAction::SetDensity {
            samples_per_axis: 100,
        },
    ] {
        let error = app
            .apply_cas_action(&action)
            .expect_err("the action is rejected");
        assert!(matches!(
            error,
            CasActionError::BadValue {
                field: "domain" | "density",
                ..
            }
        ));
        assert_eq!(app.capture_document_for_test(), before);
        assert_eq!(app.state.dirty, dirty);
    }
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base, "rejections record nothing");

    // An accepted domain is one entry and frames the new box on the next frame.
    app.apply_cas_action(&CasAction::SetDomain {
        half_extents: [2.0, 3.0, 4.0],
    })
    .expect("the domain is accepted");
    assert_eq!(app.state.domain.min, Vec3::new(-2.0, -3.0, -4.0));
    assert_eq!(app.state.domain.max, Vec3::new(2.0, 3.0, 4.0));
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base + 1);
    assert!(app.undo_document(&ctx));
    assert_eq!(app.capture_document_for_test(), before);
}

#[test]
fn probes_respect_the_shared_cap_and_scene_domain() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    app.state.set_scalar_source(
        vector_field_explorer::scalar::ScalarContext::Surface,
        "x + y".into(),
    );
    settled(&ctx, &mut app);
    let before = app.capture_document_for_test();
    let dirty = app.state.dirty;

    for position in [[2.0, 0.0, 0.0], [f32::NAN, 0.0, 0.0]] {
        let error = app
            .apply_cas_action(&CasAction::AddPointProbe { position })
            .expect_err("the probe is rejected");
        assert!(matches!(
            error,
            CasActionError::BadValue { field: "probe", .. } | CasActionError::UnsupportedHere(_)
        ));
        assert_eq!(app.capture_document_for_test(), before);
        assert_eq!(app.state.dirty, dirty);
    }

    for _ in 0..MAX_PROBES {
        app.state
            .add_point_probe(Vec3::ZERO, None)
            .expect("room for the fixture probes");
    }
    settled(&ctx, &mut app);
    let before = app.capture_document_for_test();
    let base = app.history_len();
    let error = app
        .apply_cas_action(&CasAction::AddPointProbe {
            position: [0.1, 0.1, 0.0],
        })
        .expect_err("the probe cap is enforced");
    assert!(matches!(
        error,
        CasActionError::BadValue { field: "probe", .. }
    ));
    assert_eq!(app.capture_document_for_test(), before);
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base);
    assert_eq!(app.state.probes.points.len(), MAX_PROBES);
}

#[test]
fn scalar_dimension_and_density_apply_through_existing_setters() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    let before = app.capture_document_for_test();
    let base = app.history_len();

    app.apply_cas_action(&CasAction::SetScalarSource {
        expr: "x*y + 1".into(),
    })
    .expect("the surface expression validates");
    assert_eq!(app.state.plot.surface.source, "x*y + 1");
    app.apply_cas_action(&CasAction::SetDimension { three_d: true })
        .expect("the dimension switches");
    assert!(app.state.mode_3d);
    app.apply_cas_action(&CasAction::SetDensity {
        samples_per_axis: 30,
    })
    .expect("density fits the 3D bound");
    assert_eq!(app.state.density, 30);
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base + 1, "all three edits are one entry");

    assert!(app.undo_document(&ctx));
    assert_eq!(app.capture_document_for_test(), before);
}

#[test]
fn named_libraries_resolve_or_reject_without_mutation() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);

    let before = app.capture_document_for_test();
    let base = app.history_len();
    let error = app
        .apply_cas_action(&CasAction::ApplyAppearanceProfile {
            name: "Missing".into(),
        })
        .expect_err("a missing profile is unsupported");
    assert!(matches!(error, CasActionError::UnsupportedHere(_)));
    let error = app
        .apply_cas_action(&CasAction::ApplySeedLayout {
            name: "Missing".into(),
        })
        .expect_err("a missing layout is unsupported");
    assert!(matches!(error, CasActionError::UnsupportedHere(_)));
    assert_eq!(app.capture_document_for_test(), before);
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base);

    let mut profile = AppearanceProfile::capture("Warm", &app.state);
    profile.color.colormap = Colormap::Turbo;
    app.profiles.upsert(profile).expect("the profile is valid");
    let center = app.state.scene_domain().center();
    app.seed_layouts
        .upsert(
            SeedLayoutProfile {
                name: "Mine".into(),
                seeds: SeedLayout {
                    seeds: vec![SeedPoint {
                        position: center,
                        anchor: None,
                    }],
                },
            },
            &app.state.scene_domain(),
        )
        .expect("the layout is valid");
    settled(&ctx, &mut app);
    let base = app.history_len();

    app.apply_cas_action(&CasAction::ApplyAppearanceProfile {
        name: "Warm".into(),
    })
    .expect("the profile resolves");
    assert_eq!(app.state.colormap, Colormap::Turbo);
    app.apply_cas_action(&CasAction::ApplySeedLayout {
        name: "Mine".into(),
    })
    .expect("the layout resolves");
    assert_eq!(app.state.seed_layout().len(), 1);
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base + 1, "both edits land in one frame");
}

#[test]
fn camera_actions_reframe_and_transport_stays_runtime_only() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    app.orbit.yaw = 2.0;
    app.orbit.target = Vec3::new(5.0, 5.0, 5.0);
    settled(&ctx, &mut app);
    let base = app.history_len();

    app.apply_cas_action(&CasAction::SetCamera {
        framing: CameraFraming::Fit,
    })
    .expect("fit is supported");
    let centre = app.state.scene_domain().center();
    assert!((app.orbit.target - centre).length() < 1e-5);
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base + 1, "camera framing is one entry");

    app.orbit.yaw = 2.0;
    settled(&ctx, &mut app);
    app.apply_cas_action(&CasAction::SetCamera {
        framing: CameraFraming::Isometric,
    })
    .expect("isometric is supported");
    assert!(
        (app.orbit.yaw - vector_field_explorer::camera::OrbitCamera::default().yaw).abs() < 1e-6
    );
    assert!((app.orbit.target - centre).length() < 1e-5);

    // Transport toggles runtime state only: no document change, no dirty flag
    // and no undo entry.
    idle(&ctx, &mut app);
    let snapshot = app.capture_document_for_test();
    let base = app.history_len();
    let dirty = app.state.dirty;
    assert!(app.clock.playing);
    app.apply_cas_action(&CasAction::SetTransport { playing: false })
        .expect("pause is supported");
    assert!(!app.clock.playing);
    assert_eq!(app.capture_document_for_test(), snapshot);
    assert_eq!(app.state.dirty, dirty);
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base);
    app.apply_cas_action(&CasAction::SetTransport { playing: true })
        .expect("play is supported");
    assert!(app.clock.playing);
}

#[test]
fn neutral_actions_leave_the_document_and_history_untouched() {
    let ctx = support::context();
    let mut app = prepared_app(&ctx);
    let before = app.capture_document_for_test();
    let base = app.history_len();
    let dirty = app.state.dirty;

    app.apply_cas_action(&CasAction::SetFormula {
        formula: app.state.formula.clone(),
    })
    .expect("an equal formula is a validated no-op");
    app.apply_cas_action(&CasAction::SetDensity {
        samples_per_axis: app.state.density,
    })
    .expect("an equal density is a validated no-op");
    app.apply_cas_action(&CasAction::SetDimension { three_d: false })
        .expect("an equal dimension is a validated no-op");
    assert_eq!(app.capture_document_for_test(), before);
    assert_eq!(app.state.dirty, dirty);
    idle(&ctx, &mut app);
    assert_eq!(app.history_len(), base);
}

fn accesskit_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        (node.role() == egui::Role::Button && node.label() == Some(label))
            .then(|| {
                node.bounds().map(|bounds| {
                    egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    )
                })
            })
            .flatten()
    })
}

fn has_text(output: &egui::FullOutput, text: &str) -> bool {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .is_some_and(|update| {
            update.nodes.iter().any(|(_, node)| {
                [node.label(), node.value()]
                    .into_iter()
                    .flatten()
                    .any(|value| value.contains(text))
            })
        })
}

#[test]
fn panel_previews_an_action_then_commits_it_once_on_apply() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.cas_expanded = true;
    app.layout.controls_width = 460.0;
    app.layout.cas_width = 460.0;
    app.cas.input = "formula (y, -x, 0)".into();
    let original = app.state.formula.clone();
    settled(&ctx, &mut app);

    let mut output = frame_height(&ctx, &mut app, Vec::new(), 6000.0);
    for _ in 0..3 {
        output = frame_height(&ctx, &mut app, Vec::new(), 6000.0);
    }
    assert!(
        has_text(&output, "Formula: (y, -x, 0)"),
        "the panel previews the parsed action before applying"
    );
    assert_eq!(app.state.formula, original, "preview never mutates");
    let apply = accesskit_rect(&output, "Apply action").expect("the action Apply button");
    let pos = apply.center();
    let _ = frame_height(
        &ctx,
        &mut app,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        6000.0,
    );
    let _ = frame_height(
        &ctx,
        &mut app,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        6000.0,
    );
    assert_eq!(app.state.formula, "(y, -x, 0)");
    assert_eq!(app.history_len(), 1, "one click is one undo entry");
}

#[test]
fn panel_shows_the_precise_rejection_and_offers_no_apply() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(false);
    let mut panel = CasPanelState::default();
    panel.input = "domain 0, 1, 1".into();
    let mut output = egui::FullOutput::default();
    for _ in 0..3 {
        output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 1200.0),
                )),
                ..Default::default()
            },
            |ui| {
                egui::Panel::left("controls")
                    .default_size(460.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .show(ui, |ui| cas_panel::show(ui, &mut state, &mut panel));
                    });
            },
        );
        output.textures_delta.clear();
    }
    assert!(
        has_text(&output, "domain: half-extent 0 is outside 0.1..=100"),
        "the panel shows the precise rejection: {:?}",
        panel.action_error
    );
    assert!(accesskit_rect(&output, "Apply action").is_none());
    assert_eq!(
        state.domain,
        vector_field_explorer::domain::Domain::default()
    );
}
