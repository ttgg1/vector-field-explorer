//! P3 slice 1b: the Baked flow UI copy and refusal states.
//!
//! The pure caption/status helpers keep the panel honest: the resolution,
//! interpolation, measured error, divergence residual and teaching-units
//! caveat must be named together, and a refused bake must show the error
//! verbatim rather than a generic failure.

use eframe::egui;
use vector_field_explorer::field_bake::{BAKE_TOLERANCE, BakeAccuracy, BakeAxis};
use vector_field_explorer::seed_layout_library::SeedLayoutLibrary;
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::ui::sidebar::{
    self, Controls, bake_accuracy_caption, bake_provenance_line, bake_status_text,
};
use vector_field_explorer::viewmodel::State;

use crate::support;

fn measured() -> BakeAccuracy {
    BakeAccuracy {
        probes: 512,
        max_abs_error: Some(1.2e-2),
        max_rel_error: Some(0.04),
        divergence_residual: Some(3.5e-3),
        tolerance: BAKE_TOLERANCE,
    }
}

#[test]
fn accuracy_caption_names_resolution_interpolation_metrics_and_teaching_units() {
    let caption = bake_accuracy_caption(BakeAxis::Volume3d, 48, measured());
    assert!(caption.contains("48³"), "resolution is named: {caption}");
    assert!(
        caption.contains("trilinear"),
        "the interpolation is named: {caption}"
    );
    assert!(
        caption.contains("max |Δv|"),
        "the maximum component error is named: {caption}"
    );
    assert!(
        caption.contains("max relative error"),
        "the measured relative error is named: {caption}"
    );
    assert!(
        caption.contains("divergence residual"),
        "the divergence residual is named: {caption}"
    );
    assert!(
        caption.contains("normalized teaching approximation"),
        "the values are framed as teaching approximations: {caption}"
    );
    assert!(caption.contains("not SI"), "units are stated: {caption}");
    assert!(
        caption.contains("exceeds"),
        "a tolerance exceedance is reported, not hidden: {caption}"
    );

    let plane = bake_accuracy_caption(BakeAxis::Plane2d, 128, measured());
    assert!(plane.contains("128²"), "plane resolution is named: {plane}");
}

#[test]
fn unmeasured_metrics_stay_absent_in_the_caption() {
    let caption = bake_accuracy_caption(
        BakeAxis::Volume3d,
        48,
        BakeAccuracy {
            probes: 0,
            max_abs_error: None,
            max_rel_error: None,
            divergence_residual: None,
            tolerance: BAKE_TOLERANCE,
        },
    );
    assert!(caption.contains("not measured"), "{caption}");
    assert_eq!(
        caption.matches("not measured").count(),
        3,
        "each of max |Δv|, relative error and divergence stays absent when unmeasured: {caption}"
    );
    assert!(!caption.contains("exceeds"), "{caption}");
}

#[test]
fn status_text_reports_active_stale_and_refused_bakes() {
    assert_eq!(
        bake_status_text(false, false, None),
        None,
        "the opt-in is off, so there is no status"
    );
    let active = bake_status_text(true, true, None).expect("active status");
    assert!(active.contains("traced on a baked lattice"), "{active}");

    let stale = bake_status_text(true, false, None).expect("stale status");
    assert!(stale.contains("stale"), "{stale}");
    assert!(stale.contains("Rebuild"), "{stale}");

    let refused = bake_status_text(true, false, Some("cannot bake a time-dependent field"))
        .expect("refused status");
    assert_eq!(refused, "cannot bake a time-dependent field");
}

#[test]
fn provenance_line_names_the_active_lattice() {
    let line = bake_provenance_line(BakeAxis::Volume3d, 48);
    assert!(line.contains("baked lattice"), "{line}");
    assert!(line.contains("48³"), "{line}");
    let plane = bake_provenance_line(BakeAxis::Plane2d, 128);
    assert!(plane.contains("128²"), "{plane}");
}

#[test]
fn state_refusals_are_available_to_the_panel_verbatim() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state.set_field_bake_enabled(true);
    assert_eq!(
        state.field_bake_error(),
        Some("cannot bake the Sources field; only Formula can be baked")
    );

    state.set_field_mode(FieldMode::Formula);
    state.set_formula("(t, 0, 0)".into());
    state.set_field_bake_enabled(true);
    assert_eq!(
        bake_status_text(true, state.baked_flow_active(), state.field_bake_error()),
        Some("cannot bake a time-dependent field".to_owned())
    );
}

#[test]
fn sources_refusal_stays_reachable_in_the_baked_flow_panel() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state.set_field_bake_enabled(true);
    let error = state
        .field_bake_error()
        .expect("the refused build records its reason")
        .to_owned();
    assert_eq!(
        error,
        "cannot bake the Sources field; only Formula can be baked"
    );

    let mut controls = Controls::new();
    controls.density = state.density;
    controls.domain_min = state.domain.min;
    controls.domain_max = state.domain.max;
    let mut layouts = SeedLayoutLibrary::default();
    let mut output = None;
    for _ in 0..2 {
        let mut frame = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 1200.0),
                )),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    sidebar::show(ui, &mut state, &mut controls, &mut layouts);
                });
            },
        );
        frame.textures_delta.clear();
        output = Some(frame);
    }
    let output = output.expect("a frame ran");
    let labels: Vec<&str> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .map(|update| {
            update
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label())
                .collect()
        })
        .unwrap_or_default();

    assert!(
        labels.contains(&"Baked flow"),
        "the collapsed header stays reachable outside Formula: {labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.contains("only available for Formula fields")),
        "a disabled explanation says why the controls do nothing: {labels:?}"
    );
    assert!(
        labels.iter().any(|label| *label == error),
        "the recorded refusal is shown verbatim: {labels:?}"
    );
}
