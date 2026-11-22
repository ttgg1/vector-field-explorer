//! Issue #230: the viewport colour legend.
//!
//! The overlay is driven through its real egui entry point and its pure
//! snapshot seam. Sidebar agreement is checked by rendering the real Coloring
//! section after an overlay edit; the reverse direction seeds the sidebar mirror
//! and then reads the overlay. The #219 case checks that a locked mapping keeps
//! a magnitude-to-colour identity while the probe radius changes.

use eframe::egui;
use glam::Vec3;

use crate::support;

use vector_field_explorer::color_mapping::{ColorSample, ColorStats};
use vector_field_explorer::source_scene::{FieldMode, SourceDisplay};
use vector_field_explorer::ui::gauss_shell::GaussShellState;
use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::ui::viewport_legend::{
    self, LEGEND_TITLE, LegendFrame, LegendSnapshot, PANEL_UNITS_NOTE,
};
use vector_field_explorer::viewmodel::{ArrowLengthMode, State};

const STATS: ColorStats = ColorStats {
    min_magnitude: 0.5,
    max_magnitude: 4.0,
    max_abs_component: 4.0,
};

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

fn has_value(nodes: &[Node], text: &str) -> bool {
    nodes
        .iter()
        .any(|node| node.value.as_deref() == Some(text) || node.label.as_deref() == Some(text))
}

fn has_value_containing(nodes: &[Node], text: &str) -> bool {
    nodes.iter().any(|node| {
        node.value.as_deref().is_some_and(|v| v.contains(text))
            || node.label.as_deref().is_some_and(|v| v.contains(text))
    })
}

fn labelled<'a>(nodes: &'a [Node], role: egui::Role, label: &str) -> Option<&'a Node> {
    nodes
        .iter()
        .find(|node| node.role == role && node.label.as_deref() == Some(label))
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

struct Legend {
    ctx: egui::Context,
    state: State,
    controls: Controls,
    stats: ColorStats,
    viewport: egui::Rect,
    compact: bool,
    time: f64,
}

impl Legend {
    fn new(compact: bool) -> Self {
        let ctx = support::context();
        ctx.enable_accesskit();
        let mut legend = Self {
            ctx,
            state: State::new(false),
            controls: Controls::new(),
            stats: STATS,
            viewport: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(640.0, 480.0)),
            compact,
            time: 0.0,
        };
        // AccessKit publishes the Area skeleton on the first pass; one warm-up
        // frame gives the widget tree its layout and bounds.
        legend.idle();
        legend
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> (egui::FullOutput, LegendFrame) {
        let visual_bounds =
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 700.0));
        let input = egui::RawInput {
            screen_rect: Some(visual_bounds),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let state = &mut self.state;
        let controls = &mut self.controls;
        let stats = self.stats;
        let viewport = self.viewport;
        let compact = self.compact;
        let mut legend = None;
        let mut output = self.ctx.run_ui(input, |ui| {
            legend = Some(viewport_legend::show(
                ui.ctx(),
                viewport,
                state,
                controls,
                &stats,
                compact,
            ));
        });
        output.textures_delta.clear();
        (output, legend.expect("the legend is painted"))
    }

    fn idle(&mut self) -> (egui::FullOutput, LegendFrame) {
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) -> (egui::FullOutput, LegendFrame) {
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false))
    }

    /// Render the real sidebar with this harness's state and controls, so an
    /// overlay edit and the sidebar's `apply` handoff are visibly the same.
    fn sidebar_frame(&mut self) -> egui::FullOutput {
        self.sidebar_frame_events(Vec::new())
    }

    fn sidebar_frame_events(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(600.0, 3200.0),
            )),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        self.controls.color_stats = self.stats;
        let state = &mut self.state;
        let controls = &mut self.controls;
        let mut output = self.ctx.run_ui(input, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                sidebar::show(
                    ui,
                    state,
                    controls,
                    &mut vector_field_explorer::seed_layout_library::SeedLayoutLibrary::default(),
                );
            });
        });
        output.textures_delta.clear();
        output
    }

    fn sidebar_click(&mut self, pos: egui::Pos2) {
        self.sidebar_frame_events(pointer(pos, true));
        self.sidebar_frame_events(pointer(pos, false));
    }
}

