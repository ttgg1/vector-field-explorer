//! Issue #213: interpret supported electrostatics homework prompts as editable
//! physical models.
//!
//! The parser and preview are pure; the application tests drive the real
//! `VfApp` through the same ordinary scene setters the UI uses. The grammar is
//! deliberately offline: no model call, no hidden solver and no second physics
//! model.

use eframe::egui;
use glam::DVec3;

use vector_field_explorer::VfApp;
use vector_field_explorer::prompt_model::{
    self, Axis, DipoleEnd, PromptModelError, Question, SourceRole,
};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::prompt_model::{
    self as prompt_model_ui, PromptModelAction, PromptModelState,
};

use crate::support;

const DIPOLE: &str = "two equal and opposite charges separated by d along x";
const POINT: &str = "a positive point charge at the origin";
const POINT_FIELD: &str = "field of a positive point charge at the origin";

fn dipole_position(model: &prompt_model::PromptModel, role: SourceRole) -> DVec3 {
    model
        .sources
        .iter()
        .find(|source| source.role == role)
        .unwrap_or_else(|| panic!("missing {role:?} source"))
        .position
}

#[test]
fn parses_positive_point_charge_at_the_origin() {
    let model = prompt_model::parse_prompt(POINT).expect("the point-charge example parses");
    assert_eq!(model.sources.len(), 1);
    let source = &model.sources[0];
    assert_eq!(source.role, SourceRole::Point);
    assert_eq!(source.sign, 1);
    assert_eq!(source.position, DVec3::ZERO);
    assert_eq!(source.magnitude.value, 1.0);
    assert_eq!(
        source.magnitude.symbol.as_deref(),
        Some("q"),
        "an unspecified magnitude stays symbolic, labelled with its normalized default"
    );
    assert!(source.magnitude.is_normalized_default());
    assert!(model.requested.is_empty(), "no quantity was asked for");
    assert!(model.ambiguities.is_empty());
    assert!(model.limitations.is_empty());
    assert!(model.can_apply());
}

#[test]
fn parses_the_field_request_of_the_point_charge_example() {
    let model = prompt_model::parse_prompt(POINT_FIELD).expect("the field example parses");
    assert_eq!(model.sources.len(), 1);
    assert_eq!(model.requested, vec![prompt_model::Quantity::Field]);
    assert!(prompt_model::preview(&model).requested[0].contains("field"));
}

#[test]
fn parses_the_equal_and_opposite_dipole_example_and_asks_the_missing_orientation() {
    let model = prompt_model::parse_prompt(DIPOLE).expect("the dipole example parses");
    assert_eq!(model.sources.len(), 2);
    let axis = model.dipole.as_ref().expect("dipole geometry").axis;
    assert_eq!(axis, Axis::X);
    assert_eq!(
        model.ambiguities,
        vec![Question::DipoleOrientation { axis: Axis::X }],
        "the unspecified positive end is a concrete question, not a silent default"
    );
    assert!(!model.can_apply());
    assert!(!prompt_model::preview(&model).can_apply);
    assert!(prompt_model::preview(&model).questions[0].contains("positive"));
}

#[test]
fn choosing_the_positive_end_updates_positions_and_clears_the_question() {
    let mut model = prompt_model::parse_prompt(DIPOLE).expect("dipole");
    let question = model.ambiguities[0].clone();
    assert!(model.resolve_question(&question, DipoleEnd::NegativeAxis));
    assert!(model.ambiguities.is_empty());
    assert!(model.can_apply());
    assert_eq!(
        dipole_position(&model, SourceRole::DipolePositive),
        DVec3::new(-0.5, 0.0, 0.0)
    );
    assert_eq!(
        dipole_position(&model, SourceRole::DipoleNegative),
        DVec3::new(0.5, 0.0, 0.0)
    );
    assert!(!model.resolve_question(&question, DipoleEnd::NegativeAxis));
}

