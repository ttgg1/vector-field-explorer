//! Ticket 41 follow-up: drive the real Coloring sidebar row headlessly and
//! follow the selection through `sidebar::apply` into the viewmodel and the
//! rebuilt GPU scene, then through the Coloring reset and persistence.
//!
//! egui's accesskit output exposes each widget's rect and state, so the test
//! clicks the actual `selectable_value` and `Reset` widgets — no replica of
//! the row and no hard-coded sidebar geometry beyond "the collapser beside the
//! Coloring label".

use eframe::egui;

use crate::support;

use vector_field_explorer::color_mapping::{ClipCounts, ColorQuantity, ColorStats};
use vector_field_explorer::color_ramp::{ColorRamp, RampKind, ScaleMode};
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::palette_io::{PaletteDocument, export_ramp, import_ramp};
use vector_field_explorer::persistence::{AppSettings, ViewRecord};
use vector_field_explorer::source_scene::{FieldMode, SourceDisplay};
use vector_field_explorer::ui::help_overlay::sources_help_rows;
use vector_field_explorer::ui::layout::LayoutState;
use vector_field_explorer::ui::sidebar::{self, BOTH_COLORING_NOTE, Controls};
use vector_field_explorer::ui::style_panel::AppStyle;
use vector_field_explorer::ui::viewport_legend::BOTH_NOTE;
use vector_field_explorer::viewmodel::State;

