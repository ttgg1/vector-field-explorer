//! Ticket 65: drive the real Camera sidebar section headlessly.
//!
//! The pure order/default/reset rules live in `sidebar_sections_test.rs`; this
//! surface test proves the shipped widgets: the `Fly camera` checkbox reflects
//! `Controls::fly`, toggles it in 3D, is disabled in 2D, and the section's
//! Reset exits fly. Accesskit supplies each widget's rect and state, so there
//! is no replica of the row.

use eframe::egui;

use crate::support;

use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::viewmodel::State;

struct Sidebar {
    ctx: egui::Context,
    state: State,
    controls: Controls,
    time: f64,
}

impl Sidebar {
    fn new(mode_3d: bool) -> Self {
        let ctx = support::context();
        ctx.enable_accesskit();
        Self {
            ctx,
            state: State::new(mode_3d),
            controls: Controls::new(),
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(600.0, 1200.0),
            )),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let mut output = {
            let state = &mut self.state;
            let controls = &mut self.controls;
            self.ctx.run_ui(input, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| sidebar::show(ui, state, controls));
            })
        };
        output.textures_delta.clear();
        output
    }

    fn idle(&mut self) -> egui::FullOutput {
        self.frame(Vec::new())
    }

    /// A few frames so the open-section animation settles before a click.
    fn settle(&mut self) -> egui::FullOutput {
        for _ in 0..12 {
            self.idle();
        }
        self.idle()
    }

    fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false))
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

#[derive(Clone)]
struct Node {
    role: egui::Role,
    label: Option<String>,
    value: Option<String>,
    toggled: egui::accesskit::Toggled,
    disabled: bool,
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
                toggled: node.toggled().unwrap_or(egui::accesskit::Toggled::False),
                disabled: node.is_disabled(),
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            })
        })
        .collect()
}

fn fly_checkbox(nodes: &[Node]) -> Option<&Node> {
    nodes
        .iter()
        .find(|node| node.role == egui::Role::CheckBox && node.label.as_deref() == Some("Fly camera"))
}

fn camera_reset(nodes: &[Node]) -> egui::Rect {
    let header = nodes
        .iter()
        .find(|node| node.role == egui::Role::Label && node.value.as_deref() == Some("Camera"))
        .expect("the Camera section label is exposed");
    nodes
        .iter()
        .filter(|node| {
            node.role == egui::Role::Button
                && node.label.as_deref() == Some("Reset")
                && (node.rect.center().y - header.rect.center().y).abs() < 8.0
        })
        .min_by(|a, b| a.rect.min.x.total_cmp(&b.rect.min.x))
        .expect("the Camera header has a Reset button")
        .rect
}

#[test]
fn the_camera_section_is_open_and_checks_the_fly_toggle() {
    let mut s = Sidebar::new(true);
    s.controls.fly = true;
    let out = s.settle();
    let nodes = nodes(&out);
    let checkbox = fly_checkbox(&nodes).expect("the Camera section shows the Fly camera checkbox");
    assert!(!checkbox.disabled, "3D keeps the fly toggle enabled");
    assert_eq!(
        checkbox.toggled,
        egui::accesskit::Toggled::True,
        "the checkbox reflects Controls::fly",
    );
    assert!(
        nodes
            .iter()
            .any(|node| node.value.as_deref() == Some("Flying — Esc to release")),
        "the status line says how to release a flying camera",
    );
}

#[test]
fn clicking_the_checkbox_enters_and_leaves_fly() {
    let mut s = Sidebar::new(true);
    let out = s.settle();
    let first = nodes(&out);
    let checkbox = fly_checkbox(&first).expect("the Fly camera checkbox exists");
    assert!(!s.controls.fly, "fly starts off");

    s.click(checkbox.rect.center());
    assert!(s.controls.fly, "clicking the checkbox enters fly");

    let out = s.idle();
    let second = nodes(&out);
    let checkbox = fly_checkbox(&second).expect("the checkbox is still there");
    assert_eq!(checkbox.toggled, egui::accesskit::Toggled::True);

    s.click(checkbox.rect.center());
    assert!(!s.controls.fly, "clicking it again leaves fly");
}

#[test]
fn the_fly_toggle_is_disabled_in_two_d() {
    let mut s = Sidebar::new(false);
    let out = s.settle();
    let all = nodes(&out);
    let checkbox = fly_checkbox(&all).expect("the checkbox is still exposed in 2D");
    assert!(checkbox.disabled, "fly is 3D-only, so the toggle is disabled in 2D");
    assert_ne!(
        checkbox.toggled,
        egui::accesskit::Toggled::True,
        "a 2D view never claims to be flying",
    );
}

#[test]
fn the_camera_reset_exits_fly() {
    let mut s = Sidebar::new(true);
    s.controls.fly = true;
    let out = s.settle();
    let reset = camera_reset(&nodes(&out));

    s.click(reset.center());

    assert!(!s.controls.fly, "the Camera section Reset exits fly");
}
