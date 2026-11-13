use eframe::egui;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::ui::cas_panel::{self, CasPanelState, Operation, ResultKind, Variable};
use vector_field_explorer::ui::layout;
use vector_field_explorer::ui::style_panel::{self, AppStyle};
use vector_field_explorer::viewmodel::State;

fn eval_scalar(source: &str, x: f64, y: f64) -> f64 {
    let expression = source.parse::<meval::Expr>().unwrap();
    expression
        .eval_with_context([("x", x), ("y", y), ("z", 0.0), ("t", 0.0)])
        .unwrap()
}

struct PanelOutput {
    platform_output: egui::PlatformOutput,
    shapes: Vec<egui::epaint::ClippedShape>,
    panel_rect: egui::Rect,
    clip_rect: egui::Rect,
}

fn panel_frame(
    ctx: &egui::Context,
    width: f32,
    events: Vec<egui::Event>,
    state: &mut State,
    panel: &mut CasPanelState,
) -> PanelOutput {
    let mut panel_rect = egui::Rect::NOTHING;
    let mut clip_rect = egui::Rect::NOTHING;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 1200.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            panel_rect = egui::Panel::left("controls")
                .resizable(true)
                .default_size(width)
                .min_size(layout::MIN_PANEL_WIDTH)
                .max_size(layout::MAX_PANEL_WIDTH)
                .show(ui, |ui| {
                    clip_rect = ui.clip_rect();
                    egui::ScrollArea::vertical().show(ui, |ui| cas_panel::show(ui, state, panel));
                })
                .response
                .rect;
        },
    );
    output.textures_delta.clear();
    PanelOutput {
        platform_output: output.platform_output,
        shapes: output.shapes,
        panel_rect,
        clip_rect,
    }
}

fn click_panel(
    ctx: &egui::Context,
    width: f32,
    pos: egui::Pos2,
    state: &mut State,
    panel: &mut CasPanelState,
) -> PanelOutput {
    panel_frame(
        ctx,
        width,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        state,
        panel,
    );
    panel_frame(
        ctx,
        width,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        state,
        panel,
    )
}

fn open_panel(
    ctx: &egui::Context,
    width: f32,
    state: &mut State,
    panel: &mut CasPanelState,
) -> PanelOutput {
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let output = panel_frame(ctx, width, vec![], state, panel);
    let header =
        labeled_bounds(&output.platform_output, "Symbolic tools (native)").expect("CAS header");
    click_panel(ctx, width, header.center(), state, panel);
    let mut output = panel_frame(ctx, width, vec![], state, panel);
    for _ in 0..3 {
        output = panel_frame(ctx, width, vec![], state, panel);
    }
    output
}