/// One node from the frame's accesskit tree, in screen points.
#[derive(Clone)]
struct Node {
    role: egui::Role,
    label: Option<String>,
    value: Option<String>,
    toggled: egui::accesskit::Toggled,
    disabled: bool,
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
        let ctx = support::context();
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
                egui::vec2(600.0, 3000.0),
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
                    sidebar::show(
                        ui,
                        state,
                        controls,
                        &mut vector_field_explorer::seed_layout_library::SeedLayoutLibrary::default(
                        ),
                    );
                });
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
                disabled: node.is_disabled(),
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
        let want = [
            rgb[0] as f32 / 255.0,
            rgb[1] as f32 / 255.0,
            rgb[2] as f32 / 255.0,
        ];
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
        sampled
            .iter()
            .zip(scene.arrows.iter())
            .any(|(arrow, instance)| {
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
    assert!(
        s.state.snapshot_if_dirty().is_some(),
        "the fresh scene builds once"
    );
    assert!(!s.state.dirty);

    let rows = nodes(&out);
    let turbo = labelled(&rows, egui::Role::Button, "Turbo").expect("the Turbo button");
    assert!(!is_toggled(turbo), "Turbo is not selected yet");
    let out = s.click(turbo.center());
    assert_eq!(
        s.controls.colormap,
        Colormap::Turbo,
        "the mirror follows the click"
    );
    assert_eq!(
        s.state.colormap,
        Colormap::Turbo,
        "apply reaches the viewmodel"
    );
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
    assert_eq!(
        record.colormap,
        Colormap::Turbo,
        "the record stores the selection"
    );
    let mut restored = State::new(false);
    let mut restored_controls = Controls::new();
    record.restore(&mut restored, &mut restored_controls);
    assert_eq!(
        restored.colormap,
        Colormap::Turbo,
        "a persisted Turbo survives restore"
    );
    assert_eq!(
        restored_controls.colormap,
        Colormap::Turbo,
        "the widget mirror restores too"
    );

    // Also prove the value survives the real on-disk envelope (save/load), not
    // just the record struct.
    let settings = AppSettings {
        style: AppStyle::default(),
        layout: LayoutState::default(),
        view: record,
        parameters: Default::default(),
        viewport_style: Default::default(),
        layer_styles: Default::default(),
        preset_ui: Default::default(),
        appearance_profiles: Default::default(),
        interaction: Default::default(),
        interaction_profiles: Default::default(),
        seed_layouts: Default::default(),
        up_axis: Default::default(),
        object_style: Default::default(),
        bake: Default::default(),
        pin_ui: Default::default(),
        notebook: Default::default(),
    };
    let mut storage = MemoryStorage::default();
    vector_field_explorer::persistence::save(&mut storage, &settings);
    let loaded = vector_field_explorer::persistence::load(Some(&storage));
    assert_eq!(
        loaded.view.colormap,
        Colormap::Turbo,
        "Turbo round-trips through APP_KEY"
    );
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

    assert_eq!(
        s.controls.colormap,
        Colormap::Viridis,
        "reset restores the default palette"
    );
    assert!(s.controls.auto_fit, "reset restores auto-fit");
    assert_eq!(s.state.colormap, Colormap::Viridis);
    assert!(s.state.auto_fit);

    let rows = nodes(&out);
    assert!(
        is_toggled(labelled(&rows, egui::Role::Button, "Viridis").expect("Viridis")),
        "the row shows the default selection again"
    );
    assert!(!is_toggled(
        labelled(&rows, egui::Role::Button, "Turbo").expect("Turbo")
    ));
    assert_scene_uses(&mut s.state, Colormap::Viridis);
}

#[test]
fn both_source_display_hides_colormap_and_range_with_a_reason() {
    let mut s = Sidebar::new();
    s.state.set_field_mode(FieldMode::Sources);
    // Manual range, so the range row would be visible if it were not hidden.
    s.controls.auto_fit = false;

    let mut scene = s.state.sources.clone();
    scene.set_display(SourceDisplay::Both);
    s.state.set_source_scene(scene);
    let out = open_coloring(&mut s);
    let rows = nodes(&out);
    assert!(
        labelled(&rows, egui::Role::Button, "Viridis").is_none(),
        "the Both family colours replace the colormap row"
    );
    assert!(
        labelled(&rows, egui::Role::CheckBox, "Auto-fit color range").is_none(),
        "Both has no shared magnitude range to fit"
    );
    assert!(
        !rows
            .iter()
            .any(|node| node.value.as_deref() == Some("Magnitude range:")),
        "the manual range row is hidden in Both"
    );
    assert!(
        rows.iter().any(|node| {
            node.role == egui::Role::Label && node.value.as_deref() == Some(BOTH_COLORING_NOTE)
        }),
        "the Coloring section explains why its controls are absent"
    );

    // A single-family display still uses the shared colormap and range.
    let mut scene = s.state.sources.clone();
    scene.set_display(SourceDisplay::Electric);
    s.state.set_source_scene(scene);
    let out = s.idle();
    let rows = nodes(&out);
    assert!(
        labelled(&rows, egui::Role::Button, "Viridis").is_some(),
        "Electric restores the colormap row"
    );
    assert!(
        labelled(&rows, egui::Role::CheckBox, "Auto-fit color range").is_some(),
        "Electric restores the auto-fit control"
    );
    assert!(
        rows.iter()
            .any(|node| node.value.as_deref() == Some("Magnitude range:")),
        "Electric restores the manual range row"
    );
    assert!(
        !rows
            .iter()
            .any(|node| node.value.as_deref() == Some(BOTH_COLORING_NOTE)),
        "the Both explanation disappears outside Both"
    );
}

fn has_label(nodes: &[Node], text: &str) -> bool {
    nodes
        .iter()
        .any(|node| node.role == egui::Role::Label && node.value.as_deref() == Some(text))
}

fn select_button(s: &mut Sidebar, label: &str) {
    let out = s.idle();
    let rows = nodes(&out);
    let button =
        labelled(&rows, egui::Role::Button, label).unwrap_or_else(|| panic!("the {label} button"));
    s.click(button.center());
    s.idle();
}

/// Click a real button and return the frame after the click has been applied.
fn click_button(s: &mut Sidebar, label: &str) -> egui::FullOutput {
    let out = s.idle();
    let rows = nodes(&out);
    let button =
        labelled(&rows, egui::Role::Button, label).unwrap_or_else(|| panic!("the {label} button"));
    s.click(button.center());
    s.idle()
}

fn button_count(rows: &[Node], label: &str) -> usize {
    rows.iter()
        .filter(|node| node.role == egui::Role::Button && node.label.as_deref() == Some(label))
        .count()
}

fn buttons<'a>(rows: &'a [Node], label: &str) -> Vec<&'a Node> {
    rows.iter()
        .filter(|node| node.role == egui::Role::Button && node.label.as_deref() == Some(label))
        .collect()
}

