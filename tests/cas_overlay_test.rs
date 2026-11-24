//! Issue #233: explicit linked/frozen CAS overlay dependencies, replacement
//! undo and backward-compatible scene persistence.
//!
//! The tests drive the shipped CAS scalar path and the ordinary gradient/scalar
//! seams. No second grammar, solver, quadrature or physical model exists.

use eframe::egui::{self, vec2};

use crate::support;

use vector_field_explorer::VfApp;
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::persistence::ViewRecord;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame};
use vector_field_explorer::scalar_analysis::{SeriesRole, potential_field_spec};
use vector_field_explorer::scalar_overlay::{
    FROZEN_RENDER_NOTE, MAX_OVERLAYS, OVERLAY_ASSUMPTIONS, OverlayDependency, OverlayLayer,
    OverlayLive, ScalarOverlay, ScalarOverlaySet,
};
use vector_field_explorer::scalar_view::scalar_view_expr;
use vector_field_explorer::scene_document::{SceneCamera, SceneDocument};
use vector_field_explorer::ui::cas_panel::{self, CasPanelState, Operation, SceneTarget, Variable};
use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::State;

fn xy_slice(z: f32) -> PlaneFrame {
    PlaneFrame {
        axis: PlaneAxis::Z,
        origin: glam::Vec3::new(0.0, 0.0, z),
        right: glam::Vec3::X,
        up: glam::Vec3::Y,
        normal: glam::Vec3::Z,
        half_u: 2.0,
        half_v: 2.0,
    }
}

fn parameters_d(value: f64) -> ParameterSet {
    ParameterSet {
        version: 1,
        entries: vec![Parameter {
            name: "d".to_owned(),
            value,
            min: None,
            max: None,
            step: 0.1,
        }],
    }
}

fn scalar_panel(input: &str) -> CasPanelState {
    let mut panel = CasPanelState::default();
    panel.input = input.to_owned();
    panel.operation = Operation::Differentiate;
    panel.variable = Variable::X;
    panel.refresh(2);
    panel
}

fn evaluate(formula: &str, parameters: &ParameterSet, x: f64, y: f64) -> f64 {
    scalar_view_expr(formula, parameters)
        .expect("the resolved expression parses")
        .eval_at(x, y, 0.0)
        .expect("the resolved expression is finite")
}

#[test]
fn overlay_model_distinguishes_linked_and_frozen_resolution() {
    let captured_parameters = parameters_d(1.0);
    let mut set = ScalarOverlaySet::default();
    let linked = set
        .add_linked("d*x", OverlayLayer::PotentialField, "live dipole")
        .expect("a linked overlay fits");
    let frozen = set
        .add_frozen(
            "d*x",
            OverlayLayer::PotentialField,
            "captured dipole",
            captured_parameters.clone(),
            xy_slice(0.0),
            0.5,
        )
        .expect("a frozen overlay fits");

    let live_parameters = parameters_d(2.0);
    let live_frame = xy_slice(1.0);
    let linked_resolution = set
        .resolve(
            linked,
            OverlayLive {
                parameters: &live_parameters,
                frame: live_frame,
                time: 3.0,
            },
        )
        .expect("the linked overlay resolves");
    assert_eq!(linked_resolution.dependency, OverlayDependency::Linked);
    assert_eq!(linked_resolution.parameters.value("d"), Some(2.0));
    assert_eq!(linked_resolution.frame.origin.z, 1.0);
    assert_eq!(linked_resolution.time, 3.0);
    assert_eq!(
        evaluate(
            &linked_resolution.formula,
            &linked_resolution.parameters,
            1.0,
            0.0
        ),
        2.0
    );

    let frozen_resolution = set
        .resolve(
            frozen,
            OverlayLive {
                parameters: &live_parameters,
                frame: live_frame,
                time: 3.0,
            },
        )
        .expect("the frozen overlay resolves against its capture");
    assert_eq!(frozen_resolution.dependency, OverlayDependency::Frozen);
    assert_eq!(frozen_resolution.parameters.value("d"), Some(1.0));
    assert_eq!(frozen_resolution.frame.origin.z, 0.0);
    assert_eq!(frozen_resolution.time, 0.5);
    assert_eq!(
        evaluate(
            &frozen_resolution.formula,
            &frozen_resolution.parameters,
            1.0,
            0.0
        ),
        1.0
    );

    let frozen_record = set.get(frozen).expect("the frozen record exists");
    assert_eq!(frozen_record.dependency.label(), "Frozen");
    let summary = frozen_record
        .captured_summary()
        .expect("a frozen overlay reports its captured values");
    assert!(summary.contains("d = 1"), "captured binding: {summary}");
    assert!(summary.contains("t = 0.500"), "captured time: {summary}");
    assert!(
        summary.to_ascii_lowercase().contains("slice"),
        "captured frame: {summary}"
    );
    assert!(!OVERLAY_ASSUMPTIONS.is_empty());
}

