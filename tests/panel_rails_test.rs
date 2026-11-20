use eframe::egui;
use vector_field_explorer::ui::{
    layout::RailSide,
    panel_rails::{RailEntry, RailStripResponse, fit_panel_widths, show_rail_strip},
};

#[test]
fn widths_preserve_requests_or_share_the_minimum_and_excess() {
    assert_eq!(fit_panel_widths(&[], 500.0), Vec::<f32>::new());
    assert_eq!(fit_panel_widths(&[260.0, 260.0], 600.0), vec![260.0, 260.0]);
    assert_eq!(fit_panel_widths(&[260.0, 220.0], 420.0), vec![220.0, 200.0]);
    assert_eq!(fit_panel_widths(&[260.0, 220.0], 200.0), vec![100.0, 100.0]);
    for budget in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
        assert_eq!(fit_panel_widths(&[260.0], budget), vec![0.0]);
    }
    assert_eq!(fit_panel_widths(&[180.0, 180.0], 360.0), vec![180.0, 180.0]);
    assert_eq!(
        fit_panel_widths(&[260.0, 220.0, 180.0], 600.0),
        vec![220.0, 200.0, 180.0]
    );
}

fn pass(
    ctx: &egui::Context,
    flags: &mut [bool; 3],
    order: [usize; 3],
    events: Vec<egui::Event>,
    time: f64,
) -> RailStripResponse {
    pass_recording(ctx, flags, order, events, time).0
}

/// As [`pass`], also returning the rectangle shapes the pass painted.
fn pass_recording(
    ctx: &egui::Context,
    flags: &mut [bool; 3],
    order: [usize; 3],
    events: Vec<egui::Event>,
    time: f64,
) -> (RailStripResponse, Vec<egui::epaint::RectShape>) {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 240.0));
    let mut result = None;
    let mut output = ctx.run_pass(
        egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| {
            let mut ui = egui::Ui::new(
                ctx.clone(),
                egui::Id::unique("rails-test"),
                egui::UiBuilder::new().max_rect(screen),
            );
            let [controls, cas, dummy] = flags;
            let mut entries = [
                RailEntry {
                    id: "controls",
                    label: "Controls",
                    shortcut: Some("Ctrl+B"),
                    expanded: controls,
                },
                RailEntry {
                    id: "cas",
                    label: "CAS",
                    shortcut: None,
                    expanded: cas,
                },
                RailEntry {
                    id: "dummy",
                    label: "An arbitrary exceptionally long third panel label for scrolling",
                    shortcut: None,
                    expanded: dummy,
                },
            ];
            // Reorder descriptors, not state; no ID-dependent production behavior.
            if order == [2, 0, 1] {
                entries.rotate_right(1);
            }
            result = Some(show_rail_strip(
                &mut ui,
                "strip",
                RailSide::Left,
                &mut entries,
            ));
        },
    );
    output.textures_delta.clear();
    let rects = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(shape) => Some(shape.clone()),
            _ => None,
        })
        .collect();
    (result.unwrap(), rects)
}

#[test]
fn rail_entries_paint_the_configured_corner_radius() {
    for (rounding, expected) in [
        (6.0, egui::CornerRadius::same(6)),
        (0.0, egui::CornerRadius::ZERO),
    ] {
        let ctx = crate::support::context();
        vector_field_explorer::ui::style_panel::apply(
            &ctx,
            &vector_field_explorer::ui::style_panel::AppStyle {
                rounding,
                ..Default::default()
            },
        );
        let mut flags = [false; 3];
        let (strip, rects) = pass_recording(&ctx, &mut flags, [0, 1, 2], vec![], 0.0);
        for entry in &strip.entries {
            let shape = rects
                .iter()
                .find(|shape| shape.rect == entry.response.rect)
                .unwrap_or_else(|| {
                    panic!(
                        "{}: no background rect painted at {:?}",
                        entry.id, entry.response.rect
                    )
                });
            assert_eq!(
                shape.corner_radius, expected,
                "{}: rounding {rounding} must reach the rail entry",
                entry.id
            );
        }
    }
}

fn click(ctx: &egui::Context, flags: &mut [bool; 3], pos: egui::Pos2, time: f64) {
    for (i, pressed) in [true, false].into_iter().enumerate() {
        pass(
            ctx,
            flags,
            [0, 1, 2],
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            time + i as f64 * 0.02,
        );
    }
}

#[test]
fn three_entries_are_ordered_independent_and_scrollable() {
    for font_size in [8.0, 13.0, 24.0] {
        let ctx = crate::support::context();
        ctx.style_mut_of(egui::Theme::Dark, |style| {
            for font in style.text_styles.values_mut() {
                font.size = font_size;
            }
        });
        let mut flags = [false; 3];
        let first = pass(&ctx, &mut flags, [0, 1, 2], vec![], 0.0);
        assert_eq!(first.outer_rect.width(), 32.0);
        assert_eq!(
            first.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            ["controls", "cas", "dummy"]
        );
        for entry in &first.entries {
            assert!(entry.response.rect.height() >= 40.0);
            assert!(
                entry.response.rect.contains_rect(entry.label_bounds),
                "{}: {:?} {:?}",
                entry.id,
                entry.response.rect,
                entry.label_bounds
            );
        }
        for pair in first.entries.windows(2) {
            assert!(pair[0].response.rect.bottom() <= pair[1].response.rect.top());
        }
        let cas = first.entries[1]
            .response
            .rect
            .intersect(first.entries[1].response.interact_rect);
        click(&ctx, &mut flags, cas.center(), 0.1);
        assert_eq!(flags, [false, true, false]);
        let reordered = pass(&ctx, &mut flags, [2, 0, 1], vec![], 0.2);
        assert_eq!(
            reordered.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            ["dummy", "controls", "cas"]
        );
        for entry in &reordered.entries {
            assert_eq!(
                entry.response.id,
                first
                    .entries
                    .iter()
                    .find(|e| e.id == entry.id)
                    .unwrap()
                    .response
                    .id
            );
        }
        // Restore order and let the scroll area learn its content extent.
        pass(&ctx, &mut flags, [0, 1, 2], vec![], 0.3);
        let offscreen = first.entries[2].response.rect.center();
        if offscreen.y > 240.0 {
            click(&ctx, &mut flags, offscreen, 0.4);
            assert_eq!(
                flags,
                [false, true, false],
                "offscreen widgets cannot be clicked"
            );
        }
        let mut scrolled = first;
        for i in 0..20 {
            scrolled = pass(
                &ctx,
                &mut flags,
                [0, 1, 2],
                vec![
                    egui::Event::PointerMoved(egui::pos2(16.0, 120.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -100.0),
                        modifiers: Default::default(),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Unknown,
                    },
                ],
                0.5 + i as f64 * 0.1,
            );
        }
        let dummy = &scrolled.entries[2].response;
        let hit = dummy.rect.intersect(dummy.interact_rect);
        assert!(hit.is_positive());
        assert!(hit.center().y < 240.0);
        click(&ctx, &mut flags, hit.center(), 3.0);
        assert_eq!(flags, [false, true, true]);
        // An entry's full rect can extend outside the scroll viewport.
        let clipped = scrolled.entries[0].response.rect.center();
        click(&ctx, &mut flags, clipped, 3.2);
        assert_eq!(flags, [false, true, true]);
    }
}