/// Expand the nested "Custom palette" collapser inside Coloring. The header
/// label is exposed like every other section label; its collapser sits to its
/// left on the same row.
fn open_custom_palette(s: &mut Sidebar) -> egui::FullOutput {
    let out = s.idle();
    let rows = nodes(&out);
    let header = rows
        .iter()
        .find(|node| {
            (node.role == egui::Role::Label && node.value.as_deref() == Some("Custom palette"))
                || node.label.as_deref() == Some("Custom palette")
        })
        .expect("the Custom palette header is exposed");
    s.click(header.center());
    for _ in 0..20 {
        s.idle();
    }
    s.idle()
}

/// The first hex text field in the custom palette editor.
fn hex_field(rows: &[Node]) -> &Node {
    rows.iter()
        .find(|node| {
            node.value
                .as_deref()
                .is_some_and(|value| value.starts_with('#'))
        })
        .unwrap_or_else(|| {
            panic!(
                "a hex text field; values: {:?}",
                rows.iter()
                    .filter_map(|node| node.value.as_deref())
                    .collect::<Vec<_>>()
            )
        })
}

fn remove_one(s: &mut Sidebar) {
    let out = s.idle();
    let rows = nodes(&out);
    let remove = buttons(&rows, "Remove")
        .into_iter()
        .find(|node| !node.disabled)
        .expect("an enabled Remove button");
    s.click(remove.center());
    s.idle();
}

#[test]
fn quantity_selector_applies_component_colouring() {
    let mut s = Sidebar::new();
    let out = open_coloring(&mut s);
    let rows = nodes(&out);
    let x = labelled(&rows, egui::Role::Button, "X component").expect("the X component button");
    s.click(x.center());
    let out = s.idle();

    assert_eq!(
        s.controls.color_quantity,
        ColorQuantity::ComponentX,
        "the mirror follows the quantity click"
    );
    assert_eq!(
        s.state.color_quantity,
        ColorQuantity::ComponentX,
        "apply reaches the viewmodel"
    );
    assert!(s.state.dirty, "the quantity change asks for a rebuild");
    let rows = nodes(&out);
    assert!(
        is_toggled(labelled(&rows, egui::Role::Button, "X component").expect("X component")),
        "the quantity row shows the new selection"
    );
    assert!(
        !is_toggled(labelled(&rows, egui::Role::Button, "Magnitude").expect("Magnitude")),
        "Magnitude is drawn unselected"
    );
}

#[test]
fn direction_quantity_is_disabled_in_3d_and_enabled_in_2d() {
    let mut s = Sidebar::new();
    let out = open_coloring(&mut s);
    let rows = nodes(&out);
    let direction =
        labelled(&rows, egui::Role::Button, "Direction (2D)").expect("the direction button");
    assert!(!direction.disabled, "direction colouring is usable in 2D");
    s.click(direction.center());
    s.idle();
    assert_eq!(s.state.color_quantity, ColorQuantity::Direction2d);

    let mut s = Sidebar::new();
    s.state = State::new(true);
    let out = open_coloring(&mut s);
    let rows = nodes(&out);
    let direction =
        labelled(&rows, egui::Role::Button, "Direction (2D)").expect("the direction button");
    assert!(direction.disabled, "direction colouring is disabled in 3D");
    assert!(
        has_label(
            &rows,
            "Direction colouring is available in 2D; it is disabled in 3D"
        ),
        "the disabled control explains why"
    );
    s.click(direction.center());
    s.idle();
    assert_eq!(
        s.state.color_quantity,
        ColorQuantity::Magnitude,
        "a disabled direction button cannot change the state"
    );
    assert_eq!(s.controls.color_quantity, ColorQuantity::Magnitude);
}