#[test]
fn parses_dipole_paraphrases_with_a_different_word_order_and_axis_spelling() {
    for prompt in [
        "two opposite equal charges at a separation d along the x axis",
        "a pair of equal and opposite point charges separated by d along the x-axis",
        "two charges separated by d along x, equal and opposite",
    ] {
        let model = prompt_model::parse_prompt(prompt).unwrap_or_else(|error| {
            panic!("`{prompt}` should parse, got {error:?}");
        });
        assert_eq!(model.sources.len(), 2, "{prompt}");
        assert_eq!(model.dipole.as_ref().unwrap().axis, Axis::X, "{prompt}");
        assert_eq!(
            model.ambiguities,
            vec![Question::DipoleOrientation { axis: Axis::X }],
            "{prompt}"
        );
    }
}

#[test]
fn parses_point_charge_paraphrases() {
    for prompt in [
        "a point charge with positive sign at the origin",
        "positive charge at the origin",
        "a positive charge at origin",
    ] {
        let model = prompt_model::parse_prompt(prompt)
            .unwrap_or_else(|error| panic!("`{prompt}` should parse, got {error:?}"));
        assert_eq!(model.sources.len(), 1, "{prompt}");
        assert_eq!(model.sources[0].sign, 1, "{prompt}");
        assert_eq!(model.sources[0].position, DVec3::ZERO, "{prompt}");
    }
}

#[test]
fn reversed_signs_reverse_the_model_instead_of_normalizing_away() {
    let negative = prompt_model::parse_prompt("a negative point charge at the origin").unwrap();
    assert_eq!(negative.sources[0].sign, -1);
    assert_eq!(negative.sources[0].charge(), -1.0);

    let reversed = prompt_model::parse_prompt(
        "two equal and opposite charges with the positive charge at x = -d/2, separated by d along x",
    )
    .expect("the explicit placement parses");
    assert!(
        reversed.ambiguities.is_empty(),
        "an explicit placement is not ambiguous"
    );
    assert_eq!(
        dipole_position(&reversed, SourceRole::DipolePositive),
        DVec3::new(-0.5, 0.0, 0.0)
    );
    assert_eq!(
        dipole_position(&reversed, SourceRole::DipoleNegative),
        DVec3::new(0.5, 0.0, 0.0)
    );
    assert_eq!(reversed.sources[0].charge(), 1.0);
    assert_eq!(reversed.sources[1].charge(), -1.0);
}

#[test]
fn unspecified_magnitudes_stay_symbolic_with_editable_normalized_defaults() {
    let model = prompt_model::parse_prompt(DIPOLE).unwrap();
    for source in &model.sources {
        assert_eq!(source.magnitude.symbol.as_deref(), Some("q"));
        assert_eq!(source.magnitude.value, 1.0);
        assert!(source.magnitude.is_normalized_default());
        assert!(source.magnitude.label().contains("normalized"));
    }

    let explicit = prompt_model::parse_prompt("a positive point charge at the origin with q = 2")
        .expect("an explicit magnitude parses");
    assert_eq!(explicit.sources[0].magnitude.value, 2.0);
    assert!(!explicit.sources[0].magnitude.is_normalized_default());
}

#[test]
fn preview_shows_equations_scene_frame_units_and_softened_convention() {
    let model = prompt_model::parse_prompt(DIPOLE).unwrap();
    let preview = prompt_model::preview(&model);
    assert!(preview.equations.iter().any(|line| line.contains("E(r)")));
    assert!(preview.equations.iter().any(|line| line.contains("V(r)")));
    assert!(preview.scene_summary.contains("0.5"));
    assert!(preview.frame.contains("Cartesian"));
    assert!(preview.frame.contains("+x"));
    assert!(preview.units.contains("Normalized"));
    assert!(preview.units.contains("not SI"));
    assert!(preview.convention.contains("softened"));
    assert!(preview.convention.contains("ideal"));
    assert!(
        !model.convention.grades_against_ideal(),
        "a softened sample is never graded against the ideal peak"
    );
}

