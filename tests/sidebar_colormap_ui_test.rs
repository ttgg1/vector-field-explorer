//! Ticket 41 follow-up: drive the real Coloring sidebar row headlessly and
//! follow the selection through `sidebar::apply` into the viewmodel and the
//! rebuilt GPU scene, then through the Coloring reset and persistence.
//!
//! egui's accesskit output exposes each widget's rect and state, so the test
//! clicks the actual `selectable_value` and `Reset` widgets — no replica of
//! the row and no hard-coded sidebar geometry beyond "the collapser beside the
//! Coloring label".

use eframe::egui;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::persistence::{AppSettings, ViewRecord};
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::ui::style_panel::AppStyle;
use vector_field_explorer::viewmodel::State;

/// One node from the frame's accesskit tree, in screen points.
#[derive(Clone)]
struct Node {
    role: egui::Role,
    label: Option<String>,
    value: Option<String>,
    toggled: egui::accesskit::Toggled,
    rect: egui::Rect,
}

impl Node {
    fn center(&self) -> egui::Pos2 {
        self.rect.center()
    }
}

struct Sidebar {
    ctx: egui::Context,
    state: State,
    controls: Controls,
    /// Monotonic frame time so egui's section-open animation advances.
    time: f64,
}

impl Sidebar {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        Self {
            ctx,
            state: State::new(false),
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

fn is_toggled(node: &Node) -> bool {
    node.toggled == egui::accesskit::Toggled::True
}

/// Expand the Coloring section by clicking the real collapser button beside
/// its label. The label's accessible text lives in `value` (egui maps Role::
/// Label text to the value, not the label), which lets the test find the right
/// row without relying on section order.
fn open_coloring(s: &mut Sidebar) -> egui::FullOutput {
    let out = s.idle();
    let nodes = nodes(&out);
    let label = nodes
        .iter()
        .find(|node| node.role == egui::Role::Label && node.value.as_deref() == Some("Coloring"))
        .expect("the Coloring section label is exposed");
    let collapser = nodes
        .iter()
        .find(|node| {
            node.role == egui::Role::Unknown
                && node.rect.max.x <= 20.0
                && (node.rect.center().y - label.center().y).abs() < 8.0
        })
        .expect("the Coloring collapser sits beside its label");
    s.click(collapser.center());
    // Let the section-open animation finish: the body is clipped to its
    // animated height, so a click on a lower row is ignored until it is fully
    // open even though accesskit already reports the row's rect.
    for _ in 0..20 {
        s.idle();
    }
    s.idle()
}

/// The Coloring section's header Reset button: the nearest Reset above the
/// colormap row.
fn coloring_reset(nodes: &[Node], colormap_row: &Node) -> egui::Rect {
    nodes
        .iter()
        .filter(|node| {
            node.role == egui::Role::Button
                && node.label.as_deref() == Some("Reset")
                && node.rect.max.y <= colormap_row.rect.min.y
        })
        .max_by(|a, b| a.rect.max.y.total_cmp(&b.rect.max.y))
        .expect("the Coloring header has a Reset button")
        .rect
}

fn assert_scene_uses(state: &mut State, expected: Colormap) {
    assert!(state.dirty, "the colormap change must mark the scene dirty");
    let sampled = state.sample();
    let (lo, hi) = state.color_range(&sampled);
    let scene = state.snapshot_if_dirty().expect("a dirty state rebuilds");
    assert_eq!(scene.arrows.len(), sampled.len());
    for (instance, arrow) in scene.arrows.iter().zip(&sampled) {
        let rgb = expected.colormap(arrow.mag, lo, hi);
        let want = [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0];
        for channel in 0..3 {
            assert!(
                (instance.color[channel] - want[channel]).abs() < 1e-6,
                "the rebuilt scene must use {expected:?}: {instance:?} vs {want:?}"
            );
        }
    }
    // And the palette really is a different one on this field, so the check
    // above is not vacuous.
    let other = if expected == Colormap::Viridis {
        Colormap::Turbo
    } else {
        Colormap::Viridis
    };
    assert!(
        sampled.iter().zip(scene.arrows.iter()).any(|(arrow, instance)| {
            let rgb = other.colormap(arrow.mag, lo, hi);
            (instance.color[0] - rgb[0] as f32 / 255.0).abs() > 1e-6
                || (instance.color[1] - rgb[1] as f32 / 255.0).abs() > 1e-6
                || (instance.color[2] - rgb[2] as f32 / 255.0).abs() > 1e-6
        }),
        "the new palette must change at least one arrow colour"
    );
}

#[test]
fn colormap_row_drives_state_scene_and_persistence() {
    let mut s = Sidebar::new();
    let out = open_coloring(&mut s);
    assert!(
        nodes(&out).iter().any(|node| {
            node.role == egui::Role::Button
                && node.label.as_deref() == Some("Viridis")
                && is_toggled(node)
        }),
        "Coloring opens with Viridis selected"
    );
    // Consume the startup build so the click's dirty flag is the only one.
    assert!(s.state.snapshot_if_dirty().is_some(), "the fresh scene builds once");
    assert!(!s.state.dirty);

    let rows = nodes(&out);
    let turbo = labelled(&rows, egui::Role::Button, "Turbo").expect("the Turbo button");
    assert!(!is_toggled(turbo), "Turbo is not selected yet");
    let out = s.click(turbo.center());
    assert_eq!(s.controls.colormap, Colormap::Turbo, "the mirror follows the click");
    assert_eq!(s.state.colormap, Colormap::Turbo, "apply reaches the viewmodel");
    assert!(s.state.dirty, "the change asks for a scene rebuild");
    let _ = out;

    // The next frame repaints the selection state: the displayed selection
    // really is Turbo now.
    let out_after = s.idle();
    let rows = nodes(&out_after);
    assert!(
        is_toggled(labelled(&rows, egui::Role::Button, "Turbo").expect("Turbo")),
        "Turbo is drawn selected"
    );
    assert!(
        !is_toggled(labelled(&rows, egui::Role::Button, "Viridis").expect("Viridis")),
        "Viridis is drawn unselected"
    );

    assert_scene_uses(&mut s.state, Colormap::Turbo);

    // Persistence: the UI-selected palette is what the saved view record
    // carries, and a restore (the loaded-state path) brings it back.
    let record = ViewRecord::capture(&s.state);
    assert_eq!(record.colormap, Colormap::Turbo, "the record stores the selection");
    let mut restored = State::new(false);
    let mut restored_controls = Controls::new();
    record.restore(&mut restored, &mut restored_controls);
    assert_eq!(restored.colormap, Colormap::Turbo, "a persisted Turbo survives restore");
    assert_eq!(restored_controls.colormap, Colormap::Turbo, "the widget mirror restores too");

    // Also prove the value survives the real on-disk envelope (save/load), not
    // just the record struct.
    let settings = AppSettings {
        style: AppStyle::default(),
        layout: LayoutState::default(),
        view: record,
    };
    let mut storage = MemoryStorage::default();
    vector_field_explorer::persistence::save(&mut storage, &settings);
    let loaded = vector_field_explorer::persistence::load(Some(&storage));
    assert_eq!(loaded.view.colormap, Colormap::Turbo, "Turbo round-trips through APP_KEY");
}

#[test]
fn coloring_reset_returns_viridis_through_the_real_button() {
    let mut s = Sidebar::new();
    let out = open_coloring(&mut s);
    let _ = s.state.snapshot_if_dirty(); // consume the startup build

    // Turn auto-fit off too, so Reset has more than the colormap to restore.
    let rows = nodes(&out);
    let auto_fit =
        labelled(&rows, egui::Role::CheckBox, "Auto-fit color range").expect("auto-fit checkbox");
    assert!(is_toggled(auto_fit), "auto-fit starts on");
    s.click(auto_fit.center());
    assert!(!s.controls.auto_fit, "the checkbox reaches the mirror");

    let out = s.idle();
    let rows = nodes(&out);
    let turbo = labelled(&rows, egui::Role::Button, "Turbo").expect("Turbo");
    s.click(turbo.center());
    let out = s.idle();
    assert_eq!(s.state.colormap, Colormap::Turbo);
    assert!(!s.state.auto_fit);

    let rows = nodes(&out);
    let viridis = labelled(&rows, egui::Role::Button, "Viridis").expect("Viridis");
    let reset = coloring_reset(&rows, viridis);
    let _ = s.state.snapshot_if_dirty(); // consume the Turbo rebuild
    s.click(reset.center());
    let out = s.idle();

    assert_eq!(s.controls.colormap, Colormap::Viridis, "reset restores the default palette");
    assert!(s.controls.auto_fit, "reset restores auto-fit");
    assert_eq!(s.state.colormap, Colormap::Viridis);
    assert!(s.state.auto_fit);

    let rows = nodes(&out);
    assert!(
        is_toggled(labelled(&rows, egui::Role::Button, "Viridis").expect("Viridis")),
        "the row shows the default selection again"
    );
    assert!(!is_toggled(labelled(&rows, egui::Role::Button, "Turbo").expect("Turbo")));
    assert_scene_uses(&mut s.state, Colormap::Viridis);
}

/// In-memory stand-in for eframe's disk-backed storage (the persistence test
/// uses the same shape; this one keeps the UI-driven value self-contained).
#[derive(Default)]
struct MemoryStorage {
    values: std::collections::HashMap<String, String>,
}

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.values.insert(key.to_owned(), value);
    }

    fn remove_string(&mut self, key: &str) {
        self.values.remove(key);
    }

    fn flush(&mut self) {}
}