#[test]
fn direction_legend_shows_degree_ticks_without_caps_or_clip_counts() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    select_button(&mut s, "Direction (2D)");
    assert_eq!(s.state.color_quantity, ColorQuantity::Direction2d);

    // Direction colouring wraps and never clips, so a stale mirrored count
    // must not make the legend claim samples fall outside the ramp.
    s.controls.color_clips = ClipCounts { low: 7, high: 9 };
    let _ = s.idle();
    let out = s.idle();
    let rows = nodes(&out);

    for tick in ["0°", "90°", "180°", "270°", "360°"] {
        assert!(
            has_label(&rows, tick),
            "the direction legend paints the degree tick {tick}"
        );
    }
    assert!(
        has_label(&rows, "Range: 0° - 360°"),
        "the direction legend keeps its gradient range"
    );
    assert!(
        !has_label(&rows, "< 0°"),
        "a cyclic ramp has no below-range cap"
    );
    assert!(
        !has_label(&rows, "> 360°"),
        "a cyclic ramp has no above-range cap"
    );
    assert!(
        !rows.iter().any(|node| {
            node.value
                .as_deref()
                .is_some_and(|value| value.contains("below") && value.contains("above"))
        }),
        "a cyclic ramp does not print clip counts"
    );
}

#[test]
fn signed_log_minimum_half_width_renders_legibly() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    select_button(&mut s, "Signed log");
    s.controls.signed_log_half_width = 1.0e-6;
    let _ = s.idle();
    let out = s.idle();
    let rows = nodes(&out);

    assert!(
        rows.iter().any(|node| {
            node.role == egui::Role::SpinButton && node.value.as_deref() == Some("1e-6")
        }),
        "the minimum half-width 1e-6 stays legible rather than rounding to 0.000; values: {:?}",
        rows.iter()
            .filter(|node| node.role == egui::Role::SpinButton)
            .filter_map(|node| node.value.as_deref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn signed_log_half_width_applies_and_reports_unavailable_ranges() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    select_button(&mut s, "Signed log");

    assert!(s.controls.signed_log, "the mirror follows the scale click");
    assert_eq!(
        s.state.color_mode,
        ScaleMode::SignedLog { half_width: 1.0 },
        "apply builds signed log with the default half-width"
    );

    // The default field has no negative values, so the model falls back to
    // linear and the sidebar must say why.
    let out = s.idle();
    let rows = nodes(&out);
    assert!(
        has_label(
            &rows,
            "Signed log colouring needs a range that spans zero; using linear scaling."
        ),
        "an unavailable signed-log range is explained"
    );

    // A range spanning zero makes signed log usable.
    s.controls.auto_fit = false;
    s.controls.color_min = -2.0;
    s.controls.color_max = 2.0;
    s.idle();
    let out = s.idle();
    let rows = nodes(&out);
    assert!(
        !rows.iter().any(|node| {
            node.value
                .as_deref()
                .is_some_and(|value| value.starts_with("Signed log colouring"))
        }),
        "a range spanning zero removes the fallback explanation"
    );

    // The half-width is sanitized through ScaleMode::validate.
    s.controls.signed_log_half_width = f32::NAN;
    s.idle();
    let ScaleMode::SignedLog { half_width } = s.state.color_mode else {
        panic!("signed log must stay selected with a sanitized half-width");
    };
    assert!(
        half_width.is_finite() && half_width > 0.0 && half_width <= 1.0e6,
        "a non-finite half-width becomes valid: {half_width}"
    );
    s.controls.signed_log_half_width = 5.0e7;
    s.idle();
    let ScaleMode::SignedLog { half_width } = s.state.color_mode else {
        panic!("signed log must stay selected with a capped half-width");
    };
    assert_eq!(half_width, 1.0e6, "the half-width is capped at 1e6");
}

#[test]
fn freeze_button_commits_the_effective_range_and_turns_off_auto_fit() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    s.controls.color_stats = ColorStats {
        min_magnitude: 0.5,
        max_magnitude: 4.0,
        max_abs_component: 4.0,
    };
    let out = s.idle();
    let rows = nodes(&out);
    let freeze =
        labelled(&rows, egui::Role::Button, "Freeze current range").expect("the freeze button");
    assert!(!freeze.disabled, "a usable range enables freeze");
    s.click(freeze.center());
    s.idle();

    assert!(!s.state.auto_fit, "freeze turns auto-fit off");
    assert_eq!(s.state.color_min, 0.5);
    assert_eq!(s.state.color_max, 4.0);
    assert!(!s.controls.auto_fit, "the mirror follows auto-fit off");
    assert_eq!(s.controls.color_min, 0.5);
    assert_eq!(s.controls.color_max, 4.0);
    assert!(
        !s.controls.freeze_requested,
        "the freeze request is one-shot"
    );
}

