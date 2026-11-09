//! Ticket 41: the Appearance popover's accent color picker.
//!
//! The Appearance popover owns a nested egui color-picker popup.
//! That popup is a separate floating area, so a click on its widgets used to
//! look like an outside click to the popover and closed the whole panel mid
//! pick. These tests drive the real `top_bar::show` surface headlessly through
//! accesskit (which exposes the swatch and picker widgets' rects), so the bug
//! and the dismissal contract are both verified on the shipped widgets rather
//! than on a replica.

use eframe::egui;

use crate::support;

use vector_field_explorer::screenshot::Screenshot;
use vector_field_explorer::ui::style_panel::AppStyle;
use vector_field_explorer::ui::top_bar;

/// One headless window running the real top menu bar and Appearance popover.
struct Harness {
    ctx: egui::Context,
    style: AppStyle,
    screenshot: Screenshot,
    open: bool,
    /// The last frame's "the style record moved" flag from `top_bar::show`.
    changed: bool,
}

impl Harness {
    fn new(open: bool) -> Self {
        let ctx = support::context();
        // Accesskit publishes widget rects, which is how the test finds the
        // accent swatch and the picker controls without hard-coded geometry.
        ctx.enable_accesskit();
        Self {
            ctx,
            style: AppStyle::default(),
            screenshot: Screenshot::new(),
            open,
            changed: false,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let (mut output, changed) = {
            let style = &mut self.style;
            let screenshot = &self.screenshot;
            let open = &mut self.open;
            let mut changed = false;
            let output = self.ctx.run_ui(input, |ui| {
                egui::Panel::top("top_bar").show(ui, |ui| {
                    let (_, frame_changed) = top_bar::show(ui, style, screenshot, open);
                    changed = frame_changed;
                });
            });
            (output, changed)
        };
        // There is no painter in a headless test: drop the uploads on purpose.
        output.textures_delta.clear();
        self.changed = changed;
        output
    }

    fn idle(&mut self) -> egui::FullOutput {
        self.frame(Vec::new())
    }

    /// One press frame and one release frame at `pos`; returns whether the
    /// top bar reported a style change on either frame (a color-field press
    /// moves the color immediately, before the release completes the click).
    fn click(&mut self, pos: egui::Pos2) -> bool {
        self.frame(pointer(pos, true));
        let pressed_changed = self.changed;
        self.frame(pointer(pos, false));
        pressed_changed || self.changed
    }
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn nodes(output: &egui::FullOutput) -> Vec<(egui::Role, egui::Rect)> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some((
                node.role(),
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .collect()
}

fn color_well(output: &egui::FullOutput) -> egui::Rect {
    nodes(output)
        .into_iter()
        .find_map(|(role, rect)| (role == egui::Role::ColorWell).then_some(rect))
        .expect("the Appearance popover's Accent ColorWell must be visible")
}

/// The popover's layer: the layer under the accent swatch.
fn popover_layer(h: &Harness, well: egui::Rect) -> egui::LayerId {
    h.ctx
        .layer_id_at(well.center())
        .expect("the accent swatch sits on the Appearance popover layer")
}

/// A picker widget that is on the picker's own floating layer and drawn
/// outside the popover's content bounds — the exact click that used to dismiss
/// the popover. Prefers the 2D color field (the largest picker node) so the
/// click reliably changes the accent.
fn on_picker_layer(h: &Harness, rect: egui::Rect, popover: egui::LayerId) -> bool {
    h.ctx
        .layer_id_at(rect.center())
        .is_some_and(|layer| layer.order == egui::Order::Foreground && layer != popover)
}

fn picker_widget_outside(
    h: &Harness,
    output: &egui::FullOutput,
    popover: egui::LayerId,
) -> egui::Rect {
    let all = nodes(output);
    let content = all
        .iter()
        .filter(|(_, rect)| h.ctx.layer_id_at(rect.center()) == Some(popover))
        .fold(None::<egui::Rect>, |union, (_, rect)| {
            Some(union.map_or(*rect, |u| u.union(*rect)))
        })
        .expect("the Appearance popover has widgets");
    let mut candidates: Vec<egui::Rect> = all
        .into_iter()
        .filter(|(_, rect)| on_picker_layer(h, *rect, popover))
        .map(|(_, rect)| rect)
        .filter(|rect| !content.expand(12.0).contains(rect.center()))
        .collect();
    candidates.sort_by(|a, b| {
        (b.width() * b.height())
            .total_cmp(&(a.width() * a.height()))
    });
    candidates
        .into_iter()
        .next()
        .expect("a picker widget is drawn outside the popover")
}

#[test]
fn accent_picker_stays_open_while_the_user_picks_a_color() {
    let mut h = Harness::new(true);
    h.idle();
    let out = h.idle();
    let well = color_well(&out);
    let popover = popover_layer(&h, well);

    // Clicking the swatch opens the nested picker (it needs sizing passes).
    h.click(well.center());
    assert!(h.open, "opening the picker must not close the popover");
    h.idle();
    h.idle();
    let out = h.idle();

    let picker = picker_widget_outside(&h, &out, popover);
    let before = h.style.accent;
    let changed = h.click(picker.center());

    assert!(
        h.open,
        "a click on the nested picker is not an outside click; the popover must stay up"
    );
    assert!(
        changed,
        "a picker edit reports `changed` so the app re-applies the style"
    );
    assert_ne!(
        h.style.accent, before,
        "the picked color lands in the AppStyle record"
    );

    // And the picker itself survived the click, ready for the next edit.
    let out = h.idle();
    assert!(
        nodes(&out)
            .iter()
            .any(|(_, rect)| on_picker_layer(&h, *rect, popover)),
        "the color picker stays open after its own click"
    );
}

#[test]
fn escape_still_closes_the_appearance_popover() {
    let mut h = Harness::new(true);
    h.idle();
    h.idle();
    h.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(
        !h.open,
        "Esc dismissal is part of the popover contract and survives the custom click handling"
    );
}

#[test]
fn clicking_outside_still_dismisses_the_appearance_popover() {
    let mut h = Harness::new(true);
    h.idle();
    h.idle();
    // The empty viewport well away from the popover and its picker.
    h.click(egui::pos2(900.0, 400.0));
    assert!(
        !h.open,
        "outside-click dismissal survives; only the nested picker is exempt"
    );
}

#[test]
fn closing_the_popover_also_closes_the_picker() {
    let mut h = Harness::new(true);
    h.idle();
    let out = h.idle();
    let well = color_well(&out);
    let popover = popover_layer(&h, well);
    h.click(well.center());
    h.idle();
    h.idle();

    h.click(egui::pos2(900.0, 400.0));
    assert!(!h.open, "the outside click closes the popover");
    let out = h.idle();
    assert!(
        !nodes(&out)
            .iter()
            .any(|(_, rect)| on_picker_layer(&h, *rect, popover)),
        "the nested picker cannot outlive the popover that owns it"
    );
}
