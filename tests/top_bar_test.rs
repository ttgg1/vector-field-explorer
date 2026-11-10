// Moved out of `src/ui/top_bar.rs`'s inline module (ticket-14 review I-4).
// Ticket 59: the top bar's Appearance entry point is a gear button, and it must
// open the one popover that View > Appearance… also opens.

use eframe::egui;

use crate::support;

use vector_field_explorer::screenshot::Screenshot;
use vector_field_explorer::ui::branding;
use vector_field_explorer::ui::menu::{
    ABOUT_COPYRIGHT, ABOUT_LICENSE, ABOUT_NAME, ABOUT_SITE_URL, ABOUT_TAGLINE, ABOUT_URL,
    ABOUT_VERSION, MENUS, item_shortcut_label,
};
use vector_field_explorer::ui::style_panel::AppStyle;
use vector_field_explorer::ui::top_bar::{
    self, APPEARANCE_BUTTON_LABEL, HAMBURGER_LABEL, dismisses_appearance_popover, wheel_zoom_factor,
};

/// One headless window running the real top menu bar and Appearance popover.
struct Harness {
    ctx: egui::Context,
    style: AppStyle,
    screenshot: Screenshot,
    open: bool,
}

impl Harness {
    fn new(open: bool) -> Self {
        let ctx = support::context();
        // Accesskit publishes widget rects, so the test can find the gear and
        // the popover's controls without hard-coded geometry.
        ctx.enable_accesskit();
        Self {
            ctx,
            style: AppStyle::default(),
            screenshot: Screenshot::new(),
            open,
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
        let (mut output, open) = {
            let style = &mut self.style;
            let screenshot = &self.screenshot;
            let open = &mut self.open;
            let output = self.ctx.run_ui(input, |ui| {
                egui::Panel::top("top_bar").show(ui, |ui| {
                    let _ = top_bar::show(ui, style, screenshot, open);
                });
            });
            (output, *open)
        };
        // There is no painter in a headless test: drop the uploads on purpose.
        output.textures_delta.clear();
        self.open = open;
        output
    }

    fn idle(&mut self) -> egui::FullOutput {
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) {
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false));
    }

    /// One headless pass running the compact bar (ticket 53).
    fn compact_frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(360.0, 640.0),
            )),
            events,
            ..Default::default()
        };
        let (mut output, open) = {
            let style = &mut self.style;
            let screenshot = &self.screenshot;
            let open = &mut self.open;
            let output = self.ctx.run_ui(input, |ui| {
                egui::Panel::top("compact_top_bar_test").show(ui, |ui| {
                    let _ = top_bar::show_compact(ui, style, screenshot, open);
                });
            });
            (output, *open)
        };
        output.textures_delta.clear();
        self.open = open;
        output
    }

    fn compact_idle(&mut self) -> egui::FullOutput {
        self.compact_frame(Vec::new())
    }

    fn compact_click(&mut self, pos: egui::Pos2) {
        self.compact_frame(pointer(pos, true));
        self.compact_frame(pointer(pos, false));
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

/// One accesskit node with the bits these tests compare.
struct Node {
    role: egui::Role,
    label: Option<String>,
    value: Option<String>,
    rect: egui::Rect,
}

fn nodes(output: &egui::FullOutput) -> Vec<Node> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some(Node {
                role: node.role(),
                label: node.label().map(str::to_owned),
                value: node.value().map(str::to_owned),
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn labelled<'a>(nodes: &'a [Node], role: egui::Role, label: &str) -> Option<&'a Node> {
    nodes
        .iter()
        .find(|node| node.role == role && node.label.as_deref() == Some(label))
}

fn gear(nodes: &[Node]) -> egui::Rect {
    labelled(nodes, egui::Role::Button, APPEARANCE_BUTTON_LABEL)
        .expect("the top bar exposes an Appearance gear button")
        .rect
}

fn has_control(nodes: &[Node], text: &str) -> bool {
    nodes.iter().any(|node| {
        node.label.as_deref() == Some(text) || node.value.as_deref() == Some(text)
    })
}

fn color_well(nodes: &[Node]) -> Option<egui::Rect> {
    nodes
        .iter()
        .find(|node| node.role == egui::Role::ColorWell)
        .map(|node| node.rect)
}

#[test]
fn no_scroll_is_no_zoom() {
    assert_eq!(wheel_zoom_factor(0.0), 1.0);
}

/// Ticket 53 fix round: inspect the real compact hamburger dropdown instead of
/// comparing a helper with a copied action table. Every command the wide
/// `MENUS` table exposes must be a button in the hamburger, and the hamburger
/// must not invent commands.
#[test]
fn compact_hamburger_lists_every_wide_menu_item() {
    let mut h = Harness::new(false);
    let out = h.compact_idle();
    let hamburger = labelled(&nodes(&out), egui::Role::Button, HAMBURGER_LABEL)
        .expect("the compact bar renders the hamburger")
        .rect;
    h.compact_click(hamburger.center());
    let out = h.compact_idle();

    let button_labels: Vec<String> = nodes(&out)
        .into_iter()
        .filter(|node| node.role == egui::Role::Button)
        .filter_map(|node| node.label)
        .collect();

    let mut menu_labels = Vec::new();
    for menu in MENUS {
        for item in menu.items {
            // Accesskit appends the shortcut text to a button's label.
            let label = match item_shortcut_label(item) {
                Some(shortcut) => format!("{} {shortcut}", item.label),
                None => item.label.to_string(),
            };
            menu_labels.push(label.clone());
            assert!(
                button_labels.contains(&label),
                "the hamburger is missing `{label}` from {}; buttons={button_labels:?}",
                menu.title,
            );
        }
    }

    for label in &button_labels {
        if [HAMBURGER_LABEL, "Screenshot", "GitHub"].contains(&label.as_str()) {
            continue;
        }
        assert!(
            menu_labels.contains(label),
            "the hamburger exposes `{label}`, which the wide menu table does not",
        );
    }
}

#[test]
fn the_compact_bar_renders_hamburger_github_and_screenshot() {
    let ctx = support::context();
    ctx.enable_accesskit();
    let mut style = AppStyle::default();
    let screenshot = Screenshot::new();
    let mut open = false;
    let mut published = Vec::new();

    // Two frames: the first publishes accesskit nodes, the second settles them.
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(360.0, 640.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::Panel::top("compact_top_bar").show(ui, |ui| {
                let _ = top_bar::show_compact(ui, &mut style, &screenshot, &mut open);
            });
        });
        output.textures_delta.clear();
        published = nodes(&output);
    }

    assert!(
        labelled(&published, egui::Role::Button, HAMBURGER_LABEL).is_some(),
        "the compact bar has a hamburger menu button"
    );
    assert!(
        labelled(&published, egui::Role::Button, "GitHub").is_some(),
        "the GitHub link stays one tap away"
    );
    assert!(
        labelled(&published, egui::Role::Button, "Screenshot").is_some(),
        "the Screenshot action stays one tap away"
    );
}