#[test]
fn legend_shows_effective_range_ticks_and_clip_counts() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    s.controls.color_stats = ColorStats {
        min_magnitude: 0.0,
        max_magnitude: 4.0,
        max_abs_component: 4.0,
    };
    s.controls.color_clips = ClipCounts { low: 3, high: 2 };
    let out = s.idle();
    let rows = nodes(&out);

    assert!(has_label(&rows, "Range: 0.00 - 4.00"));
    for tick in ["0.00", "1.00", "2.00", "3.00", "4.00"] {
        assert!(has_label(&rows, tick), "the legend shows tick {tick}");
    }
    assert!(has_label(&rows, "< 0.00"), "the below-range cap is shown");
    assert!(has_label(&rows, "> 4.00"), "the above-range cap is shown");
    assert!(
        has_label(&rows, "3 below, 2 above"),
        "clip counts come from the mirrored facts"
    );
}

#[test]
fn both_display_keeps_the_live_family_note_without_advanced_controls() {
    let mut s = Sidebar::new();
    s.state.set_field_mode(FieldMode::Sources);
    let mut scene = s.state.sources.clone();
    scene.set_display(SourceDisplay::Both);
    s.state.set_source_scene(scene);
    let out = open_coloring(&mut s);
    let rows = nodes(&out);

    assert!(
        has_label(&rows, BOTH_COLORING_NOTE),
        "the family-colour explanation stays"
    );
    for label in ["Quantity", "Linear", "Signed log", "Freeze current range"] {
        assert!(
            !rows.iter().any(|node| {
                (node.role == egui::Role::Button || node.role == egui::Role::Label)
                    && (node.label.as_deref() == Some(label)
                        || node.value.as_deref() == Some(label))
            }),
            "Both must not render the advanced control {label}"
        );
    }
    assert!(
        !rows.iter().any(|node| {
            node.value
                .as_deref()
                .is_some_and(|value| value.starts_with("Range:"))
        }),
        "Both must not render the legend"
    );
}

#[test]
fn both_colour_help_never_promises_fixed_amber_or_cyan() {
    let help = sources_help_rows()
        .iter()
        .find(|row| row.input == "E / B / Both")
        .expect("the E/B/Both help row");
    for text in [BOTH_COLORING_NOTE, BOTH_NOTE, help.action] {
        let lower = text.to_lowercase();
        assert!(
            !lower.contains("amber") && !lower.contains("cyan"),
            "user-facing Both help still names a fixed colour: {text}"
        );
    }
    assert!(
        !BOTH_COLORING_NOTE.contains("E amber") && !BOTH_COLORING_NOTE.contains("B cyan"),
        "the Coloring note must not promise universal family colours"
    );
}

#[test]
fn reset_coloring_restores_quantity_scale_and_auto_fit() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    select_button(&mut s, "X component");
    select_button(&mut s, "Signed log");

    let out = s.idle();
    let rows = nodes(&out);
    let auto_fit =
        labelled(&rows, egui::Role::CheckBox, "Auto-fit color range").expect("auto-fit checkbox");
    s.click(auto_fit.center());
    s.idle();
    assert!(!s.state.auto_fit);

    let out = s.idle();
    let rows = nodes(&out);
    let viridis = labelled(&rows, egui::Role::Button, "Viridis").expect("Viridis");
    let reset = coloring_reset(&rows, viridis);
    s.click(reset.center());
    s.idle();

    assert_eq!(s.controls.color_quantity, ColorQuantity::Magnitude);
    assert_eq!(s.state.color_quantity, ColorQuantity::Magnitude);
    assert_eq!(s.controls.color_mode, ScaleMode::Linear);
    assert_eq!(s.state.color_mode, ScaleMode::Linear);
    assert!(!s.controls.signed_log);
    assert!(s.state.auto_fit);
    assert!(s.controls.auto_fit);
    assert!(!s.controls.freeze_requested);
    assert_eq!(s.controls.color_stats, ColorStats::default());
    assert_eq!(s.controls.color_clips, ClipCounts::default());
}