#[test]
fn operation_choices_are_visible_and_clickable_at_supported_widths_and_font_size() {
    let choices = [
        ("Differentiate", Operation::Differentiate),
        ("Gradient", Operation::Gradient),
        ("Divergence", Operation::Divergence),
        ("Curl", Operation::Curl),
    ];
    for (width, font_size) in [
        layout::MIN_PANEL_WIDTH,
        layout::DEFAULT_CONTROLS_WIDTH,
        layout::MAX_PANEL_WIDTH,
    ]
    .into_iter()
    .flat_map(|width| {
        [style_panel::MIN_FONT_SIZE, 13.0, style_panel::MAX_FONT_SIZE].map(|font| (width, font))
    }) {
        let ctx = crate::support::context();
        ctx.enable_accesskit();
        style_panel::apply(
            &ctx,
            &AppStyle {
                font_size,
                ..Default::default()
            },
        );
        let mut app_state = State::new(false);
        let mut panel = CasPanelState::default();
        let mut output = open_panel(&ctx, width, &mut app_state, &mut panel);
        for (label, operation) in choices {
            let combo_rect = output
                .platform_output
                .accesskit_update
                .as_ref()
                .into_iter()
                .flat_map(|update| update.nodes.iter())
                .find_map(|(_, node)| {
                    (node.role() == egui::Role::ComboBox)
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
                .unwrap_or_else(|| panic!("operation selector is not visible at width {width}"));
            assert!(
                output.panel_rect.width() <= layout::MAX_PANEL_WIDTH,
                "CAS exceeds the allowed Controls maximum: {:?}",
                output.panel_rect
            );
            let visible_width = combo_rect.intersect(output.clip_rect);
            assert!(
                visible_width == combo_rect,
                "operation selector bounds {combo_rect:?} escape the {width}-point panel"
            );
            click_panel(&ctx, width, combo_rect.center(), &mut app_state, &mut panel);
            output = panel_frame(&ctx, width, vec![], &mut app_state, &mut panel);
            let mut option_rect = output
                .platform_output
                .accesskit_update
                .as_ref()
                .into_iter()
                .flat_map(|update| update.nodes.iter())
                .find_map(|(_, node)| {
                    (node.label() == Some(label))
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
                .unwrap_or_else(|| {
                    panic!("{label} option is not visible in the selector at width {width}")
                });
            // AccessKit bounds are not clipped to the combo's scroll viewport.
            // Large-font wrapped options can exceed egui's 200-point menu cap;
            // scroll the real popup before clicking, as a user must do.
            let option_clip = |output: &PanelOutput, rect: egui::Rect| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == label
                            && rect.intersects(text.visual_bounding_rect()) =>
                    {
                        Some(shape.clip_rect)
                    }
                    _ => None,
                })
            };
            let clip = option_clip(&output, option_rect).expect("option paint clip");
            if !clip.contains_rect(option_rect) {
                let pos = option_rect.intersect(clip).center();
                panel_frame(
                    &ctx,
                    width,
                    vec![egui::Event::PointerMoved(pos)],
                    &mut app_state,
                    &mut panel,
                );
                output = panel_frame(
                    &ctx,
                    width,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -300.0),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Wheel,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    &mut app_state,
                    &mut panel,
                );
                for _ in 0..3 {
                    output = panel_frame(&ctx, width, vec![], &mut app_state, &mut panel);
                }
                option_rect =
                    labeled_bounds(&output.platform_output, label).expect("scrolled option");
            }
            assert!(
                option_clip(&output, option_rect)
                    .unwrap()
                    .contains_rect(option_rect),
                "{label} must be fully visible before clicking at width {width}, font {font_size}: rect {option_rect:?}, clip {:?}",
                option_clip(&output, option_rect)
            );
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == label
                            && option_rect.intersects(text.visual_bounding_rect()) =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .expect("painted option");
            assert!(
                option_rect
                    .expand(1.0)
                    .contains_rect(text.visual_bounding_rect()),
                "{label} ink must fit its real option hitbox"
            );
            assert!(
                option_rect.height() >= text.galley.size().y + 2.0,
                "{label} needs vertical padding around its rendered content"
            );
            assert_eq!(
                option_rect.intersect(output.panel_rect),
                option_rect,
                "{label} menu option bounds {option_rect:?} escape the {width}-point panel"
            );
            panel_frame(&ctx, width, vec![], &mut app_state, &mut panel);
            output = click_panel(
                &ctx,
                width,
                option_rect.center(),
                &mut app_state,
                &mut panel,
            );
            assert_eq!(
                panel.operation, operation,
                "clicking {label} at width {width}"
            );
        }
    }
}

#[test]
fn scalar_parentheses_are_preserved_and_bare_parenthesized_vector_components_split() {
    let mut panel = CasPanelState::default();
    panel.input = "(x)+(y)".into();
    panel.operation = Operation::Differentiate;
    panel.variable = Variable::X;
    panel.refresh(2);
    let derivative = &panel.preview.as_ref().expect("sum preview").formula;
    assert_eq!(eval_scalar(derivative, 2.0, 3.0), 1.0);

    panel.input = "(x)*(y)".into();
    panel.refresh(2);
    let derivative = &panel.preview.as_ref().expect("product preview").formula;
    assert_eq!(eval_scalar(derivative, 2.0, 3.0), 3.0);

    panel.input = "((x)+(y))*(x+(y))".into();
    panel.refresh(2);
    let derivative = &panel.preview.as_ref().expect("nested preview").formula;
    assert_eq!(eval_scalar(derivative, 2.0, 3.0), 10.0);

    panel.input = "(x), (y)".into();
    panel.operation = Operation::Divergence;
    panel.refresh(2);
    let divergence = &panel.preview.as_ref().expect("bare vector preview").formula;
    assert_eq!(eval_scalar(divergence, 2.0, 3.0), 2.0);

    for (source, expected) in [
        ("(x)+(y)", glam::Vec3::new(1.0, 1.0, 0.0)),
        ("(x)*(y)", glam::Vec3::new(3.0, 2.0, 0.0)),
        ("((x)+(y))*(x+(y))", glam::Vec3::new(10.0, 10.0, 0.0)),
    ] {
        panel.input = source.into();
        panel.operation = Operation::Gradient;
        panel.refresh(2);
        let field = FieldExpr::parse(&panel.preview.as_ref().expect(source).formula).unwrap();
        assert_eq!(field.eval3(glam::Vec3::new(2.0, 3.0, 0.0)), Some(expected));
    }
}

