//! Issue #273: the in-context Help / Glossary beside `Interpret a prompt`.
//!
//! The glossary is a stable vocabulary table rendered inside the session-only
//! prompt section. These tests drive the real pointer and keyboard paths, keep
//! the typed prompt and parsed model across opening and closing, and check every
//! advertised example against the shipped `prompt_model::parse_prompt` grammar.

use eframe::egui;

use vector_field_explorer::VfApp;
use vector_field_explorer::prompt_model;
use vector_field_explorer::ui::prompt_help::{self, ExampleKind};
use vector_field_explorer::ui::prompt_model::{self as prompt_model_ui, PromptModelState};

use crate::support;

const DIPOLE: &str = "two equal and opposite charges separated by d along x";
const WIDE: f32 = 980.0;
const COMPACT: f32 = 280.0;

fn parsed_state() -> PromptModelState {
    let mut state = PromptModelState {
        input: DIPOLE.into(),
        ..PromptModelState::default()
    };
    state.parse();
    state
}

fn frame(
    ctx: &egui::Context,
    state: &mut PromptModelState,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 6000.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            let _ = prompt_model_ui::show(ui, state);
        },
    );
    output.textures_delta.clear();
    output
}

fn idle(ctx: &egui::Context, state: &mut PromptModelState, width: f32) -> egui::FullOutput {
    for _ in 0..3 {
        let _ = frame(ctx, state, width, Vec::new());
    }
    frame(ctx, state, width, Vec::new())
}

fn click(
    ctx: &egui::Context,
    state: &mut PromptModelState,
    width: f32,
    pos: egui::Pos2,
) -> egui::FullOutput {
    let _ = frame(
        ctx,
        state,
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
    );
    frame(
        ctx,
        state,
        width,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )
}

fn key_event(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn button_node(
    output: &egui::FullOutput,
    label: &str,
) -> Option<(egui::accesskit::NodeId, egui::Rect)> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(id, node)| {
        if node.role() != egui::Role::Button || node.label() != Some(label) {
            return None;
        }
        let bounds = node.bounds()?;
        let rect = egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        );
        Some((*id, rect))
    })
}

fn button_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    button_node(output, label).map(|(_, rect)| rect)
}

fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => {
                Some((text.galley.text().to_owned(), text.visual_bounding_rect()))
            }
            _ => None,
        })
        .collect()
}

fn painted(output: &egui::FullOutput, needle: &str) -> bool {
    texts(output).iter().any(|(text, _)| text.contains(needle))
}

#[test]
fn glossary_data_is_complete_and_names_the_shipped_grammar_topics() {
    assert!(!prompt_help::GLOSSARY_SECTIONS.is_empty());
    let mut corpus = String::new();
    for section in prompt_help::GLOSSARY_SECTIONS {
        assert!(!section.title.trim().is_empty());
        assert!(!section.intro.trim().is_empty());
        assert!(
            !section.entries.is_empty(),
            "{} has no terms",
            section.title
        );
        corpus.push_str(section.title);
        corpus.push(' ');
        corpus.push_str(section.intro);
        corpus.push(' ');
        for entry in section.entries {
            assert!(!entry.term.trim().is_empty(), "a term has no label");
            assert!(
                !entry.detail.trim().is_empty(),
                "`{}` has no content",
                entry.term
            );
            corpus.push_str(entry.term);
            corpus.push(' ');
            corpus.push_str(entry.detail);
            corpus.push(' ');
        }
    }
    for example in prompt_help::ACCEPTED_EXAMPLES {
        assert!(!example.prompt.trim().is_empty());
        assert!(!example.meaning.trim().is_empty());
        corpus.push_str(example.prompt);
        corpus.push(' ');
        corpus.push_str(example.meaning);
        corpus.push(' ');
    }
    for example in prompt_help::REJECTED_EXAMPLES {
        assert!(!example.prompt.trim().is_empty());
        assert!(!example.reason.trim().is_empty());
        corpus.push_str(example.prompt);
        corpus.push(' ');
        corpus.push_str(example.reason);
        corpus.push(' ');
    }
    let corpus = corpus.to_lowercase();
    for required in [
        "point charge",
        "dipole",
        "origin",
        "sign",
        "positive end",
        "axis",
        "separation",
        "cartesian",
        "normalized",
        "ideal",
        "softened",
        "question",
        "unsupported",
        "formula",
        "apply",
        "cancel",
        "units",
    ] {
        assert!(
            corpus.contains(required),
            "the glossary must explain `{required}`"
        );
    }
}