fn custom_palette_working(s: &Sidebar) -> &ColorRamp {
    s.controls
        .palette_working
        .as_ref()
        .expect("the custom palette draft is open")
}

#[test]
fn editing_a_builtin_seeds_the_nine_stop_approximation() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    click_button(&mut s, "Viridis");
    let out = open_custom_palette(&mut s);
    let rows = nodes(&out);
    assert!(labelled(&rows, egui::Role::Button, "Edit current palette").is_some());
    assert!(has_label(&rows, "Built-in: Viridis"));
    assert_eq!(
        s.controls.palette_text, "",
        "the portable buffer starts empty"
    );

    let out = click_button(&mut s, "Edit current palette");
    let rows = nodes(&out);
    assert_eq!(
        button_count(&rows, "Remove"),
        9,
        "Viridis seeds the nine-stop approximation"
    );
    assert_eq!(custom_palette_working(&s).stops.len(), 9);

    let out = click_button(&mut s, "Apply palette");
    let _ = out;
    assert_eq!(
        s.state.color_ramp,
        Some(ColorRamp::builtin(Colormap::Viridis)),
        "apply installs the seeded approximation"
    );
    assert!(s.state.dirty, "the applied palette dirties the scene");
}

#[test]
fn removing_stops_bottoms_out_at_two() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    open_custom_palette(&mut s);
    click_button(&mut s, "Edit current palette");
    assert_eq!(custom_palette_working(&s).stops.len(), 9);

    for _ in 0..7 {
        remove_one(&mut s);
    }
    assert_eq!(custom_palette_working(&s).stops.len(), 2);

    let out = s.idle();
    let rows = nodes(&out);
    let removes = buttons(&rows, "Remove");
    assert_eq!(removes.len(), 2);
    assert!(
        removes.iter().all(|node| node.disabled),
        "Remove is disabled at the two-stop minimum"
    );
    // A disabled click cannot drop below the minimum.
    s.click(removes[0].center());
    s.idle();
    assert_eq!(custom_palette_working(&s).stops.len(), 2);
}

#[test]
fn adding_stops_is_bounded_and_keeps_positions_increasing() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    open_custom_palette(&mut s);
    click_button(&mut s, "Edit current palette");

    for _ in 0..7 {
        click_button(&mut s, "Add stop");
    }
    assert_eq!(custom_palette_working(&s).stops.len(), ColorRamp::MAX_STOPS);
    let out = s.idle();
    let rows = nodes(&out);
    let add = labelled(&rows, egui::Role::Button, "Add stop").expect("the Add stop button");
    assert!(add.disabled, "Add stop is disabled at the maximum");
    let stops = &custom_palette_working(&s).stops;
    assert!(
        stops
            .windows(2)
            .all(|pair| pair[0].position < pair[1].position),
        "positions stay strictly increasing: {stops:?}"
    );
    // A disabled click cannot exceed the maximum.
    s.click(add.center());
    s.idle();
    assert_eq!(custom_palette_working(&s).stops.len(), ColorRamp::MAX_STOPS);
}

#[test]
fn invalid_hex_does_not_change_the_stop() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    let out = open_custom_palette(&mut s);
    let _ = out;
    let out = click_button(&mut s, "Edit current palette");
    let rows = nodes(&out);
    let field = hex_field(&rows);
    let before = custom_palette_working(&s).stops[0].color;

    s.click(field.center());
    let out = s.frame(vec![egui::Event::Text("#zzzzzz".to_owned())]);
    let rows = nodes(&out);

    assert_eq!(
        custom_palette_working(&s).stops[0].color,
        before,
        "unparseable hex must not touch the stop colour"
    );
    assert!(
        has_label(&rows, "invalid hex"),
        "the row reports invalid hex inline"
    );
    assert!(
        hex_field(&rows)
            .value
            .as_deref()
            .is_some_and(|value| value.contains("#zzzzzz")),
        "the field keeps the invalid text so the user can fix it"
    );
}