#[test]
fn previews_operations_with_explicit_result_kind_and_copy_payload() {
    let mut panel = CasPanelState::default();
    panel.input = "x*y".into();
    panel.operation = Operation::Differentiate;
    panel.variable = Variable::X;
    panel.refresh(2);
    assert_eq!(panel.preview.as_ref().unwrap().formula, "y");
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Scalar);
    assert_eq!(panel.copy_text(), Some("y"));

    panel.input = "(x, y)".into();
    panel.operation = Operation::Curl;
    panel.refresh(2);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Scalar);
    assert!(!panel.can_apply(2));
    assert_eq!(
        eval_scalar(&panel.preview.as_ref().unwrap().formula, 2.0, 3.0),
        0.0
    );
    panel.input = "(x*y, y*z, z*x)".into();
    panel.refresh(3);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Vector);
    assert_eq!(
        FieldExpr::parse(&panel.preview.as_ref().unwrap().formula)
            .unwrap()
            .eval3(glam::Vec3::new(2.0, 3.0, 4.0)),
        Some(glam::Vec3::new(-3.0, -4.0, -2.0))
    );

    panel.input = "(x*y, x^2)".into();
    panel.operation = Operation::Differentiate;
    panel.variable = Variable::X;
    panel.refresh(2);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Vector);
    assert_eq!(
        FieldExpr::parse(&panel.preview.as_ref().unwrap().formula)
            .unwrap()
            .eval3(glam::Vec3::new(2.0, 3.0, 0.0)),
        Some(glam::Vec3::new(3.0, 4.0, 0.0))
    );

    panel.input = "(x*y, x+y)".into();
    panel.operation = Operation::Divergence;
    panel.refresh(2);
    assert_eq!(panel.preview.as_ref().unwrap().kind, ResultKind::Scalar);
    assert!(!panel.can_apply(2));
    assert_eq!(
        eval_scalar(&panel.preview.as_ref().unwrap().formula, 2.0, 3.0),
        4.0
    );
}

#[derive(Debug, PartialEq)]
struct FieldEvidence {
    formula: String,
    parse_error: String,
    dimension: usize,
    values: Vec<Option<glam::Vec3>>,
    samples: Vec<vector_field_explorer::sampling::ArrowData>,
    rendered_arrows: Vec<u8>,
    dirty: bool,
}

fn evidence(state: &State) -> FieldEvidence {
    FieldEvidence {
        formula: state.formula.clone(),
        parse_error: state.parse_error.clone(),
        dimension: state.field.dimension,
        values: [
            glam::Vec3::ZERO,
            glam::Vec3::new(2.0, 3.0, 4.0),
            glam::Vec3::new(-1.0, 0.5, 2.0),
        ]
        .map(|point| state.field.eval3_at(point, 0.7))
        .to_vec(),
        samples: state.sample(),
        rendered_arrows: bytemuck::cast_slice(state.snapshot().arrows.as_slice()).to_vec(),
        dirty: state.dirty,
    }
}

fn vector_panel(dimension: usize) -> CasPanelState {
    let mut panel = CasPanelState::default();
    panel.operation = Operation::Gradient;
    panel.input = "x*y+z^2+t*x".into();
    panel.refresh(dimension);
    assert!(panel.can_apply(dimension));
    panel
}