#[test]
fn linked_overlay_follows_separation_while_frozen_copy_keeps_its_capture() {
    let mut state = State::new(false);
    state
        .commit_parameters(parameters_d(1.0))
        .expect("commit d = 1");
    state.set_formula("(0, 0)".into());

    let linked = state
        .add_linked_overlay("d*x", OverlayLayer::PotentialField)
        .expect("the linked overlay is added");
    let frozen = state
        .add_frozen_overlay("d*x", OverlayLayer::PotentialField)
        .expect("the frozen copy is added");
    assert_eq!(state.overlays().len(), 2);

    let before = state.overlays().get(linked).unwrap();
    assert_eq!(before.dependency, OverlayDependency::Linked);
    assert!(before.capture.is_none());

    state
        .commit_parameters(parameters_d(2.0))
        .expect("edit the separation");

    let linked_resolution = state
        .resolve_overlay(linked)
        .expect("the linked overlay follows the live parameters");
    assert_eq!(linked_resolution.parameters.value("d"), Some(2.0));
    assert_eq!(
        evaluate(
            &linked_resolution.formula,
            &linked_resolution.parameters,
            1.0,
            0.0
        ),
        2.0
    );

    let frozen_resolution = state
        .resolve_overlay(frozen)
        .expect("the frozen copy resolves against its captured parameters");
    assert_eq!(frozen_resolution.parameters.value("d"), Some(1.0));
    assert_eq!(
        evaluate(
            &frozen_resolution.formula,
            &frozen_resolution.parameters,
            1.0,
            0.0
        ),
        1.0
    );
    assert_eq!(
        state.overlays().get(frozen).unwrap().dependency,
        OverlayDependency::Frozen
    );
}

#[test]
fn missing_parameters_and_invalid_frames_are_identifiable_unavailable_results() {
    let mut set = ScalarOverlaySet::default();
    let missing = set
        .add_linked("missing*x", OverlayLayer::PotentialField, "missing")
        .expect("the record itself is structurally valid");
    let live = OverlayLive {
        parameters: &ParameterSet::default(),
        frame: xy_slice(0.0),
        time: 0.0,
    };
    let error = set
        .resolve(missing, live)
        .expect_err("a missing parameter makes the overlay unavailable");
    assert!(
        error.to_string().contains("missing"),
        "the reason names the unresolved identifier: {error}"
    );

    let mut invalid_frame = xy_slice(0.0);
    invalid_frame.half_u = f32::NAN;
    let bad_frame = ScalarOverlay::frozen(
        "x",
        OverlayLayer::PotentialField,
        "bad frame",
        ParameterSet::default(),
        invalid_frame,
        0.0,
    );
    let set = ScalarOverlaySet::from_overlays(vec![bad_frame]);
    let error = set
        .resolve(
            set.as_slice()[0].id,
            OverlayLive {
                parameters: &ParameterSet::default(),
                frame: xy_slice(0.0),
                time: 0.0,
            },
        )
        .expect_err("an invalid captured frame is unavailable");
    assert!(
        error.to_string().to_ascii_lowercase().contains("finite")
            || error.to_string().to_ascii_lowercase().contains("frame"),
        "the reason names the frame problem: {error}"
    );

    let mut z_set = ScalarOverlaySet::default();
    let z_overlay = z_set
        .add_linked("z*x", OverlayLayer::PotentialField, "z")
        .expect("the record is structurally valid");
    let error = z_set
        .resolve(
            z_overlay,
            OverlayLive {
                parameters: &ParameterSet::default(),
                frame: xy_slice(0.0),
                time: 0.0,
            },
        )
        .expect_err("a z reference is an unsupported slice expression");
    assert!(
        error.to_string().to_ascii_lowercase().contains("x/y")
            || error.to_string().to_ascii_lowercase().contains("scalar"),
        "the explicit unsupported-slice reason is preserved: {error}"
    );
}