#[test]
fn invalid_ramp_disables_apply_and_shows_the_reason() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    open_custom_palette(&mut s);
    click_button(&mut s, "Edit current palette");
    // Drop the 0.5 stop, then a diverging ramp is missing its required centre.
    let out = s.idle();
    let rows = nodes(&out);
    let mut removes = buttons(&rows, "Remove");
    removes.sort_by(|a, b| a.rect.min.y.total_cmp(&b.rect.min.y));
    assert_eq!(removes.len(), 9);
    s.click(removes[4].center());
    s.idle();
    let out = click_button(&mut s, "Diverging");
    assert_eq!(custom_palette_working(&s).kind, RampKind::Diverging);
    let validation = custom_palette_working(&s).validate();
    assert!(validation.is_err(), "the draft is invalid: {validation:?}");
    let rows = nodes(&out);

    let apply = labelled(&rows, egui::Role::Button, "Apply palette").expect("Apply palette");
    assert!(apply.disabled, "an invalid ramp cannot be applied");
    assert!(
        has_label(&rows, "diverging ramps need a stop at exactly 0.5"),
        "the validation reason is shown"
    );
    assert_eq!(custom_palette_working(&s).kind, RampKind::Diverging);
    assert!(
        s.state.color_ramp.is_none(),
        "the invalid draft is not applied"
    );
}

#[test]
fn applying_a_palette_overrides_the_builtin_and_choosing_a_builtin_clears_it() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    open_custom_palette(&mut s);
    click_button(&mut s, "Edit current palette");
    click_button(&mut s, "Apply palette");
    assert!(
        s.state.color_ramp.is_some(),
        "the custom palette overrides the builtin"
    );

    // Later frames must not clobber the override by re-applying the builtin
    // selection.
    s.idle();
    assert!(
        s.state.color_ramp.is_some(),
        "the override survives idle frames"
    );

    // Clicking the *same* built-in still clears the override through the
    // one-shot revert request, even though the selection value does not move.
    click_button(&mut s, "Viridis");
    s.idle();
    assert_eq!(s.state.colormap, Colormap::Viridis);
    assert!(
        s.state.color_ramp.is_none(),
        "re-selecting the same builtin clears the custom override"
    );

    // A different built-in clears the override through `set_colormap`.
    click_button(&mut s, "Edit current palette");
    click_button(&mut s, "Apply palette");
    assert!(s.state.color_ramp.is_some());
    click_button(&mut s, "Turbo");
    s.idle();
    assert_eq!(s.state.colormap, Colormap::Turbo);
    assert!(
        s.state.color_ramp.is_none(),
        "choosing a builtin clears the custom override"
    );
}

#[test]
fn export_text_round_trips_through_apply_text() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    open_custom_palette(&mut s);
    click_button(&mut s, "Edit current palette");
    click_button(&mut s, "Export text");
    let exported_text = s.controls.palette_text.clone();
    let exported = import_ramp(&exported_text).expect("the export parses back");
    assert_eq!(exported, ColorRamp::builtin(Colormap::Viridis));

    // Change the draft after exporting; applying the text must restore it.
    let out = s.idle();
    let rows = nodes(&out);
    let reversed = labelled(&rows, egui::Role::CheckBox, "Reversed").expect("Reversed checkbox");
    s.click(reversed.center());
    s.idle();
    assert!(custom_palette_working(&s).reversed);

    click_button(&mut s, "Apply text");
    assert_eq!(
        s.state.color_ramp.as_ref(),
        Some(&exported),
        "Apply text installs the exported ramp"
    );
    assert_eq!(
        custom_palette_working(&s),
        &exported,
        "Apply text replaces the draft too"
    );
}