#[test]
fn every_request_identity_edit_rejects_stale_apply_and_recomputes() {
    // Omitting any identity field lets an old successful vector mutate the field.
    for edit in 0..3 {
        for dirty in [false, true] {
            let mut state = State::new(false);
            state.dirty = dirty;
            let before = evidence(&state);
            let mut panel = vector_panel(2);
            match edit {
                0 => panel.input = "x^2+y^2".into(),
                1 => panel.operation = Operation::Differentiate,
                2 => panel.variable = Variable::T,
                _ => unreachable!(),
            }
            assert!(!panel.can_apply(2), "identity edit {edit}");
            assert!(!panel.apply(&mut state, 2), "identity edit {edit}");
            assert_eq!(evidence(&state), before);
            panel.refresh(2);
            let preview = panel.preview.as_ref().unwrap();
            if edit == 1 {
                assert_eq!(preview.kind, ResultKind::Scalar);
                assert_eq!(eval_scalar(&preview.formula, 2.0, 3.0), 3.0);
            } else {
                assert!(panel.can_apply(2));
            }
            assert_eq!(evidence(&state), before);
        }
    }
}

#[test]
fn authoritative_mode_not_field_arity_or_caller_dimension_controls_apply() {
    let mut state = State::new(false);
    state.set_mode(true);
    assert_eq!(state.field.dimension, 2);
    assert!(!state.is_3d()); // This sampling helper is NOT the authoritative mode.
    let before = evidence(&state);
    let mut panel = vector_panel(2);
    for dimension in [2, 3] {
        assert!(!panel.apply(&mut state, dimension));
        assert_eq!(evidence(&state), before);
    }
    // Even a structurally valid replacement payload cannot rescue an old
    // request from the previous dimension.
    panel.preview.as_mut().unwrap().formula = "(x, y, z)".into();
    assert!(!panel.apply(&mut state, 3));
    assert_eq!(evidence(&state), before);
    panel.refresh(3);
    assert!(panel.can_apply(3));
    assert!(!panel.apply(&mut state, 2));
    assert_eq!(evidence(&state), before);
    assert!(panel.apply(&mut state, 3));
    assert_eq!(state.field.dimension, 3);
    assert_eq!(
        state.field.eval3_at(glam::Vec3::new(2.0, 3.0, 4.0), 0.7),
        Some(glam::Vec3::new(3.7, 2.0, 8.0))
    );

    state.set_mode(false);
    let before = evidence(&state);
    assert_eq!(state.field.dimension, 3);
    assert!(!panel.apply(&mut state, 3));
    assert!(!panel.apply(&mut state, 2));
    assert_eq!(evidence(&state), before);
}

#[test]
fn malformed_and_wrong_arity_previews_never_reach_the_mutating_setter() {
    for formula in ["(x,)", "(x, sin())", "(x, y, z)"] {
        for dirty in [false, true] {
            let mut state = State::new(false);
            // An existing parse error and its last valid plotted field must survive too.
            state.set_formula("(x +, y)".into());
            state.dirty = dirty;
            let before = evidence(&state);
            let mut panel = vector_panel(2);
            panel.preview.as_mut().unwrap().formula = formula.into();
            assert!(!panel.can_apply(2), "{formula}");
            assert!(!panel.apply(&mut state, 2), "{formula}");
            assert_eq!(evidence(&state), before, "{formula}");
        }
    }
}

#[test]
fn failed_recomputation_invalid_shapes_and_unsupported_input_preserve_the_plot() {
    for (input, operation, dimension) in [
        ("x +", Operation::Gradient, 2),
        ("abs(x)", Operation::Gradient, 2),
        ("mystery(x)", Operation::Differentiate, 2),
        ("(x, y)", Operation::Gradient, 2),
        ("x", Operation::Divergence, 2),
        ("(x, y, z)", Operation::Curl, 2),
        ("(x, y)", Operation::Differentiate, 3),
        ("x, y, z, t", Operation::Differentiate, 2),
    ] {
        let mut state = State::new(dimension == 3);
        state.dirty = false;
        let before = evidence(&state);
        let mut panel = vector_panel(dimension);
        panel.input = input.into();
        panel.operation = operation;
        panel.refresh(dimension);
        assert!(panel.preview.is_none(), "{input} {operation:?}");
        assert!(panel.error.as_ref().is_some_and(|error| !error.is_empty()));
        assert!(!panel.can_apply(dimension));
        assert!(!panel.apply(&mut state, dimension));
        assert_eq!(evidence(&state), before);
    }
}