#[test]
fn every_accepted_example_parses_through_the_shipped_grammar() {
    let mut point = false;
    let mut dipole = false;
    for example in prompt_help::ACCEPTED_EXAMPLES {
        let model = prompt_model::parse_prompt(example.prompt)
            .unwrap_or_else(|error| panic!("`{}` must parse, got {error:?}", example.prompt));
        assert!(
            model.limitations.is_empty(),
            "`{}` is advertised as accepted but carries {:?}",
            example.prompt,
            model.limitations
        );
        match example.kind {
            ExampleKind::PointCharge => {
                point = true;
                assert_eq!(
                    model.sources.len(),
                    1,
                    "`{}` should be one point charge",
                    example.prompt
                );
                assert!(
                    model.dipole.is_none(),
                    "`{}` is not a dipole",
                    example.prompt
                );
            }
            ExampleKind::Dipole => {
                dipole = true;
                assert_eq!(
                    model.sources.len(),
                    2,
                    "`{}` should be two charges",
                    example.prompt
                );
                assert!(
                    model.dipole.is_some(),
                    "`{}` should carry dipole geometry",
                    example.prompt
                );
            }
        }
        assert!(
            model.can_apply() || !model.ambiguities.is_empty(),
            "`{}` is either ready to apply or asks a concrete question",
            example.prompt
        );
    }
    assert!(point, "the glossary advertises a point-charge example");
    assert!(dipole, "the glossary advertises a dipole example");
}

#[test]
fn every_rejected_example_fails_for_the_stated_reason() {
    for example in prompt_help::REJECTED_EXAMPLES {
        let model = prompt_model::parse_prompt(example.prompt).unwrap_or_else(|error| {
            panic!(
                "unsupported prompts are parsed into a stated limitation, got {error:?} for `{}`",
                example.prompt
            )
        });
        assert!(
            model.sources.is_empty(),
            "`{}` must not become a guessed source",
            example.prompt
        );
        assert!(
            !model.limitations.is_empty(),
            "`{}` must state why it is unsupported",
            example.prompt
        );
        assert!(
            model.limitations.join(" ").contains(example.reason),
            "`{}` should fail for `{}`, got {:?}",
            example.prompt,
            example.reason,
            model.limitations
        );
        assert!(!model.can_apply(), "`{}` cannot apply", example.prompt);
        assert!(
            model.formula_path().to_lowercase().contains("formula"),
            "`{}` points at the formula path",
            example.prompt
        );
    }
}

#[test]
fn unsupported_distribution_terms_only_appear_as_rejected_examples() {
    for term in prompt_help::UNSUPPORTED_DISTRIBUTION_TERMS {
        assert!(
            prompt_help::REJECTED_EXAMPLES
                .iter()
                .any(|example| example.prompt.contains(term)),
            "`{term}` needs a rejected example"
        );
        assert!(
            !prompt_help::ACCEPTED_EXAMPLES
                .iter()
                .any(|example| example.prompt.contains(term)),
            "`{term}` must not be advertised as accepted"
        );
    }
}

