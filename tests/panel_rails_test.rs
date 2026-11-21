use eframe::egui;
use vector_field_explorer::VfApp;
use vector_field_explorer::ui::{
    layout::{RAIL_WIDTH, RailSide, collapse_chevron, rail_chevron},
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

/// As [`pass`], also returning the rectangle and text shapes the pass painted.
fn pass_recording(
    ctx: &egui::Context,
    flags: &mut [bool; 3],
    order: [usize; 3],
    events: Vec<egui::Event>,
    time: f64,
) -> (
    RailStripResponse,
    Vec<egui::epaint::RectShape>,
    Vec<egui::epaint::TextShape>,
) {
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
    let text = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => Some(shape.clone()),
            _ => None,
        })
        .collect();
    (result.unwrap(), rects, text)
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
        let (strip, rects, _) = pass_recording(&ctx, &mut flags, [0, 1, 2], vec![], 0.0);
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
fn click_frame_paints_the_toggled_entry_state() {
    // The strip used to toggle `expanded` before painting, so the click frame
    // already showed the new selection and collapse chevron. The shared painter
    // must not reintroduce a one-frame stale visual.
    let ctx = crate::support::context();
    let mut flags = [false; 3];
    let first = pass(&ctx, &mut flags, [0, 1, 2], vec![], 0.0);
    let target = first.entries[0].response.rect.center();

    pass(
        &ctx,
        &mut flags,
        [0, 1, 2],
        vec![egui::Event::PointerMoved(target)],
        0.1,
    );
    pass(
        &ctx,
        &mut flags,
        [0, 1, 2],
        vec![
            egui::Event::PointerMoved(target),
            egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
        0.12,
    );
    let (strip, _, text) = pass_recording(
        &ctx,
        &mut flags,
        [0, 1, 2],
        vec![
            egui::Event::PointerMoved(target),
            egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ],
        0.14,
    );

    assert_eq!(flags, [true, false, false], "the release toggles the entry");
    let tile = strip.entries[0].response.rect;
    let paints_in_tile = |glyph: &str| {
        text.iter().any(|shape| {
            shape.galley.text() == glyph && tile.contains_rect(shape.visual_bounding_rect())
        })
    };
    assert!(
        paints_in_tile(collapse_chevron(RailSide::Left)),
        "the click frame paints the new collapse chevron"
    );
    assert!(
        !paints_in_tile(rail_chevron(RailSide::Left)),
        "the click frame must not still paint the old outward chevron"
    );
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

// --- Issue #154: one right rail column, mirroring the left strip -----------

const SCREEN: egui::Vec2 = egui::vec2(1280.0, 1000.0);

/// Drive real wide application frames so the right chrome is asserted against
/// the shipped assembly, not a reimplementation.
struct RightRailHarness {
    ctx: egui::Context,
    app: VfApp,
}

impl RightRailHarness {
    fn new() -> Self {
        let ctx = crate::support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut app = VfApp::new();
        app.layout.help_visible = false;
        app.layout.presets_expanded = false;
        app.layout.appearance_expanded = false;
        app.layout.interaction_expanded = false;
        Self { ctx, app }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut self.app, ui, &mut frame),
        );
        out.textures_delta.clear();
        out
    }

    fn idle(&mut self) -> egui::FullOutput {
        for _ in 0..3 {
            self.frame(vec![]);
        }
        self.frame(vec![])
    }

    fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        self.idle()
    }
}

/// Every rectangle shape the frame painted, nested vectors flattened.
fn rects(out: &egui::FullOutput) -> Vec<egui::epaint::RectShape> {
    fn collect(shape: &egui::Shape, into: &mut Vec<egui::epaint::RectShape>) {
        match shape {
            egui::Shape::Rect(rect) => into.push(rect.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, into);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &out.shapes {
        collect(&clipped.shape, &mut found);
    }
    found
}

/// The panel-footprint rects of the right rail column: full-height, exactly
/// [`RAIL_WIDTH`] wide (the separator removes one point from the painted
/// rect), on the right half of the screen. There is one per rail panel; issue
/// #154 requires exactly one on the right side.
fn right_rail_panel_rects(out: &egui::FullOutput) -> Vec<egui::Rect> {
    rects(out)
        .into_iter()
        .filter(|shape| {
            (shape.rect.width() - RAIL_WIDTH).abs() < 2.0
                && shape.rect.height() > 500.0
                && shape.rect.left() > SCREEN.x / 2.0
        })
        .map(|shape| shape.rect)
        .collect()
}

/// The painted entry tiles of the right strip in top-to-bottom order: rounded
/// label-sized rects narrower than the rail panel and **contained in the
/// recorded right rail panel band**, so an unrelated small widget near the
/// screen's right edge cannot be counted as an entry.
fn right_entry_tiles(out: &egui::FullOutput) -> Vec<egui::Rect> {
    let bands = right_rail_panel_rects(out);
    let mut tiles: Vec<egui::Rect> = rects(out)
        .into_iter()
        .filter(|shape| {
            shape.rect.width() < RAIL_WIDTH - 0.5
                && shape.rect.height() < 200.0
                && shape.rect.right() > SCREEN.x - RAIL_WIDTH
                && bands.iter().any(|band| band.contains_rect(shape.rect))
        })
        .map(|shape| shape.rect)
        .collect();
    tiles.sort_by(|a, b| a.top().total_cmp(&b.top()));
    tiles
}

/// Bounding rect of the rotated text run `label` painted this frame.
fn rotated_label_rect(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label && text.angle != 0.0 => Some(
                text.galley
                    .mesh_bounds
                    .rotate_bb(egui::emath::Rot2::from_angle(text.angle))
                    .translate(text.pos.to_vec2()),
            ),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    out.shapes
        .iter()
        .find_map(|clipped| find(&clipped.shape, label))
}

/// AccessKit widget bounds by label, so a test can prove an expanded body drew.
fn widget_bounds(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    out.platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            if node.label() != Some(label) && node.value() != Some(label) {
                return None;
            }
            node.bounds().map(|bounds| {
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                )
            })
        })
        .min_by(|a, b| a.left().total_cmp(&b.left()))
}