#[test]
fn the_overlay_set_is_bounded_and_stored_data_is_sanitized() {
    let mut set = ScalarOverlaySet::default();
    for index in 0..MAX_OVERLAYS {
        set.add_linked(
            format!("{index}*x"),
            OverlayLayer::PotentialField,
            format!("overlay {index}"),
        )
        .expect("entries up to the cap are accepted");
    }
    let error = set
        .add_linked("x", OverlayLayer::PotentialField, "one too many")
        .expect_err("the overlay set is bounded");
    assert!(
        error.contains(&MAX_OVERLAYS.to_string()),
        "cap named: {error}"
    );
    assert_eq!(set.len(), MAX_OVERLAYS);

    let long_label = "l".repeat(200);
    let duplicate = ScalarOverlay::linked(
        set.as_slice()[0].id,
        "replacement",
        OverlayLayer::Contours,
        long_label,
    );
    let empty = ScalarOverlay::linked(
        vector_field_explorer::scalar_overlay::OverlayId(99),
        "   ",
        OverlayLayer::Contours,
        "empty",
    );
    let frozen_without_capture = ScalarOverlay {
        id: vector_field_explorer::scalar_overlay::OverlayId(100),
        label: "no capture".to_owned(),
        formula: "x".to_owned(),
        layer: OverlayLayer::PotentialField,
        active_contour: false,
        dependency: OverlayDependency::Frozen,
        capture: None,
    };
    let mut stored = set.clone();
    let mut raw = stored.as_slice().to_vec();
    raw.push(duplicate);
    raw.push(empty);
    raw.push(frozen_without_capture);
    stored = ScalarOverlaySet::from_overlays(raw);

    let sanitized = stored.clone().sanitized();
    assert!(
        sanitized.len() <= MAX_OVERLAYS,
        "sanitizing enforces the cap"
    );
    assert!(
        sanitized
            .iter()
            .all(|overlay| !overlay.formula.trim().is_empty()),
        "empty expressions are dropped"
    );
    assert!(
        sanitized
            .iter()
            .all(|overlay| overlay.dependency != OverlayDependency::Frozen
                || overlay.capture.is_some()),
        "frozen entries without a capture are dropped"
    );
    assert!(
        sanitized.iter().all(|overlay| overlay.label.len()
            <= vector_field_explorer::scalar_overlay::MAX_OVERLAY_LABEL_BYTES),
        "over-long labels are truncated"
    );
    assert_eq!(
        sanitized
            .iter()
            .filter(|overlay| overlay.id == set.as_slice()[0].id)
            .count(),
        1,
        "duplicate ids keep one entry"
    );
    assert!(stored.validate().is_err(), "the raw set is invalid");
    assert!(sanitized.validate().is_ok(), "the sanitized set is valid");
}