#[test]
fn scalar_and_reset_scratch_never_change_the_plot() {
    let mut state = State::new(false);
    state.dirty = false;
    let before = evidence(&state);
    let mut panel = CasPanelState::default();
    panel.input = "(x, x*y)".into();
    panel.operation = Operation::Curl;
    panel.refresh(2);
    assert_eq!(eval_scalar(panel.copy_text().unwrap(), 2.0, 3.0), 3.0);
    assert!(!panel.apply(&mut state, 2));
    assert_eq!(evidence(&state), before);
    panel.variable = Variable::T;
    panel.reset();
    assert_eq!(panel.input, "x");
    assert_eq!(panel.operation, Operation::Differentiate);
    assert_eq!(panel.variable, Variable::X);
    assert!(panel.preview.is_none());
    assert!(panel.error.is_none());
    assert_eq!(evidence(&state), before);
    panel.refresh(2);
    assert_eq!(eval_scalar(panel.copy_text().unwrap(), 2.0, 3.0), 1.0);
}

fn button_bounds(output: &egui::PlatformOutput, label: &str) -> Option<egui::Rect> {
    output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.role() == egui::Role::Button && node.label() == Some(label))
                .then(|| {
                    node.bounds().map(|r| {
                        egui::Rect::from_min_max(
                            egui::pos2(r.x0 as f32, r.y0 as f32),
                            egui::pos2(r.x1 as f32, r.y1 as f32),
                        )
                    })
                })
                .flatten()
        })
}

fn labeled_bounds(output: &egui::PlatformOutput, label: &str) -> Option<egui::Rect> {
    output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.label() == Some(label))
                .then(|| {
                    node.bounds().map(|r| {
                        egui::Rect::from_min_max(
                            egui::pos2(r.x0 as f32, r.y0 as f32),
                            egui::pos2(r.x1 as f32, r.y1 as f32),
                        )
                    })
                })
                .flatten()
        })
}

fn has_text(output: &egui::PlatformOutput, text: &str) -> bool {
    output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .any(|(_, node)| {
            [node.label(), node.value()]
                .into_iter()
                .flatten()
                .any(|value| value.contains(text))
        })
}

#[test]
fn actual_section_copies_scalar_has_no_apply_and_shows_inline_failure() {
    let ctx = crate::support::context();
    ctx.enable_accesskit();
    let mut state = State::new(false);
    state.dirty = false;
    let before = evidence(&state);
    let mut panel = CasPanelState::default();
    panel.input = "(x, x*y)".into();
    panel.operation = Operation::Curl;
    let output = open_panel(&ctx, 460.0, &mut state, &mut panel);
    assert!(has_text(&output.platform_output, "Result — Scalar"));
    assert!(button_bounds(&output.platform_output, "Apply vector").is_none());
    let copy = button_bounds(&output.platform_output, "Copy").expect("actual Copy button");
    let output = click_panel(&ctx, 460.0, copy.center(), &mut state, &mut panel);
    assert!(
        output.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text == "(y-0)")
        }),
        "Copy must emit the actual scalar preview {:?} to the clipboard: {:?}",
        panel.copy_text(),
        output.platform_output.commands
    );
    assert_eq!(evidence(&state), before);

    panel.input = "abs(x)".into();
    panel.operation = Operation::Gradient;
    let output = panel_frame(&ctx, 460.0, vec![], &mut state, &mut panel);
    assert!(has_text(&output.platform_output, "Error:"));
    assert!(button_bounds(&output.platform_output, "Apply vector").is_none());
    assert!(button_bounds(&output.platform_output, "Copy").is_none());
    assert_eq!(evidence(&state), before);
    let reset = button_bounds(&output.platform_output, "Reset").unwrap();
    click_panel(&ctx, 460.0, reset.center(), &mut state, &mut panel);
    assert_eq!(panel.input, "x");
    assert!(panel.error.is_none());
    assert_eq!(evidence(&state), before);
}

fn app_frame(
    app: &mut vector_field_explorer::VfApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::PlatformOutput {
    let mut frame = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 6000.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut frame),
    );
    output.textures_delta.clear();
    output.platform_output
}