#[test]
fn right_side_paints_one_rail_column() {
    let mut h = RightRailHarness::new();
    let out = h.idle();
    assert!(
        !h.app.layout.presets_expanded
            && !h.app.layout.appearance_expanded
            && !h.app.layout.interaction_expanded,
        "the harness starts with all three right panels collapsed"
    );

    let panels = right_rail_panel_rects(&out);
    assert_eq!(
        panels.len(),
        1,
        "the right side must paint one rail column, got {panels:?}"
    );
    let panel = panels[0];
    assert!(
        (panel.width() - RAIL_WIDTH).abs() < 2.0,
        "the right rail column is {} wide, expected {RAIL_WIDTH} less the separator",
        panel.width()
    );
    assert!(
        (panel.right() - SCREEN.x).abs() < 0.5,
        "the right rail column ends at {} instead of the screen edge {}",
        panel.right(),
        SCREEN.x
    );

    // The left side already stacks its collapsed entries in one column; the
    // right column must be the mirror, not three adjacent rails.
    let left_panels = rects(&out)
        .into_iter()
        .filter(|shape| {
            (shape.rect.width() - RAIL_WIDTH).abs() < 2.0
                && shape.rect.height() > 500.0
                && shape.rect.right() < SCREEN.x / 2.0
        })
        .count();
    assert_eq!(left_panels, 1, "the left side paints one rail column");
}

#[test]
fn right_rail_stacks_presets_appearance_and_interaction() {
    let mut h = RightRailHarness::new();
    let out = h.idle();
    let labels = ["Presets", "Appearance", "Interaction"];
    let ink: Vec<egui::Rect> = labels
        .iter()
        .map(|label| {
            rotated_label_rect(&out, label)
                .unwrap_or_else(|| panic!("the right column paints {label}"))
        })
        .collect();

    // One x band: every entry sits in the same narrow right-hand column.
    for pair in ink.windows(2) {
        assert!(
            (pair[0].center().x - pair[1].center().x).abs() < 0.5,
            "{} and {} are not in the same column: {} vs {}",
            labels[0],
            labels[1],
            pair[0].center().x,
            pair[1].center().x
        );
    }
    // Vertically disjoint and ordered top to bottom.
    for (index, pair) in ink.windows(2).enumerate() {
        assert!(
            pair[0].bottom() <= pair[1].top(),
            "{} and {} overlap vertically: {:?} vs {:?}",
            labels[index],
            labels[index + 1],
            pair[0],
            pair[1]
        );
        assert!(
            pair[0].top() < pair[1].top(),
            "{} must sit above {}",
            labels[index],
            labels[index + 1]
        );
    }

    // Each entry owns a label-sized tile at the top of the column, not one
    // full-height strip per panel.
    let tiles = right_entry_tiles(&out);
    assert_eq!(tiles.len(), 3, "three entry tiles, got {tiles:?}");
    for (label, ink) in labels.iter().zip(&ink) {
        let tile = tiles
            .iter()
            .find(|tile| tile.contains_rect(*ink))
            .unwrap_or_else(|| panic!("{label} ink {ink:?} has no entry tile in {tiles:?}"));
        assert!(
            tile.height() < 200.0,
            "{label} tile is {} tall, not one entry",
            tile.height()
        );
        assert!(
            tile.width() < RAIL_WIDTH,
            "{label} tile is {} wide, expected the rail content band",
            tile.width()
        );
    }
    assert!(
        tiles.iter().all(|tile| {
            (tile.left() - tiles[0].left()).abs() < 0.5
                && (tile.width() - tiles[0].width()).abs() < 0.5
        }),
        "the three entry tiles share one x band: {tiles:?}"
    );
}

#[test]
fn right_rail_entry_opens_its_panel() {
    let bodies = ["Save current view", "Save current appearance", "Rebind"];
    for (index, label) in ["Presets", "Appearance", "Interaction"].iter().enumerate() {
        let mut h = RightRailHarness::new();
        h.idle();
        let target = rotated_label_rect(&h.frame(vec![]), label)
            .unwrap_or_else(|| panic!("the right column paints {label}"))
            .center();
        let out = h.click(target);

        let flags = (
            h.app.layout.presets_expanded,
            h.app.layout.appearance_expanded,
            h.app.layout.interaction_expanded,
        );
        let expected = match index {
            0 => (true, false, false),
            1 => (false, true, false),
            _ => (false, false, true),
        };
        assert_eq!(flags, expected, "clicking {label} toggles only its flag");
        assert!(
            widget_bounds(&out, bodies[index]).is_some(),
            "clicking {label} must paint the {} body",
            label
        );
        for (other, body) in bodies.iter().enumerate() {
            if other != index {
                assert!(
                    widget_bounds(&out, body).is_none(),
                    "index {index} ({label}): the other expanded body {body} must stay hidden"
                );
            }
        }
    }
}