#[test]
fn scrolling_up_zooms_in_and_down_zooms_out() {
    let up = wheel_zoom_factor(50.0);
    let down = wheel_zoom_factor(-50.0);
    assert!(up > 1.0, "scroll up comes closer: {up}");
    assert!(down < 1.0, "scroll down pulls back: {down}");
    assert!((up * down - 1.0).abs() < 1e-6, "the directions are reciprocal");
    assert!(up > 1.05, "a wheel notch is a visible step, not a nudge: {up}");
}

#[test]
fn nonsense_scroll_leaves_the_zoom_alone() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(wheel_zoom_factor(bad), 1.0, "{bad} must not move the view");
    }
    assert_eq!(wheel_zoom_factor(1e30), 1.0, "an overflowing factor falls back to 1");
}

#[test]
fn clicking_the_popover_or_its_open_picker_keeps_the_popover() {
    // Ticket 41: the nested color picker is a separate foreground area, often
    // drawn beyond the popover rect. Its clicks must not count as outside.
    let popover_rect = egui::Rect::from_min_size(egui::pos2(100.0, 20.0), egui::vec2(260.0, 160.0));
    let popover_layer =
        egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("appearance_popover"));
    let picker_layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("popup"));
    let inside = egui::pos2(120.0, 80.0);
    let picker = egui::pos2(320.0, 300.0);
    let background = egui::pos2(900.0, 400.0);
    let background_layer = egui::LayerId::new(egui::Order::Background, egui::Id::unique("background"));

    assert!(
        !dismisses_appearance_popover(
            true,
            Some(inside),
            popover_rect,
            Some(popover_layer),
            Some(picker_layer)
        ),
        "a click inside the popover is not outside"
    );
    assert!(
        !dismisses_appearance_popover(
            true,
            Some(picker),
            popover_rect,
            Some(picker_layer),
            Some(picker_layer)
        ),
        "a click on the open picker belongs to the picker"
    );
    assert!(
        dismisses_appearance_popover(
            true,
            Some(background),
            popover_rect,
            Some(background_layer),
            Some(picker_layer)
        ),
        "a click on the app behind the popover still dismisses"
    );
    assert!(
        !dismisses_appearance_popover(
            false,
            Some(background),
            popover_rect,
            Some(background_layer),
            None
        ),
        "a frame with no click never dismisses"
    );
}