#[test]
fn real_app_sidebar_applies_only_on_click_and_formula_bar_follows() {
    let ctx = crate::support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = vector_field_explorer::VfApp::new();
    app.layout.controls_width = 460.0;
    app.controls.cas = vector_panel(2);
    let original = app.state.formula.clone();
    let mut output = app_frame(&mut app, &ctx, vec![]);
    for _ in 0..3 {
        output = app_frame(&mut app, &ctx, vec![]);
    }
    let pos = labeled_bounds(&output, "Symbolic tools (native)")
        .expect("app sidebar CAS header")
        .center();
    app_frame(
        &mut app,
        &ctx,
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
    app_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    for _ in 0..3 {
        output = app_frame(&mut app, &ctx, vec![]);
    }
    assert_eq!(app.state.formula, original, "preview must never auto-apply");
    assert!(
        has_text(&output, "Result — Vector"),
        "the real app must draw the CAS section"
    );
    let apply = button_bounds(&output, "Apply vector").expect("sidebar Apply widget");
    assert!(apply.center().y < 6000.0, "Apply must be on-screen");
    let pos = apply.center();
    app_frame(
        &mut app,
        &ctx,
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
    app_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_ne!(
        app.state.formula, original,
        "actual Apply must invoke the action"
    );
    assert_eq!(
        app.state
            .field
            .eval3_at(glam::Vec3::new(2.0, 3.0, 4.0), 0.7),
        Some(glam::Vec3::new(3.7, 2.0, 0.0))
    );
    assert!(app.state.parse_error.is_empty());
    let applied_formula = app.state.formula.clone();
    app_frame(&mut app, &ctx, vec![]);
    assert_eq!(
        app.state.formula, applied_formula,
        "the formula bar must not undo Apply"
    );
    assert_eq!(
        app.bar.text, applied_formula,
        "next-frame formula-bar synchronization"
    );
    assert_eq!(app.controls.cas.input, "x*y+z^2+t*x");

    app.controls.cas.variable = Variable::T;
    app.controls.cas.input = "abs(x)".into();
    app.controls.cas.refresh(2);
    assert!(app.controls.cas.error.is_some());

    // Save/reload uses the real app envelope; ordinary applied field persists,
    // while scratch input/operation/preview/error never does.
    #[derive(Default)]
    struct Storage(std::collections::HashMap<String, String>);
    impl eframe::Storage for Storage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }
    let mut storage = Storage::default();
    eframe::App::save(&mut app, &mut storage);
    let loaded = vector_field_explorer::VfApp::new_with_storage(Some(&storage));
    assert_eq!(loaded.state.formula, app.state.formula);
    assert_eq!(loaded.controls.cas.input, "x");
    assert_eq!(loaded.controls.cas.operation, Operation::Differentiate);
    assert_eq!(loaded.controls.cas.variable, Variable::X);
    assert!(loaded.controls.cas.preview.is_none());
    assert!(loaded.controls.cas.error.is_none());
}

#[test]
fn only_explicit_current_vector_apply_mutates_the_field() {
    let mut panel = CasPanelState::default();
    let mut state = State::new(false);
    state.snapshot_if_dirty();
    let original = state.formula.clone();
    panel.input = "x*y".into();
    panel.operation = Operation::Gradient;
    panel.refresh(2);
    assert_eq!(state.formula, original);
    assert!(panel.apply(&mut state, 2));
    assert_eq!(state.formula, "(y, x)");
    assert_eq!(FieldExpr::parse(&state.formula).unwrap().dimension, 2);
    assert!(state.snapshot_if_dirty().is_some());

    panel.input = "x".into();
    panel.operation = Operation::Differentiate;
    panel.refresh(2);
    assert!(!panel.can_apply(2));
    let before = evidence(&state);
    assert!(!panel.apply(&mut state, 2));
    assert_eq!(evidence(&state), before);
}

#[test]
fn edits_mode_switch_and_failures_invalidate_apply_without_mutation() {
    let mut panel = CasPanelState::default();
    let mut state = State::new(false);
    state.snapshot_if_dirty();
    let original = state.formula.clone();
    let before = evidence(&state);
    panel.input = "x".into();
    panel.operation = Operation::Gradient;
    panel.refresh(2);
    assert!(panel.can_apply(2));
    panel.input.push_str(" +");
    assert!(!panel.can_apply(2));
    panel.refresh(2);
    assert!(panel.error.is_some());
    assert!(!panel.apply(&mut state, 2));
    assert_eq!(state.formula, original);
    assert_eq!(evidence(&state), before);
    panel.input = "x".into();
    panel.refresh(2);
    assert!(!panel.can_apply(3));
    assert!(!panel.apply(&mut state, 3));
    assert_eq!(state.formula, original);
    assert_eq!(evidence(&state), before);
    assert!(state.snapshot_if_dirty().is_none());
}