#[test]
fn malformed_or_future_text_is_rejected_atomically() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    open_custom_palette(&mut s);
    click_button(&mut s, "Edit current palette");
    let draft = custom_palette_working(&s).clone();

    s.controls.palette_text = "not json".to_owned();
    click_button(&mut s, "Apply text");
    assert!(
        s.controls.palette_message.contains("invalid palette JSON"),
        "a malformed document is named: {:?}",
        s.controls.palette_message
    );
    assert_eq!(custom_palette_working(&s), &draft, "the draft is untouched");
    assert!(s.state.color_ramp.is_none(), "the state is untouched");

    let mut future = PaletteDocument::from_ramp(&draft);
    future.version = 99;
    s.controls.palette_text = future.to_json();
    click_button(&mut s, "Apply text");
    assert!(
        s.controls.palette_message.contains("99")
            && s.controls.palette_message.contains("not supported"),
        "a future version is rejected with a message: {:?}",
        s.controls.palette_message
    );
    assert_eq!(custom_palette_working(&s), &draft, "the draft is untouched");
    assert!(s.state.color_ramp.is_none(), "the state is untouched");
}

#[test]
fn copy_text_is_available_only_with_a_buffer() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    open_custom_palette(&mut s);
    assert_eq!(s.controls.palette_text, "");

    let out = s.idle();
    let rows = nodes(&out);
    let copy = labelled(&rows, egui::Role::Button, "Copy text").expect("Copy text");
    assert!(copy.disabled, "an empty buffer cannot be copied");

    click_button(&mut s, "Export text");
    let out = s.idle();
    let rows = nodes(&out);
    let copy = labelled(&rows, egui::Role::Button, "Copy text").expect("Copy text");
    assert_eq!(
        s.controls.palette_text,
        export_ramp(&ColorRamp::builtin(Colormap::Viridis))
    );
    assert!(!copy.disabled, "a non-empty buffer can be copied");
}

#[test]
fn reset_coloring_clears_the_palette_draft_and_keeps_the_override_when_the_builtin_is_unchanged() {
    let mut s = Sidebar::new();
    let out = open_coloring(&mut s);
    let _ = out;
    open_custom_palette(&mut s);
    click_button(&mut s, "Edit current palette");
    click_button(&mut s, "Apply palette");
    click_button(&mut s, "Export text");
    s.controls.palette_message = "stale message".to_owned();
    let active = s.state.color_ramp.clone();
    assert!(active.is_some());

    let out = s.idle();
    let rows = nodes(&out);
    let viridis = labelled(&rows, egui::Role::Button, "Viridis").expect("Viridis");
    let reset = coloring_reset(&rows, viridis);
    s.click(reset.center());
    s.idle();

    assert_eq!(s.controls.colormap, Colormap::Viridis);
    assert_eq!(s.controls.color_quantity, ColorQuantity::Magnitude);
    assert_eq!(s.controls.color_mode, ScaleMode::Linear);
    assert!(s.controls.auto_fit);
    assert!(s.controls.palette_working.is_none(), "the draft clears");
    assert_eq!(s.controls.palette_text, "", "the portable buffer clears");
    assert_eq!(s.controls.palette_message, "", "the message clears");
    assert!(!s.controls.palette_apply_requested);
    assert_eq!(
        s.state.color_ramp, active,
        "the override survives because the built-in was already viridis"
    );
}

#[test]
fn reset_coloring_clears_the_override_when_it_also_resets_the_builtin() {
    let mut s = Sidebar::new();
    open_coloring(&mut s);
    click_button(&mut s, "Turbo");
    assert_eq!(s.state.colormap, Colormap::Turbo);

    open_custom_palette(&mut s);
    click_button(&mut s, "Edit current palette");
    click_button(&mut s, "Apply palette");
    assert!(
        s.state.color_ramp.is_some(),
        "the custom palette overrides turbo"
    );

    let out = s.idle();
    let rows = nodes(&out);
    let viridis = labelled(&rows, egui::Role::Button, "Viridis").expect("Viridis");
    let reset = coloring_reset(&rows, viridis);
    s.click(reset.center());
    s.idle();

    assert_eq!(s.controls.colormap, Colormap::Viridis);
    assert_eq!(
        s.state.colormap,
        Colormap::Viridis,
        "apply moves the built-in back to viridis"
    );
    assert!(
        s.state.color_ramp.is_none(),
        "resetting a non-viridis built-in discards the override"
    );
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
