use eframe::egui::{Rect, pos2, vec2};
use eframe::egui;
use vector_field_explorer::ui::viewport_panel::{
    ViewportPanelState, clamp_position, show_slide, slide_pos, slide_rect,
};

#[test]
fn clamps_panel_origin_without_hiding_the_header() {
    let viewport = Rect::from_min_max(pos2(20.0, 40.0), pos2(420.0, 280.0));
    let size = vec2(100.0, 60.0);
    let default = pos2(308.0, 52.0);
    assert_eq!(clamp_position(viewport, default, size), default);
    let moved = pos2(120.0, 90.0);
    assert_eq!(clamp_position(viewport, moved, size), moved);
    assert_eq!(
        clamp_position(viewport, pos2(500.0, -30.0), size),
        pos2(320.0, 40.0)
    );
    assert_eq!(
        clamp_position(viewport, moved, vec2(500.0, 300.0)),
        viewport.min
    );
}

#[test]
fn slide_offset_leaves_layout_geometry_unchanged() {
    let base = pos2(120.0, 90.0);
    // A slide-in starts displaced and settles exactly on the resting position.
    assert_ne!(slide_pos(base, 0.0, 16.0), base);
    assert_eq!(slide_pos(base, 1.0, 16.0), base);
    assert_eq!(slide_pos(base, 0.5, 16.0).y, base.y - 8.0);

    // The painted rect moves, but its size — and therefore the layout footprint
    // and hit target — is identical at every progress value.
    let rect = Rect::from_min_size(base, vec2(180.0, 60.0));
    assert_eq!(slide_rect(rect, 0.0, 16.0).size(), rect.size());
    assert_eq!(slide_rect(rect, 1.0, 16.0).size(), rect.size());
    assert_eq!(slide_rect(rect, 1.0, 16.0), rect);
}

#[test]
fn a_sliding_panel_keeps_accessible_labels_and_rect_size_stable() {
    fn frame(progress: f32) -> (egui::Rect, Vec<String>) {
        let ctx = crate::support::context();
        ctx.enable_accesskit();
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 400.0));
        let distance = 12.0;
        let slide = vec2(0.0, distance * (1.0 - progress));
        let mut placement = ViewportPanelState::default();
        let input = egui::RawInput {
            screen_rect: Some(viewport),
            ..Default::default()
        };
        let mut rect = Rect::NOTHING;
        let mut draw = |ui: &mut egui::Ui, rect: &mut Rect| {
            *rect = show_slide(
                ui.ctx(),
                viewport,
                egui::Id::unique("motion_harness_panel"),
                pos2(100.0, 50.0),
                &mut placement,
                slide,
                |ui| {
                    ui.button("Measure field").clicked();
                },
            )
            .response
            .rect;
        };
        let mut output = ctx.run_ui(input.clone(), |ui| draw(ui, &mut rect));
        output.textures_delta.clear();
        let mut output = ctx.run_ui(input, |ui| draw(ui, &mut rect));
        output.textures_delta.clear();
        let labels: Vec<String> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|update| {
                update
                    .nodes
                    .iter()
                    .filter_map(|(_, node)| node.label().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        (rect, labels)
    }

    let (rest, rest_labels) = frame(1.0);
    let (mid, mid_labels) = frame(0.5);
    let (start, start_labels) = frame(0.0);

    // No layout geometry change: the panel size is identical at every frame.
    assert_eq!(start.size(), rest.size());
    assert_eq!(mid.size(), rest.size());
    // The slide is purely visual: labels are the same while in flight.
    assert_eq!(start_labels, rest_labels);
    assert_eq!(mid_labels, rest_labels);
    assert!(
        rest_labels.iter().any(|label| label == "Measure field"),
        "the control stays accessible during the transition: {rest_labels:?}"
    );
}