#[test]
fn help_opens_and_closes_by_pointer_while_preserving_the_prompt_and_model() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = parsed_state();
    state.error = Some("kept error".into());
    state.message = Some("kept message".into());
    let before = state.clone();

    let output = idle(&ctx, &mut state, WIDE);
    let toggle =
        button_rect(&output, prompt_help::TOGGLE_LABEL).expect("the Help / Glossary control");
    let output = click(&ctx, &mut state, WIDE, toggle.center());

    assert!(state.help_open, "the pointer opens the glossary");
    assert!(
        painted(&output, prompt_help::TITLE),
        "the open glossary paints its title"
    );
    assert!(
        painted(&output, prompt_help::CLOSE_LABEL),
        "the open glossary paints a Close control"
    );
    assert_eq!(state.input, before.input, "opening keeps the typed prompt");
    assert_eq!(state.model, before.model, "opening keeps the parsed model");
    assert_eq!(state.error, before.error, "opening keeps the error");
    assert_eq!(state.message, before.message, "opening keeps the message");

    let close = button_rect(&output, prompt_help::CLOSE_LABEL).expect("the Close control");
    let _ = click(&ctx, &mut state, WIDE, close.center());
    let output = idle(&ctx, &mut state, WIDE);

    assert!(!state.help_open, "Close dismisses the glossary");
    assert!(
        !painted(&output, prompt_help::TITLE),
        "the closed glossary no longer paints"
    );
    assert_eq!(state.input, before.input, "closing keeps the typed prompt");
    assert_eq!(state.model, before.model, "closing keeps the parsed model");
    assert_eq!(state.error, before.error, "closing keeps the error");
    assert_eq!(state.message, before.message, "closing keeps the message");
}

#[test]
fn escape_dismisses_the_help_and_keeps_the_prompt() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = parsed_state();
    state.help_open = true;
    let before = state.clone();
    let output = idle(&ctx, &mut state, WIDE);
    assert!(
        painted(&output, prompt_help::TITLE),
        "the fixture starts with the glossary open"
    );

    let output = frame(&ctx, &mut state, WIDE, vec![key_event(egui::Key::Escape)]);

    assert!(!state.help_open, "Escape dismisses the glossary");
    assert!(
        !painted(&output, prompt_help::TITLE),
        "the dismissed glossary no longer paints"
    );
    assert_eq!(state.input, before.input);
    assert_eq!(state.model, before.model);
    assert_eq!(state.error, before.error);
    assert_eq!(state.message, before.message);
}

#[test]
fn keyboard_focus_and_space_open_the_help() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut state = parsed_state();

    let output = idle(&ctx, &mut state, WIDE);
    let (node, _) =
        button_node(&output, prompt_help::TOGGLE_LABEL).expect("the Help / Glossary control");

    let _ = frame(
        &ctx,
        &mut state,
        WIDE,
        vec![egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Focus,
                target_node: node,
                target_tree: egui::accesskit::TreeId::ROOT,
                data: None,
            },
        )],
    );
    assert!(
        ctx.memory(|memory| memory.focused()).is_some(),
        "the Help control accepts keyboard focus"
    );

    let _ = frame(&ctx, &mut state, WIDE, vec![key_event(egui::Key::Space)]);
    assert!(
        state.help_open,
        "Space activates the focused Help control and focuses it visibly via the active widget style"
    );

    let _ = frame(&ctx, &mut state, WIDE, vec![key_event(egui::Key::Escape)]);
    assert!(!state.help_open, "Escape still dismisses the help");
}

#[test]
fn the_glossary_stays_inside_compact_and_wide_layouts() {
    for width in [COMPACT, WIDE] {
        let ctx = support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut state = parsed_state();
        state.help_open = true;

        let output = idle(&ctx, &mut state, width);
        for needle in [
            prompt_help::TITLE,
            "Point charge at the origin",
            "Dipole axis",
            "Normalized teaching units",
            "Softened evaluation",
            "Formula path",
            "Apply",
            "Cancel",
        ] {
            assert!(painted(&output, needle), "width {width} paints `{needle}`");
        }
        for (text, rect) in texts(&output) {
            // The single-line capture hint is a pre-existing clipped TextEdit
            // hint, not glossary content; the help itself must wrap.
            if text.contains("two equal and opposite charges separated by d along x") {
                continue;
            }
            assert!(
                rect.max.x <= width + 0.5,
                "at width {width}, `{text}` overflows to {}",
                rect.max.x
            );
        }
    }
}