#[test]
fn the_gear_opens_the_appearance_popover() {
    let mut h = Harness::new(false);
    let out = h.idle();
    let gear = gear(&nodes(&out));
    assert!(
        gear.center().y < 40.0,
        "the gear sits in the top bar, not in the viewport: {gear:?}"
    );

    h.click(gear.center());
    h.idle();
    let out = h.idle();

    assert!(h.open, "clicking the gear opens the Appearance popover");
    assert!(
        color_well(&nodes(&out)).is_some(),
        "the popover that opened is the one with the appearance controls",
    );
}

#[test]
fn the_gear_toggles_the_open_popover_closed() {
    let mut h = Harness::new(true);
    h.idle();
    let out = h.idle();
    let gear = gear(&nodes(&out));

    h.click(gear.center());
    h.idle();

    assert!(
        !h.open,
        "the gear is a toggle for the popover it anchors, not an open-only command",
    );
}

#[test]
fn the_popover_keeps_the_style_controls() {
    let mut h = Harness::new(true);
    h.idle();
    let out = h.idle();
    let nodes = nodes(&out);

    assert!(
        color_well(&nodes).is_some(),
        "the accent picker's ColorWell is still in the popover",
    );
    for control in ["Theme", "Accent", "Rounding", "Spacing", "Font size"] {
        assert!(
            has_control(&nodes, control),
            "the Appearance popover still shows the {control} control",
        );
    }
}

#[test]
fn escape_closes_the_appearance_popover() {
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
    assert!(!h.open, "Esc still dismisses the popover");
}

#[test]
fn clicking_outside_closes_the_appearance_popover() {
    let mut h = Harness::new(true);
    h.idle();
    h.idle();
    h.click(egui::pos2(900.0, 400.0));
    assert!(!h.open, "an outside click still dismisses the popover");
}

/// The About window on its own headless context: the same widget the Help menu
/// opens, without a top bar or a window shell around it.
struct AboutHarness {
    ctx: egui::Context,
    open: bool,
}

impl AboutHarness {
    fn new() -> Self {
        let ctx = support::context();
        ctx.enable_accesskit();
        Self { ctx, open: true }
    }

    fn frame(&mut self) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let (mut output, open) = {
            let open = &mut self.open;
            let output = self
                .ctx
                .run_ui(input, |ui| top_bar::about_window(ui.ctx(), open));
            (output, *open)
        };
        output.textures_delta.clear();
        self.open = open;
        output
    }
}

fn all_text(nodes: &[Node]) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|node| [node.label.clone(), node.value.clone()])
        .flatten()
        .collect()
}

#[test]
fn about_renders_the_full_brand_identity() {
    let mut h = AboutHarness::new();
    h.frame();
    let out = h.frame();
    let nodes = nodes(&out);
    let text = all_text(&nodes);

    for expected in [
        ABOUT_NAME,
        ABOUT_TAGLINE,
        ABOUT_COPYRIGHT,
        ABOUT_SITE_URL,
        ABOUT_URL,
    ] {
        assert!(
            text.iter().any(|line| line == expected),
            "About must show `{expected}`; got {text:#?}",
        );
    }
    let version = format!("Version {ABOUT_VERSION}");
    assert!(
        text.iter().any(|line| line == &version),
        "About must show `{version}`; got {text:#?}",
    );
    let license = format!("License: {ABOUT_LICENSE}");
    assert!(
        text.iter().any(|line| line == &license),
        "About must show `{license}`; got {text:#?}",
    );
}

#[test]
fn about_draws_the_mark_at_128_points() {
    let mut h = AboutHarness::new();
    h.frame();
    let out = h.frame();
    let nodes = nodes(&out);

    let logo = nodes
        .iter()
        .find(|node| {
            node.role == egui::Role::Image
                && node.label.as_deref() == Some("Vector Field Explorer logo")
        })
        .expect("About exposes the logo with readable alt text");

    assert!(
        (logo.rect.width() - 128.0).abs() < 0.5 && (logo.rect.height() - 128.0).abs() < 0.5,
        "the About logo is 128x128 pt, got {:?}",
        logo.rect.size(),
    );
}

#[test]
fn the_logo_texture_is_the_cached_256px_mark() {
    let ctx = support::context();
    let first = branding::logo_texture(&ctx);
    assert_eq!(first.size(), [256, 256], "the helper decodes the committed 256 px mark");

    let second = branding::logo_texture(&ctx);
    assert_eq!(
        first.id(),
        second.id(),
        "the texture handle is cached in context data, not decoded again",
    );
}