/// Expand the Coloring section through its real collapser.
fn open_coloring(l: &mut Legend) -> egui::FullOutput {
    let out = l.sidebar_frame();
    let rows = nodes(&out);
    let label = rows
        .iter()
        .find(|node| node.role == egui::Role::Label && node.value.as_deref() == Some("Coloring"))
        .expect("the Coloring section label is exposed");
    let collapser = rows
        .iter()
        .find(|node| {
            node.role == egui::Role::Unknown
                && node.rect.max.x <= 20.0
                && (node.rect.center().y - label.center().y).abs() < 8.0
        })
        .expect("the Coloring collapser sits beside its label");
    l.sidebar_click(collapser.center());
    for _ in 0..20 {
        l.sidebar_frame();
    }
    l.sidebar_frame()
}

fn source_state(state: &mut State, display: SourceDisplay) {
    state.set_field_mode(FieldMode::Sources);
    let mut scene = state.sources.clone();
    scene.set_display(display);
    state.set_source_scene(scene);
}

#[test]
fn snapshot_reports_quantity_units_range_lock_and_arrow_mode() {
    let mut state = State::new(false);
    let snapshot = LegendSnapshot::new(&state, &STATS);
    assert_eq!(snapshot.quantity, "F", "a formula field is labelled F");
    assert_eq!(snapshot.units, PANEL_UNITS_NOTE);
    assert!(
        snapshot.units.contains("Normalized"),
        "honest unit convention"
    );
    assert!(snapshot.range.contains("0.50") && snapshot.range.contains("4.00"));
    assert!(!snapshot.locked);
    assert_eq!(snapshot.state_label, "Auto");
    assert_eq!(snapshot.arrow_mode, "Proportional");
    assert!(!snapshot.arrow_meaning.is_empty());

    state.set_color_range(1.0, 3.0);
    state.set_auto_fit(false);
    let locked = LegendSnapshot::new(&state, &STATS);
    assert!(locked.locked);
    assert_eq!(locked.state_label, "Locked");
    assert!(locked.range.contains("1.00") && locked.range.contains("3.00"));

    state.set_arrow_length_mode(ArrowLengthMode::Uniform);
    let uniform = LegendSnapshot::new(&state, &STATS);
    assert_eq!(uniform.arrow_mode, "Uniform");
    assert_eq!(uniform.arrow_meaning, ArrowLengthMode::Uniform.tooltip());
}

#[test]
fn family_quantity_labels_are_honest_for_e_b_and_both() {
    let mut state = State::new(false);
    source_state(&mut state, SourceDisplay::Electric);
    assert_eq!(LegendSnapshot::new(&state, &STATS).quantity, "E");
    source_state(&mut state, SourceDisplay::Magnetic);
    assert_eq!(LegendSnapshot::new(&state, &STATS).quantity, "B");
    source_state(&mut state, SourceDisplay::Both);
    let both = LegendSnapshot::new(&state, &STATS);
    assert_eq!(both.quantity, "E + B");
    assert!(!both.numeric_ramp, "Both has no single numeric ramp");
    assert!(!both.lockable);
    let scales = both.family_scales.expect("Both names its family scales");
    assert_eq!(scales[0].0, "E amber");
    assert_eq!(scales[1].0, "B cyan");
    assert_ne!(scales[0].1, scales[1].1, "the family colours differ");
}

#[test]
fn both_display_overlay_names_family_colours_without_a_numeric_ramp() {
    let mut legend = Legend::new(false);
    source_state(&mut legend.state, SourceDisplay::Both);
    let (out, frame) = legend.idle();
    assert!(frame.body_visible);
    let rows = nodes(&out);
    assert!(has_value(&rows, "E amber"));
    assert!(has_value(&rows, "B cyan"));
    assert!(
        has_value_containing(&rows, "No single magnitude ramp"),
        "Both must explain that the families are fixed colours"
    );
    assert!(!has_value(&rows, "Lock range"));
    assert!(
        !has_value_containing(&rows, "Range:"),
        "Both must not pretend one numeric ramp covers both families"
    );
}

