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
use vector_field_explorer::ui::layout::TOUCH_TARGET;
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

    fn frame_in_layout(&mut self, events: Vec<egui::Event>, compact: bool) -> egui::FullOutput {
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
                egui::ScrollArea::vertical().show(ui, |ui| {
                    sidebar::show_compact(ui, state, controls, compact)
                });
            })
        };
        output.textures_delta.clear();
        output
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.frame_in_layout(events, false)
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
fn compact_controls_do_not_offer_a_fly_camera_toggle() {
    let mut sidebar = Sidebar::new(true);
    let mut output = {
        let state = &mut sidebar.state;
        let controls = &mut sidebar.controls;
        sidebar.ctx.run_ui(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 1200.0),
            )),
            ..Default::default()
        }, |ui| sidebar::show_compact(ui, state, controls, true))
    };
    let labels: Vec<_> = nodes(&output)
        .into_iter()
        .filter_map(|node| node.label)
        .collect();
    output.textures_delta.clear();
    assert!(
        !labels.iter().any(|label| label.contains("Fly camera")),
        "the Compact Controls sheet must not offer fly camera entry",
    );
}

fn sidebar_accessible_rows(compact: bool) -> Vec<Node> {
    let mut sidebar = Sidebar::new(true);
    let output = sidebar.frame_in_layout(Vec::new(), compact);
    let disclosure = nodes(&output)
        .into_iter()
        .find(|node| {
            node.role == egui::Role::DisclosureTriangle
                && node.label.as_deref() == Some("Arrow appearance")
        })
        .expect("the Display section exposes its nested Arrow appearance control");
    sidebar.frame_in_layout(pointer(disclosure.rect.center(), true), compact);
    sidebar.frame_in_layout(pointer(disclosure.rect.center(), false), compact);
    let mut output = sidebar.frame_in_layout(Vec::new(), compact);
    let rows = nodes(&output);
    output.textures_delta.clear();
    rows
}

fn interactive_gap_after(rows: &[Node], label: &str, role: egui::Role) -> f32 {
    let parent = rows
        .iter()
        .find(|node| node.role == role && node.label.as_deref() == Some(label))
        .unwrap_or_else(|| panic!("interactive AccessKit row `{label}` exists"));
    let next_top = rows
        .iter()
        .filter(|node| {
            matches!(
                node.role,
                egui::Role::Button
                    | egui::Role::CheckBox
                    | egui::Role::Slider
                    | egui::Role::SpinButton
                    | egui::Role::DisclosureTriangle
            ) && node.rect.top() >= parent.rect.bottom() - 0.01
                && node.rect.center().y > parent.rect.center().y
        })
        .map(|node| node.rect.top())
        .min_by(f32::total_cmp)
        .expect("a following interactive widget row exists");
    next_top - parent.rect.bottom()
}

#[test]
fn actual_compact_sidebar_widget_rows_and_nested_spacing_use_accesskit_bounds() {
    let compact = sidebar_accessible_rows(true);
    let wide = sidebar_accessible_rows(false);

    for rows in [&compact, &wide] {
        let nested = rows
            .iter()
            .find(|node| {
                node.role == egui::Role::DisclosureTriangle
                    && node.label.as_deref() == Some("Arrow appearance")
            })
            .expect("the real nested Arrow appearance disclosure is accessible");
        let nested_child = rows
            .iter()
            .filter(|node| {
                matches!(node.role, egui::Role::Button | egui::Role::CheckBox | egui::Role::Slider | egui::Role::SpinButton)
                    && node.rect.top() >= nested.rect.bottom() - 0.01
            })
            .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .expect("an interactive child is visible under Arrow appearance");
        let nested_gap = nested_child.rect.top() - nested.rect.bottom();
        assert!(nested_gap >= 0.0, "nested control rows do not overlap: {nested_gap}");
        if std::ptr::eq(rows, &compact) {
            assert!(nested_child.rect.height() >= TOUCH_TARGET);
        }
    }

    let compact_control = compact
        .iter()
        .find(|node| node.role == egui::Role::Button && node.label.as_deref() == Some("Magnitude"))
        .unwrap();
    let wide_control = wide
        .iter()
        .find(|node| node.role == egui::Role::Button && node.label.as_deref() == Some("Magnitude"))
        .unwrap();
    assert!(compact_control.rect.height() >= TOUCH_TARGET, "real button target: {}pt", compact_control.rect.height());
    assert!(compact_control.rect.height() > wide_control.rect.height(), "Wide button sizing stays smaller");
    let compact_gap = interactive_gap_after(&compact, "Magnitude", egui::Role::Button);
    let wide_gap = interactive_gap_after(&wide, "Magnitude", egui::Role::Button);
    assert!(compact_gap < wide_gap, "real adjacent widget gap compact={compact_gap}, wide={wide_gap}");

    let compact_nested_gap = interactive_gap_after(&compact, "Arrow appearance", egui::Role::DisclosureTriangle);
    let wide_nested_gap = interactive_gap_after(&wide, "Arrow appearance", egui::Role::DisclosureTriangle);
    assert!(
        compact_nested_gap < wide_nested_gap,
        "real nested-control gap compact={compact_nested_gap}, wide={wide_nested_gap}",
    );
}

fn preset_accessible_rows(compact: bool) -> Vec<Node> {
    use vector_field_explorer::render::Camera;
    use vector_field_explorer::ui::presets::PresetPanel;

    let ctx = support::context();
    ctx.enable_accesskit();
    let mut state = State::new(false);
    let mut presets = PresetPanel::default();
    let mut camera = Camera::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 1200.0))),
            ..Default::default()
        },
        |ui| {
            presets.show_compact(ui, &mut camera, &mut state, compact);
        },
    );
    let rows = nodes(&output);
    output.textures_delta.clear();
    rows
}

fn first_preset_named<'a>(rows: &'a [Node], label: &str) -> &'a Node {
    rows.iter()
        .filter(|node| node.role == egui::Role::Button && node.label.as_deref() == Some(label))
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap_or_else(|| panic!("real preset row `{label}` is exposed"))
}

#[test]
fn actual_compact_preset_rows_and_spacing_use_accesskit_bounds() {
    let compact = preset_accessible_rows(true);
    let wide = preset_accessible_rows(false);
    let compact_vortex = first_preset_named(&compact, "Vortex");
    let wide_vortex = first_preset_named(&wide, "Vortex");
    assert!(compact_vortex.rect.height() >= TOUCH_TARGET);
    assert!(compact_vortex.rect.height() > wide_vortex.rect.height(), "Wide preset sizing remains unchanged");

    let gap_after = |rows: &[Node], label: &str, next_label: &str| {
        let current = first_preset_named(rows, label);
        let next = first_preset_named(rows, next_label);
        next.rect.top() - current.rect.bottom()
    };
    let compact_gap = gap_after(&compact, "Vortex", "3D swirl (3D)");
    let wide_gap = gap_after(&wide, "Vortex", "3D swirl (3D)");
    assert!(compact_gap < wide_gap, "actual preset row gap compact={compact_gap}, wide={wide_gap}");
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