#[test]
fn editing_the_separation_updates_positions_potential_and_field_from_one_model() {
    let mut model = prompt_model::parse_prompt(DIPOLE).unwrap();
    let question = model.ambiguities[0].clone();
    assert!(model.resolve_question(&question, DipoleEnd::PositiveAxis));
    let before = prompt_model::preview(&model);

    model
        .set_separation(2.0)
        .expect("a finite positive separation is editable");
    assert_eq!(model.dipole.as_ref().unwrap().separation.value, 2.0);
    assert_eq!(
        dipole_position(&model, SourceRole::DipolePositive),
        DVec3::new(1.0, 0.0, 0.0)
    );
    assert_eq!(
        dipole_position(&model, SourceRole::DipoleNegative),
        DVec3::new(-1.0, 0.0, 0.0)
    );
    let after = prompt_model::preview(&model);
    assert!(after.scene_summary.contains("2"));
    assert_eq!(
        before.equations.len(),
        after.equations.len(),
        "the same equations are re-derived from the same model"
    );
    assert_ne!(before.scene_summary, after.scene_summary);

    assert!(model.set_separation(0.0).is_err());
    assert!(model.set_separation(f64::NAN).is_err());
}

#[test]
fn apply_uses_the_ordinary_scene_setters() {
    let mut app = VfApp::new();
    let mut model = prompt_model::parse_prompt(DIPOLE).unwrap();
    let question = model.ambiguities[0].clone();
    assert!(model.resolve_question(&question, DipoleEnd::PositiveAxis));

    prompt_model::apply(&model, &mut app.state).expect("the model applies");
    assert_eq!(app.state.field_mode, FieldMode::Sources);
    assert_eq!(app.state.sources.entries.len(), 2);
    assert!(app.state.sources.entries.iter().all(|entry| entry.enabled));
    assert_eq!(
        app.state.sources.entries[0].source.center,
        DVec3::new(0.5, 0.0, 0.0)
    );
    assert_eq!(
        app.state.sources.entries[1].source.center,
        DVec3::new(-0.5, 0.0, 0.0)
    );

    // The same model re-applied after an edit moves the committed sources.
    model.set_separation(2.0).unwrap();
    prompt_model::apply(&model, &mut app.state).expect("the edited model applies");
    assert_eq!(
        app.state.sources.entries[0].source.center,
        DVec3::new(1.0, 0.0, 0.0)
    );
}

#[test]
fn ambiguous_or_unsupported_models_refuse_to_apply_without_mutation() {
    let mut app = VfApp::new();
    let before = app.state.sources.clone();

    let ambiguous = prompt_model::parse_prompt(DIPOLE).unwrap();
    let error = prompt_model::apply(&ambiguous, &mut app.state)
        .expect_err("an unresolved orientation cannot apply");
    assert!(matches!(error, PromptModelError::Ambiguous(_)), "{error:?}");
    assert_eq!(app.state.sources, before);

    let unsupported = prompt_model::parse_prompt("a uniformly charged cube").unwrap();
    assert!(unsupported.sources.is_empty());
    assert!(!unsupported.limitations.is_empty());
    assert!(!prompt_model::preview(&unsupported).can_apply);
    let error = prompt_model::apply(&unsupported, &mut app.state)
        .expect_err("an unsupported distribution cannot apply");
    assert!(
        matches!(error, PromptModelError::Unsupported(_)),
        "{error:?}"
    );
    assert_eq!(app.state.sources, before);
}

#[test]
fn unsupported_distribution_names_the_limitation_and_the_formula_path() {
    let model = prompt_model::parse_prompt("a uniformly charged cube of side 2").unwrap();
    let limitation = model.limitations.join(" ");
    assert!(
        limitation.contains("cube") || limitation.contains("not"),
        "the limitation is specific: {limitation}"
    );
    assert!(limitation.contains("point charge"));
    assert!(limitation.contains("dipole"));
    let path = model.formula_path();
    assert!(path.to_lowercase().contains("formula"));
    assert!(path.contains("E") || path.contains("ρ"));
    let preview = prompt_model::preview(&model);
    assert!(preview.formula_path.to_lowercase().contains("formula"));
    assert!(preview.limitations[0].contains("point charge"));

    // A point charge away from the origin is outside the bounded grammar; it
    // must not be silently moved to the origin.
    let elsewhere = prompt_model::parse_prompt("a point charge at x = 2").unwrap();
    assert!(elsewhere.sources.is_empty());
    assert!(!elsewhere.limitations.is_empty());
    assert!(!prompt_model::preview(&elsewhere).can_apply);
}