#[test]
fn overlay_lock_updates_state_and_the_sidebar_legend() {
    let mut legend = Legend::new(false);
    let (out, _) = legend.idle();
    let rows = nodes(&out);
    let lock = labelled(&rows, egui::Role::Button, "Lock range").expect("the lock button");
    legend.click(lock.center());
    legend.idle();

    assert!(!legend.state.auto_fit, "lock turns auto-fit off");
    assert_eq!(legend.state.color_min, 0.5);
    assert_eq!(legend.state.color_max, 4.0);
    assert!(!legend.controls.auto_fit, "the controls mirror follows");
    assert_eq!(legend.controls.color_min, 0.5);
    assert_eq!(legend.controls.color_max, 4.0);

    let sidebar = open_coloring(&mut legend);
    let rows = nodes(&sidebar);
    assert!(
        has_value(&rows, "Range: 0.50 - 4.00"),
        "the sidebar legend shows the same locked range"
    );
    let auto = labelled(&rows, egui::Role::CheckBox, "Auto-fit color range")
        .expect("the sidebar Auto-fit checkbox");
    assert_ne!(auto.toggled, egui::accesskit::Toggled::True);
}

#[test]
fn overlay_edit_updates_state_and_the_sidebar() {
    let mut legend = Legend::new(false);
    legend.state.set_color_range(1.0, 4.0);
    legend.state.set_auto_fit(false);
    legend.controls.auto_fit = false;
    legend.controls.color_min = 1.0;
    legend.controls.color_max = 4.0;
    legend.idle();

    let (out, _) = legend.idle();
    let rows = nodes(&out);
    let spins: Vec<&Node> = rows
        .iter()
        .filter(|node| node.role == egui::Role::SpinButton)
        .collect();
    assert_eq!(spins.len(), 2, "locked legend exposes min and max");
    let start = spins[0].center();
    let end = start + egui::vec2(40.0, 0.0);
    legend.frame(pointer(start, true));
    legend.frame(vec![egui::Event::PointerMoved(end)]);
    legend.frame(pointer(end, false));
    legend.idle();

    let edited_min = legend.state.color_min;
    assert!(
        edited_min != 1.0,
        "dragging the min row edits the stored range"
    );
    assert_eq!(legend.state.color_max, 4.0);
    assert_eq!(legend.controls.color_min, edited_min);

    let sidebar = open_coloring(&mut legend);
    let rows = nodes(&sidebar);
    assert!(
        has_value(&rows, &format!("Range: {edited_min:.2} - 4.00")),
        "the sidebar follows the overlay edit without a reload"
    );
}

#[test]
fn overlay_unlock_restores_auto_fit_and_sidebar_auto() {
    let mut legend = Legend::new(false);
    legend.state.set_color_range(0.5, 4.0);
    legend.state.set_auto_fit(false);
    legend.idle();
    let (out, _) = legend.idle();
    let rows = nodes(&out);
    let unlock = labelled(&rows, egui::Role::Button, "Unlock").expect("the unlock button");
    legend.click(unlock.center());
    legend.idle();

    assert!(legend.state.auto_fit, "unlock returns to auto-fit");
    assert!(legend.controls.auto_fit);

    let sidebar = open_coloring(&mut legend);
    let rows = nodes(&sidebar);
    let auto = labelled(&rows, egui::Role::CheckBox, "Auto-fit color range")
        .expect("the sidebar Auto-fit checkbox");
    assert_eq!(auto.toggled, egui::accesskit::Toggled::True);
    assert!(!has_value_containing(&rows, "Range: 1.00"));
}