#[test]
fn scene_document_preserves_overlay_dependencies_and_loads_older_files() {
    let mut overlays = ScalarOverlaySet::default();
    overlays
        .add_linked("d*x", OverlayLayer::PotentialField, "linked")
        .expect("linked");
    overlays
        .add_frozen(
            "d*x",
            OverlayLayer::Contours,
            "frozen",
            parameters_d(1.0),
            xy_slice(0.0),
            0.25,
        )
        .expect("frozen");
    let mut document = SceneDocument::capture(
        "overlays",
        ViewRecord::default(),
        parameters_d(2.0),
        SceneCamera::default(),
    )
    .with_overlays(overlays.clone());

    let json = document
        .to_json()
        .expect("a scene with overlays serializes");
    let restored = SceneDocument::from_json(&json).expect("a scene with overlays parses");
    assert_eq!(restored.overlays, overlays);
    assert_eq!(
        restored
            .overlays
            .iter()
            .map(|overlay| overlay.dependency)
            .collect::<Vec<_>>(),
        vec![OverlayDependency::Linked, OverlayDependency::Frozen]
    );
    let mut state = State::new(false);
    state
        .set_overlays(restored.overlays.clone())
        .expect("the imported overlays restore");
    assert_eq!(state.overlays().len(), 2);

    // Older files have no overlays key at all and still load.
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    value
        .as_object_mut()
        .expect("a document object")
        .remove("overlays");
    let legacy = serde_json::to_string(&value).expect("re-serializes");
    let older = SceneDocument::from_json(&legacy).expect("an older scene still loads");
    assert!(older.overlays.is_empty());

    // A newer file version is still rejected rather than repaired.
    document.version = 2;
    let future = document.to_json().expect("serializes");
    let error = SceneDocument::from_json(&future).expect_err("future versions are rejected");
    assert!(error.contains('2') && error.contains('1'), "got {error}");

    // Excess overlay data is rejected by validation, not silently truncated.
    let mut too_many = document_with_overlays(overlays);
    let mut raw = too_many.overlays.as_slice().to_vec();
    for index in 0..=MAX_OVERLAYS {
        raw.push(ScalarOverlay::linked(
            vector_field_explorer::scalar_overlay::OverlayId(200 + index as u32),
            format!("{index}*x"),
            OverlayLayer::Contours,
            format!("extra {index}"),
        ));
    }
    too_many.overlays = ScalarOverlaySet::from_overlays(raw);
    let error = too_many.validate().expect_err("bound is validated");
    assert!(error.contains(&MAX_OVERLAYS.to_string()), "got {error}");
}

fn document_with_overlays(overlays: ScalarOverlaySet) -> SceneDocument {
    SceneDocument::capture(
        "overlays",
        ViewRecord::default(),
        parameters_d(2.0),
        SceneCamera::default(),
    )
    .with_overlays(overlays)
}

#[test]
fn replace_field_is_validated_and_one_undo_restores_the_original() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.controls_width = 460.0;
    app.layout.cas_expanded = true;
    app.layout.cas_width = 460.0;
    app.state.set_formula("(1, 0)".into());
    app.cas = {
        let mut panel = CasPanelState::default();
        panel.input = "x*y".into();
        panel.operation = Operation::Gradient;
        panel.refresh(2);
        panel
    };
    let original = app.state.formula.clone();
    for _ in 0..4 {
        idle(&ctx, &mut app);
    }
    let base = app.history_len();

    click_app_button(&ctx, &mut app, "Replace field");
    assert_ne!(app.state.formula, original, "the field is replaced");
    assert_eq!(
        app.history_len(),
        base + 1,
        "Replace field is exactly one undo entry"
    );
    assert!(app.undo_document(&ctx), "one undo is available");
    assert_eq!(
        app.state.formula, original,
        "one undo restores the original field"
    );
    assert_eq!(app.history_len(), base);

    // A scalar result never offers a whole-field replacement.
    let mut state = State::new(false);
    let mut scalar = scalar_panel("x*y");
    assert_eq!(
        scalar.preview.as_ref().unwrap().kind,
        vector_field_explorer::ui::cas_panel::ResultKind::Scalar
    );
    let error = scalar
        .replace_field(&mut state, 2)
        .expect_err("a scalar result is not a whole-field model");
    assert!(error.contains("scalar"), "{error}");
}

fn render_panel(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut CasPanelState,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1400.0, 6000.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::Panel::left("overlay_controls")
                .resizable(false)
                .default_size(width)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        cas_panel::show(ui, state, panel);
                    });
                });
        },
    );
    output.textures_delta.clear();
    output
}