#[test]
fn an_unnamed_dipole_axis_is_asked_and_resolvable() {
    let mut model = prompt_model::parse_prompt("two equal and opposite charges separated by d")
        .expect("the axis-free dipole parses");
    assert!(model.ambiguities.contains(&Question::DipoleAxis));
    assert!(!model.can_apply());
    assert!(model.resolve_axis(Axis::Y));
    assert!(!model.ambiguities.contains(&Question::DipoleAxis));
    assert!(
        model
            .ambiguities
            .contains(&Question::DipoleOrientation { axis: Axis::Y }),
        "after the axis the orientation is still open"
    );
    assert!(!model.can_apply());
}

#[test]
fn empty_and_oversized_prompts_are_rejected() {
    assert_eq!(
        prompt_model::parse_prompt("   "),
        Err(PromptModelError::Empty)
    );
    let long = "a".repeat(prompt_model::MAX_PROMPT_BYTES + 1);
    assert_eq!(
        prompt_model::parse_prompt(&long),
        Err(PromptModelError::TooLong)
    );
}

#[test]
fn panel_state_parses_resolves_edits_and_cancels() {
    let mut state = PromptModelState {
        input: DIPOLE.into(),
        ..PromptModelState::default()
    };
    state.parse();
    assert!(state.model.is_some());
    assert!(
        !state
            .preview()
            .expect("a parsed model has a preview")
            .can_apply
    );

    let question = state.model.as_ref().unwrap().ambiguities[0].clone();
    assert!(state.resolve(&question, DipoleEnd::PositiveAxis));
    assert!(state.preview().unwrap().can_apply);

    assert!(state.set_separation(2.0));
    assert!(state.preview().unwrap().scene_summary.contains("2"));

    state.cancel();
    assert!(state.model.is_none());
    assert!(state.preview().is_none());
}

#[test]
fn app_handler_parses_previews_applies_and_cancels_through_ordinary_setters() {
    let mut app = VfApp::new();
    let before = app.state.sources.clone();

    let mut state = PromptModelState {
        input: DIPOLE.into(),
        ..PromptModelState::default()
    };
    app.apply_prompt_model_action(&mut state, PromptModelAction::Parse)
        .expect("parse");
    assert!(
        app.apply_prompt_model_action(&mut state, PromptModelAction::Apply)
            .is_err()
    );
    assert_eq!(app.state.sources, before, "an ambiguous apply is neutral");

    let question = state.model.as_ref().unwrap().ambiguities[0].clone();
    app.apply_prompt_model_action(
        &mut state,
        PromptModelAction::Resolve {
            question,
            answer: DipoleEnd::PositiveAxis,
        },
    )
    .expect("resolve");
    let message = app
        .apply_prompt_model_action(&mut state, PromptModelAction::Apply)
        .expect("apply");
    assert!(message.contains("Applied"));
    assert_ne!(app.state.sources, before);
    assert_eq!(app.state.field_mode, FieldMode::Sources);

    let applied = app.state.sources.clone();
    app.apply_prompt_model_action(&mut state, PromptModelAction::Cancel)
        .expect("cancel");
    assert!(state.model.is_none());
    assert_eq!(
        app.state.sources, applied,
        "cancel leaves the scene unchanged"
    );
}

#[test]
fn preview_panel_paints_its_interpretation_section() {
    let ctx = support::context();
    let mut state = PromptModelState {
        input: POINT.into(),
        ..PromptModelState::default()
    };
    state.parse();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            let _ = prompt_model_ui::show(ui, &mut state);
        },
    );
    output.textures_delta.clear();
    let texts: Vec<String> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    let all = texts.join("\n");
    assert!(
        all.contains("Interpret"),
        "the panel title is painted: {all}"
    );
    assert!(
        all.contains("Preview"),
        "the preview section is painted: {all}"
    );
}