fn app_frame(ctx: &egui::Context, app: &mut VfApp, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 6000.0),
            )),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn app_click(ctx: &egui::Context, app: &mut VfApp, pos: egui::Pos2) -> egui::FullOutput {
    let _ = app_frame(
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
    app_frame(
        ctx,
        app,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )
}

fn stored_state(ctx: &egui::Context) -> PromptModelState {
    ctx.data_mut(|data| {
        data.get_temp::<PromptModelState>(prompt_model_ui::state_id())
            .expect("the prompt section stored its session state")
    })
}

#[test]
fn opening_and_closing_help_in_the_cas_panel_leaves_the_document_unchanged() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.cas_expanded = true;
    app.layout.controls_width = 460.0;
    app.layout.cas_width = 460.0;

    let seeded = parsed_state();
    ctx.data_mut(|data| data.insert_temp(prompt_model_ui::state_id(), seeded.clone()));

    let mut output = app_frame(&ctx, &mut app, Vec::new());
    for _ in 0..3 {
        output = app_frame(&ctx, &mut app, Vec::new());
    }
    let document = app.capture_document_for_test();
    let history = app.history_len();

    let toggle = button_rect(&output, prompt_help::TOGGLE_LABEL)
        .expect("the CAS panel renders the Help / Glossary control");
    let output = app_click(&ctx, &mut app, toggle.center());
    let state = stored_state(&ctx);
    assert!(state.help_open, "the in-panel control opens the glossary");
    assert!(painted(&output, prompt_help::TITLE));
    assert_eq!(state.input, seeded.input);
    assert_eq!(state.model, seeded.model);
    assert_eq!(app.capture_document_for_test(), document);
    assert_eq!(app.history_len(), history, "opening help is not an edit");

    let close = button_rect(&output, prompt_help::CLOSE_LABEL).expect("the Close control");
    let _ = app_click(&ctx, &mut app, close.center());
    let state = stored_state(&ctx);
    assert!(!state.help_open, "Close dismisses the in-panel glossary");
    assert_eq!(state.input, seeded.input);
    assert_eq!(state.model, seeded.model);
    assert_eq!(app.capture_document_for_test(), document);
    assert_eq!(app.history_len(), history, "closing help is not an edit");
}

#[test]
fn escape_in_the_cas_panel_dismisses_help_and_leaves_the_document_unchanged() {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = VfApp::new();
    app.layout.help_visible = false;
    app.layout.cas_expanded = true;
    app.layout.controls_width = 460.0;
    app.layout.cas_width = 460.0;

    let seeded = parsed_state();
    ctx.data_mut(|data| data.insert_temp(prompt_model_ui::state_id(), seeded.clone()));

    let mut output = app_frame(&ctx, &mut app, Vec::new());
    for _ in 0..3 {
        output = app_frame(&ctx, &mut app, Vec::new());
    }
    let document = app.capture_document_for_test();
    let history = app.history_len();

    let toggle = button_rect(&output, prompt_help::TOGGLE_LABEL)
        .expect("the CAS panel renders the Help / Glossary control");
    let output = app_click(&ctx, &mut app, toggle.center());
    assert!(
        stored_state(&ctx).help_open,
        "the in-panel control opens the glossary before Escape arrives"
    );
    assert!(painted(&output, prompt_help::TITLE));

    let output = app_frame(&ctx, &mut app, vec![key_event(egui::Key::Escape)]);
    let state = stored_state(&ctx);
    assert!(
        !state.help_open,
        "Escape through the real app frame dismisses the in-panel glossary"
    );
    assert!(
        !painted(&output, prompt_help::TITLE),
        "the dismissed glossary no longer paints"
    );
    assert_eq!(state.input, seeded.input);
    assert_eq!(state.model, seeded.model);
    assert_eq!(app.capture_document_for_test(), document);
    assert_eq!(app.history_len(), history, "dismissing help is not an edit");
}