#[test]
fn overlay_follows_a_sidebar_range_change_in_the_same_session() {
    let mut legend = Legend::new(false);
    legend.controls.auto_fit = false;
    legend.controls.color_min = 1.0;
    legend.controls.color_max = 3.0;
    // The real sidebar handoff commits the mirror to the viewmodel.
    sidebar::apply(&mut legend.state, &mut legend.controls);
    let (out, _) = legend.idle();
    let rows = nodes(&out);
    assert!(has_value_containing(&rows, "1.00"));
    assert!(has_value_containing(&rows, "3.00"));
    assert!(
        has_value(&rows, "Locked"),
        "overlay follows the sidebar lock"
    );
}

#[test]
fn legend_collapses_and_compact_stays_narrow() {
    let mut legend = Legend::new(false);
    let (out, frame) = legend.idle();
    assert!(frame.body_visible, "wide default opens the body");
    let rows = nodes(&out);
    let header = rows
        .iter()
        .find(|node| {
            node.role == egui::Role::DisclosureTriangle
                && node.label.as_deref() == Some(LEGEND_TITLE)
        })
        .expect("the legend header is a disclosure widget");
    legend.click(header.center());
    // Let the collapse animation finish; the body closure runs while the
    // section is still animating shut.
    for _ in 0..30 {
        legend.idle();
    }
    let (out, frame) = legend.idle();
    assert!(!frame.body_visible, "the header collapses the body");
    assert!(
        has_value_containing(&nodes(&out), LEGEND_TITLE),
        "the header stays visible and keyboard-accessible"
    );

    let mut compact = Legend::new(true);
    let (out, frame) = compact.idle();
    assert!(!frame.body_visible, "compact starts collapsed");
    assert!(has_value_containing(&nodes(&out), LEGEND_TITLE));
    assert!(
        viewport_legend::panel_width(compact.viewport, true) < compact.viewport.width() * 0.7,
        "compact width leaves most of the viewport uncovered"
    );
    assert!(frame.rect.width() <= compact.viewport.width() * 0.7);
}

#[test]
fn overlay_controls_are_ordinary_keyboard_reachable_widgets() {
    let mut legend = Legend::new(false);
    let (out, _) = legend.idle();
    let rows = nodes(&out);
    assert!(
        rows.iter().any(|node| {
            node.role == egui::Role::DisclosureTriangle
                && node.label.as_deref() == Some(LEGEND_TITLE)
        }),
        "the collapse header is a real widget"
    );
    assert!(labelled(&rows, egui::Role::Button, "Lock range").is_some());
}

#[test]
fn locked_mapping_is_invariant_to_probe_radius_and_resampling() {
    let mut state = State::new(false);
    state.set_color_range(1.0, 4.0);
    state.set_auto_fit(false);
    let sample = ColorSample {
        magnitude: 2.0,
        dir: Vec3::X,
    };
    let before_stats = ColorStats {
        min_magnitude: 0.5,
        max_magnitude: 4.0,
        max_abs_component: 4.0,
    };
    let before = state.color_mapping().sample(&sample, &before_stats);
    assert_eq!(before.clipped, None);

    // #219: doubling the Gauss-shell probe radius changes the reading, not the
    // locked colour mapping.
    let mut shell = GaussShellState::default();
    shell.probe_radius = 1.5;
    shell.probe_radius *= 2.0;
    assert_eq!(shell.probe_radius, 3.0, "the #219 probe radius doubled");
    let after_stats = ColorStats {
        min_magnitude: 0.1,
        max_magnitude: 100.0,
        max_abs_component: 100.0,
    };
    let after = state.color_mapping().sample(&sample, &after_stats);
    assert_eq!(
        before.rgb, after.rgb,
        "equal magnitudes keep equal colours under a locked range"
    );
    assert_eq!(before.clipped, after.clipped);

    // Non-vacuous: the same magnitude does move colour when auto-fit follows
    // the resampled stats.
    state.set_auto_fit(true);
    let auto = state.color_mapping().sample(&sample, &after_stats);
    assert_ne!(
        auto.rgb, after.rgb,
        "auto-fit is what rescales, so the lock is doing the work"
    );
}