fn button_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    output
        .platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.role() == egui::Role::Button && node.label() == Some(label)).then(|| {
                node.bounds().map(|r| {
                    egui::Rect::from_min_max(
                        egui::pos2(r.x0 as f32, r.y0 as f32),
                        egui::pos2(r.x1 as f32, r.y1 as f32),
                    )
                })
            })?
        })
}

fn has_text(output: &egui::FullOutput, text: &str) -> bool {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .into_iter()
        .flat_map(|update| update.nodes.iter())
        .any(|(_, node)| {
            [node.label(), node.value()]
                .into_iter()
                .flatten()
                .any(|value| value.contains(text))
        })
}

fn click(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut CasPanelState,
    pos: egui::Pos2,
) {
    render_panel(
        ctx,
        width,
        state,
        panel,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    render_panel(
        ctx,
        width,
        state,
        panel,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

#[test]
fn cas_panel_offers_dependency_controls_and_shows_bindings_assumptions_and_captures() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(false);
    state
        .commit_parameters(parameters_d(1.0))
        .expect("commit d = 1");
    state.set_formula("(0, 0)".into());
    let mut panel = scalar_panel("x*y");

    let mut output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    for _ in 0..3 {
        output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    }
    assert!(has_text(&output, "Expression:"));
    assert!(has_text(&output, "Bindings:"));
    assert!(has_text(&output, "d = 1"));
    assert!(has_text(&output, OVERLAY_ASSUMPTIONS));
    let linked = button_bounds(&output, "Add linked overlay")
        .expect("the linked overlay control is offered");
    let frozen =
        button_bounds(&output, "Add frozen copy").expect("the frozen copy control is offered");

    click(&ctx, 460.0, &mut state, &mut panel, linked.center());
    assert_eq!(state.overlays().len(), 1);
    assert!(state.gradient.is_some(), "the linked overlay is rendered");
    let linked_index = 0;

    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    let frozen = button_bounds(&output, "Add frozen copy")
        .map(|rect| rect.center())
        .unwrap_or(frozen.center());
    click(&ctx, 460.0, &mut state, &mut panel, frozen);
    assert_eq!(state.overlays().len(), 2);

    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    assert!(has_text(&output, "Linked"), "linked is visibly labelled");
    assert!(has_text(&output, "Frozen"), "frozen is visibly labelled");
    assert!(
        has_text(&output, "d = 1") && has_text(&output, "t = 0.000"),
        "the frozen copy shows its captured values"
    );
    assert!(
        has_text(&output, FROZEN_RENDER_NOTE),
        "the single rendered gradient limitation is stated honestly"
    );
    assert_eq!(
        state.overlays().as_slice()[linked_index].dependency,
        OverlayDependency::Linked
    );

    // The same dependency controls exist in the compact-width panel.
    let output = render_panel(&ctx, 390.0, &mut state, &mut panel, Vec::new());
    assert!(
        button_bounds(&output, "Add linked overlay").is_some(),
        "the linked control survives a narrow panel"
    );
    assert!(
        button_bounds(&output, "Add frozen copy").is_some(),
        "the frozen control survives a narrow panel"
    );
}

#[test]
fn removing_the_drawn_linked_overlay_through_the_panel_clears_the_gradient() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(false);
    state
        .commit_parameters(parameters_d(1.0))
        .expect("commit d = 1");
    state.set_formula("(0, 0)".into());
    let mut panel = scalar_panel("x*y");

    let mut output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    for _ in 0..3 {
        output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    }
    let add = button_bounds(&output, "Add linked overlay")
        .expect("the linked overlay control is offered");
    click(&ctx, 460.0, &mut state, &mut panel, add.center());
    assert_eq!(state.overlays().len(), 1);
    assert!(state.gradient.is_some(), "the linked overlay is drawn");
    let id = state.overlays().as_slice()[0].id;

    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    assert!(
        has_text(&output, "Drawn now"),
        "the list marks the linked overlay that is actually drawn"
    );
    let remove = button_bounds(&output, &format!("Remove {id}"))
        .expect("the drawn overlay has a real Remove control");
    click(&ctx, 460.0, &mut state, &mut panel, remove.center());

    assert!(state.overlays().is_empty(), "the overlay disappears");
    assert!(
        state.gradient.is_none(),
        "removing the drawn overlay clears the gradient it owned"
    );
    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    assert!(
        !has_text(&output, "Drawn now"),
        "the list is honest after the drawn overlay is gone"
    );

    // The remove control also survives the compact-width panel.
    let output = render_panel(&ctx, 390.0, &mut state, &mut panel, Vec::new());
    let add = button_bounds(&output, "Add linked overlay")
        .expect("the linked overlay control survives a narrow panel");
    click(&ctx, 390.0, &mut state, &mut panel, add.center());
    let id = state.overlays().as_slice()[0].id;
    let output = render_panel(&ctx, 390.0, &mut state, &mut panel, Vec::new());
    assert!(
        button_bounds(&output, &format!("Remove {id}")).is_some(),
        "the per-overlay Remove control survives a narrow panel"
    );
}

#[test]
fn removing_a_frozen_copy_keeps_the_linked_gradient_and_removal_reaches_the_cap_case() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(false);
    state
        .commit_parameters(parameters_d(1.0))
        .expect("commit d = 1");
    state.set_formula("(0, 0)".into());
    let mut panel = scalar_panel("x*y");

    let mut output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    let add_linked = button_bounds(&output, "Add linked overlay")
        .expect("the linked overlay control is offered");
    click(&ctx, 460.0, &mut state, &mut panel, add_linked.center());
    output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    let add_frozen =
        button_bounds(&output, "Add frozen copy").expect("the frozen copy control is offered");
    click(&ctx, 460.0, &mut state, &mut panel, add_frozen.center());
    assert_eq!(state.overlays().len(), 2);
    assert!(state.gradient.is_some());

    let frozen_id = state.overlays().as_slice()[1].id;
    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    let remove_frozen = button_bounds(&output, &format!("Remove {frozen_id}"))
        .expect("a frozen copy also has a real Remove control");
    click(&ctx, 460.0, &mut state, &mut panel, remove_frozen.center());
    assert_eq!(state.overlays().len(), 1);
    assert!(
        state.gradient.is_some(),
        "removing a frozen copy leaves the drawn linked gradient untouched"
    );
    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    assert!(has_text(&output, "Drawn now"));

    // Fill to the cap, then remove from the panel even after the cap message
    // told the user to do exactly that.
    for _ in 0..MAX_OVERLAYS - 1 {
        let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
        let add = button_bounds(&output, "Add linked overlay")
            .expect("the linked control remains offered below the cap");
        click(&ctx, 460.0, &mut state, &mut panel, add.center());
    }
    assert_eq!(state.overlays().len(), MAX_OVERLAYS);
    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    let add = button_bounds(&output, "Add linked overlay")
        .expect("the add control is still visible at the cap");
    click(&ctx, 460.0, &mut state, &mut panel, add.center());
    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    assert!(
        has_text(&output, "remove one before adding another"),
        "the cap message instructs the user to remove an overlay"
    );
    let last_id = state.overlays().as_slice().last().unwrap().id;
    let remove = button_bounds(&output, &format!("Remove {last_id}"))
        .expect("the cap case still exposes the removal control");
    click(&ctx, 460.0, &mut state, &mut panel, remove.center());
    assert_eq!(state.overlays().len(), MAX_OVERLAYS - 1);
}

#[test]
fn a_missing_parameter_overlay_is_shown_as_unavailable_in_the_panel() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(false);
    let mut panel = scalar_panel("x*y");
    let id = state
        .add_linked_overlay("missing*x", OverlayLayer::PotentialField)
        .expect("the record exists even with a missing parameter");
    assert!(state.resolve_overlay(id).is_err());

    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    assert!(
        has_text(&output, "Unavailable:"),
        "a missing parameter is identifiable rather than an empty layer"
    );
    assert!(
        has_text(&output, "missing"),
        "the unavailable reason names the missing identifier"
    );
}

fn idle(ctx: &egui::Context, app: &mut VfApp) {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1280.0, 6000.0),
            )),
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
}

fn app_output(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1280.0, 6000.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn click_app_button(ctx: &egui::Context, app: &mut VfApp, label: &str) {
    let output = app_output(ctx, app, Vec::new());
    let pos = button_bounds(&output, label)
        .unwrap_or_else(|| panic!("the shipped panel offers the {label:?} button"))
        .center();
    app_output(
        ctx,
        app,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    app_output(
        ctx,
        app,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

#[test]
fn the_overlay_layer_labels_do_not_claim_an_unshipped_physics_model() {
    assert_eq!(OverlayLayer::PotentialField.label(), "E = -∇V");
    assert_eq!(OverlayLayer::Contours.label(), "contours");
    assert_eq!(OverlayDependency::Linked.label(), "Linked");
    assert_eq!(OverlayDependency::Frozen.label(), "Frozen");
    assert!(FROZEN_RENDER_NOTE.to_ascii_lowercase().contains("one"));
    assert!(FROZEN_RENDER_NOTE.to_ascii_lowercase().contains("not"));
    assert_eq!(
        SeriesRole::ElectricPotential.label(),
        "E = -∇V",
        "the overlay reuses the shipped role label"
    );
    let _ = ScalarOverlay::linked(
        vector_field_explorer::scalar_overlay::OverlayId(1),
        "x",
        OverlayLayer::PotentialField,
        "linked",
    );
}

// ---------------------------------------------------------------------------
// Release-audit slice: overlay history, restart and live-input truth
// ---------------------------------------------------------------------------

#[test]
fn adding_and_removing_a_cas_overlay_is_one_document_edit() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.controls_width = 460.0;
    app.layout.cas_expanded = true;
    app.layout.cas_width = 460.0;
    app.state.set_formula("(1, 0)".into());
    app.cas = scalar_panel("x*y");
    for _ in 0..4 {
        idle(&ctx, &mut app);
    }
    let base = app.history_len();

    click_app_button(&ctx, &mut app, "Add linked overlay");
    assert_eq!(app.state.overlays().len(), 1);
    assert_eq!(
        app.history_len(),
        base + 1,
        "adding a CAS overlay is one document edit"
    );
    assert_eq!(app.next_undo_label(), Some("add linked CAS overlay"));
    let id = app.state.overlays().as_slice()[0].id;

    // Remove the same overlay through the panel; the row and its drawing must
    // both be reversible by ordinary Undo, not just the gradient setting.
    click_app_button(&ctx, &mut app, &format!("Remove {id}"));
    assert!(
        app.state.overlays().is_empty(),
        "the overlay row is removed"
    );
    let removed = app.history_len();
    assert!(removed > base, "removal is recorded");
    assert_eq!(app.next_undo_label(), Some("change CAS overlay"));
    assert!(app.undo_document(&ctx));
    assert_eq!(
        app.state.overlays().len(),
        1,
        "one undo restores the overlay row that owned the drawing"
    );
    assert!(app.redo_document(&ctx));
    assert!(
        app.state.overlays().is_empty(),
        "redo removes the overlay row again"
    );
}

#[test]
fn cas_overlays_survive_workspace_settings_without_a_named_scene() {
    let mut state = State::new(false);
    state
        .add_linked_overlay("x*y", OverlayLayer::PotentialField)
        .expect("the overlay is added");
    let record = ViewRecord::capture(&state);

    let mut restored = State::new(false);
    record.restore(&mut restored, &mut Controls::new());
    assert_eq!(
        restored.overlays().len(),
        1,
        "the workspace view record carries CAS overlays"
    );
    assert_eq!(
        restored.overlays().as_slice()[0].formula,
        "x*y",
        "the restored overlay keeps its expression"
    );
}

#[test]
fn a_legacy_view_record_without_overlays_restores_an_empty_set() {
    let mut state = State::new(false);
    state
        .add_linked_overlay("x*y", OverlayLayer::PotentialField)
        .expect("the overlay is added");

    // An older workspace envelope has no `overlays` key at all.
    let mut value = serde_json::to_value(ViewRecord::capture(&state))
        .expect("a view record serializes to JSON");
    value
        .as_object_mut()
        .expect("a view record object")
        .remove("overlays");
    let legacy: ViewRecord =
        serde_json::from_value(value).expect("the legacy record still deserializes");

    assert!(
        legacy.overlays.is_empty(),
        "a missing overlays field defaults to the empty set"
    );
    let mut restored = State::new(false);
    legacy.restore(&mut restored, &mut Controls::new());
    assert!(
        restored.overlays().is_empty(),
        "the restored live set is empty"
    );
}

#[test]
fn a_linked_gradient_keeps_following_the_live_frame_with_no_open_section() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.controls_expanded = false;
    app.sheet.close();
    app.state.set_formula("(0, 0)".into());
    app.state.set_scalar_source(
        vector_field_explorer::scalar::ScalarContext::Surface,
        "x*y".into(),
    );
    let frame = app.state.scalar_view_frame();
    app.state
        .set_gradient(Some(potential_field_spec("x*y".into(), frame).unwrap()))
        .expect("the gradient is valid");
    app.state
        .add_linked_overlay("x*y", OverlayLayer::PotentialField)
        .expect("the linked overlay is added");
    app.state.refresh_scalar_analysis(true);
    assert!(!app.state.scalar_overlay_lines().is_empty());

    // The slice/domain moved after the panel was last open.
    app.state
        .set_domain(glam::Vec3::splat(-3.0), glam::Vec3::splat(3.0));
    let current = app.state.scalar_view_frame();
    assert_ne!(frame, current);
    assert!(
        app.state.scalar_overlay_lines().is_empty(),
        "stale analysis geometry is not painted after the frame changes"
    );

    idle(&ctx, &mut app);
    let gradient = app.state.gradient.as_ref().expect("the gradient stays");
    assert_eq!(
        gradient.frame, current,
        "the installed linked gradient resolves against the live frame"
    );
    assert!(
        !app.state.scalar_overlay_lines().is_empty(),
        "the overlay is refreshed without the Scalar analysis section open"
    );
}

#[test]
fn a_linked_time_dependent_overlay_keeps_the_shared_clock_consumer() {
    let mut state = State::new(false);
    state.set_formula("(0, 0)".into());
    state.set_scalar_source(
        vector_field_explorer::scalar::ScalarContext::Surface,
        "x*y".into(),
    );
    assert!(!state.has_time_dependent_consumer());
    state
        .add_linked_overlay("t*x", OverlayLayer::PotentialField)
        .expect("a time-dependent linked overlay is valid");
    assert!(
        state.has_time_dependent_consumer(),
        "a linked time-dependent overlay consumes the clock"
    );
    state.dirty = false;
    assert!(state.set_time(0.4));
    assert!(
        state.dirty,
        "advancing time invalidates the live overlay geometry"
    );
}

#[test]
fn removing_a_contour_overlay_removes_its_drawing() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = State::new(false);
    state.set_formula("(0, 0)".into());
    let mut panel = scalar_panel("x*y");
    panel
        .apply_scene(&mut state, 2, SceneTarget::Contours)
        .expect("contours are plotted");
    assert_eq!(state.scalar_contour_levels(), 8);
    assert_eq!(state.overlays().len(), 1);
    let id = state.overlays().as_slice()[0].id;
    assert!(!state.scalar_overlay_lines().is_empty());

    let output = render_panel(&ctx, 460.0, &mut state, &mut panel, Vec::new());
    let remove = button_bounds(&output, &format!("Remove {id}"))
        .expect("the contour overlay has a Remove control");
    click(&ctx, 460.0, &mut state, &mut panel, remove.center());

    assert!(state.overlays().is_empty());
    assert_eq!(
        state.scalar_contour_levels(),
        8,
        "removing the CAS layer leaves manual contour levels unchanged"
    );
    assert!(
        state.scalar_overlay_lines().is_empty(),
        "no stale contour segments remain after removal"
    );
}
