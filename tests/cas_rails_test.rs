//! Native CAS assembly exercised through real application frames and pointer events.
use eframe::egui;
use vector_field_explorer::VfApp;

struct Harness {
    ctx: egui::Context,
    app: VfApp,
    width: f32,
}
impl Harness {
    fn new(width: f32) -> Self {
        let ctx = crate::support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|s| s.animation_time = 0.0);
        let mut app = VfApp::new();
        app.layout.help_visible = false;
        Self { ctx, app, width }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.width, 1000.0),
                )),
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
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false))
    }
    fn pick(&mut self, label: &str) -> egui::FullOutput {
        let out = self.idle();
        self.click(
            bounds(&out, label)
                .unwrap_or_else(|| panic!("missing widget {label}"))
                .center(),
        )
    }
    fn rail(&mut self, label: &str) -> egui::Rect {
        let out = self.idle();
        rail_bounds(&out, label).unwrap_or_else(|| panic!("missing rail {label}"))
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
fn bounds(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    out.platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .filter_map(|(_, n)| {
            if n.label() != Some(label) && n.value() != Some(label) {
                return None;
            }
            n.bounds().map(|r| {
                (
                    n.role() == egui::Role::Button,
                    egui::Rect::from_min_max(
                        egui::pos2(r.x0 as f32, r.y0 as f32),
                        egui::pos2(r.x1 as f32, r.y1 as f32),
                    ),
                )
            })
        })
        .min_by(|a, b| b.0.cmp(&a.0).then(a.1.left().total_cmp(&b.1.left())))
        .map(|(_, r)| r)
}
fn rail_bounds(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == label && t.angle != 0.0 => Some(
                t.galley
                    .mesh_bounds
                    .rotate_bb(egui::emath::Rot2::from_angle(t.angle))
                    .translate(t.pos.to_vec2()),
            ),
            egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|s| find(&s.shape, label))
}
fn viewport(out: &egui::FullOutput) -> egui::Rect {
    out.shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgb(30, 33, 41) => Some(r.rect),
            _ => None,
        })
        .expect("real viewport fallback background")
}

#[test]
fn cas_rail_is_below_controls_and_independent() {
    let mut h = Harness::new(1280.0);
    assert!(!h.app.layout.cas_expanded && h.app.layout.controls_expanded);
    let controls = h.rail("Controls");
    let cas = h.rail("CAS");
    assert!(cas.top() > controls.bottom());
    h.click(cas.center());
    assert!(h.app.layout.cas_expanded && h.app.layout.controls_expanded);
    h.pick("‹"); // Controls header is first.
    assert!(!h.app.layout.controls_expanded && h.app.layout.cas_expanded);
    assert!(bounds(&h.idle(), "Input (scalar or vector)").is_some());
    let cas = h.rail("CAS");
    h.click(cas.center());
    assert!(!h.app.layout.cas_expanded && !h.app.layout.controls_expanded);
    h.click(cas.center());
    h.pick("‹"); // Only CAS header remains.
    assert!(!h.app.layout.cas_expanded);
}

fn seed(h: &mut Harness) -> String {
    h.app.state.set_formula("(y,-x)".into());
    h.app.cas.input = "x^2+y^2".into();
    h.app.cas.operation = vector_field_explorer::ui::cas_panel::Operation::Gradient;
    h.app.cas.refresh(2);
    h.app.cas.copy_text().unwrap().to_owned()
}
fn menu(h: &mut Harness, group: &str, item: &str) {
    h.pick(if h.width <= 700.0 { "☰" } else { group });
    h.pick(item);
}
fn scratch(h: &Harness, preview: &str) {
    assert_eq!(h.app.cas.input, "x^2+y^2");
    assert_eq!(h.app.cas.copy_text(), Some(preview));
    assert_eq!(h.app.state.formula, "(y,-x)");
}

#[test]
fn cas_preview_survives_panel_and_layout_changes() {
    let mut h = Harness::new(1280.0);
    let preview = seed(&mut h);
    let rail = h.rail("CAS");
    h.click(rail.center());
    h.click(rail.center());
    scratch(&h, &preview);
    menu(&mut h, "View", "Toggle CAS");
    assert!(h.app.layout.cas_expanded);
    scratch(&h, &preview);
    menu(&mut h, "View", "Reset layout");
    assert!(!h.app.layout.cas_expanded);
    scratch(&h, &preview);
    h.width = 360.0;
    h.pick("CAS");
    h.pick("✕");
    h.pick("CAS");
    scratch(&h, &preview);
    h.width = 1280.0;
    menu(&mut h, "Edit", "Reset all controls");
    assert_eq!(h.app.cas.input, "x");
    assert_ne!(h.app.cas.copy_text(), Some(preview.as_str()));
    assert_eq!(h.app.state.formula, "(y,-x)");
    seed(&mut h);
    h.app.controls.density = 37;
    h.app.layout.controls_expanded = false;
    menu(&mut h, "View", "Toggle CAS");
    h.pick("Reset");
    assert_eq!(h.app.cas.input, "x");
    assert_ne!(h.app.cas.copy_text(), Some(preview.as_str()));
    assert_eq!(h.app.controls.density, 37);
    assert_eq!(h.app.state.formula, "(y,-x)");
}

#[test]
fn cas_apply_and_formula_bar_remain_explicit() {
    let mut h = Harness::new(1280.0);
    let preview = seed(&mut h);
    let out = h.idle();
    assert!(
        bounds(&out, "Input (scalar or vector)").is_none(),
        "Controls has no CAS body"
    );
    assert!(bounds(&out, "Symbolic tools (native)").is_none());
    menu(&mut h, "View", "Toggle CAS");
    let copied = h.pick("Copy");
    assert!(
        copied
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c, egui::OutputCommand::CopyText(text) if text == &preview))
    );
    assert_eq!(h.app.state.formula, "(y,-x)");
    h.pick("Apply vector");
    assert_eq!(h.app.state.formula, preview);
    h.idle();
    assert_eq!(h.app.bar.text, preview);
    // Locate the real formula TextEdit by its current value, then select all.
    let out = h.idle();
    let r = out
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, n)| {
            (n.role() == egui::Role::TextInput && n.value() == Some(preview.as_str()))
                .then(|| n.bounds())
                .flatten()
        })
        .expect("formula bar TextEdit");
    h.click(egui::pos2(
        (r.x0 + r.x1) as f32 / 2.0,
        (r.y0 + r.y1) as f32 / 2.0,
    ));
    h.frame(vec![
        egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
        egui::Event::Text("(x,y)".into()),
    ]);
    assert_eq!(h.app.state.formula, "(x,y)");
    let rail = h.rail("CAS");
    h.click(rail.center());
    h.click(rail.center());
    h.idle();
    assert_eq!(h.app.state.formula, "(x,y)");
    assert_eq!(h.app.cas.copy_text(), Some(preview.as_str()));
}

#[test]
fn compact_cas_reuses_single_sheet_without_changing_wide_layout() {
    use vector_field_explorer::ui::layout::{SheetSize, SheetState};
    for width in [320.0, 360.0] {
        let mut h = Harness::new(width);
        h.app.style.font_size = 24.0;
        let before = h.app.layout.clone();
        h.pick("CAS");
        assert_eq!(h.app.sheet, SheetState::Cas);
        h.pick("Controls");
        assert_eq!(h.app.sheet, SheetState::Controls);
        // Scroll the tab row far enough to bring Presets into the viewport.
        // Five tabs overflow at large fonts, and a fixed -400 now overshoots
        // Presets and lands on Controls.
        let out = h.idle();
        let tabs = bounds(&out, "CAS").unwrap();
        let presets_left = bounds(&out, "Presets").unwrap().left();
        let delta = -((presets_left - 12.0).max(0.0));
        h.frame(vec![
            egui::Event::PointerMoved(tabs.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(delta, 0.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
            },
        ]);
        h.pick("Presets");
        assert_eq!(h.app.sheet, SheetState::Presets);
        h.pick("Presets");
        assert_eq!(h.app.sheet, SheetState::None);
        menu(&mut h, "View", "Toggle CAS");
        assert_eq!(h.app.sheet, SheetState::Cas);
        assert_eq!(h.app.sheet_expanded, SheetSize::Default);
        let out = h.idle();
        let close = bounds(&out, "✕").unwrap();
        assert!(close.width() >= 40.0 && close.height() >= 40.0);
        for label in ["Controls", "CAS", "Presets"] {
            if let Some(r) = bounds(&out, label)
                && r.bottom() > 900.0
            {
                // AccessKit reports unscrolled content bounds; hit testing is
                // clipped to the scroll viewport, not those raw bounds.
                let clip = out.shapes.iter().find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == label && t.pos.y > 880.0 => {
                        Some(s.clip_rect)
                    }
                    _ => None,
                });
                if let Some(clip) = clip {
                    let visible = r.intersect(clip);
                    if visible.width() >= 1.0 {
                        assert!(r.width() >= 40.0 && r.height() >= 40.0);
                        assert!(!visible.intersects(close));
                    }
                }
            }
        }
        assert!(viewport(&out).height() >= 448.0);
        h.pick("✕");
        assert_eq!(h.app.sheet, SheetState::None);
        menu(&mut h, "View", "Toggle CAS");
        h.app.sheet_expanded = SheetSize::Collapsed;
        let out = h.idle();
        let handle = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Rect(r)
                    if (r.rect.width() - 36.0).abs() < 0.1
                        && (r.rect.height() - 4.0).abs() < 0.1 =>
                {
                    Some(r.rect.center())
                }
                _ => None,
            })
            .expect("painted sheet swipe handle");
        h.frame(pointer(handle, true));
        let end = handle + egui::vec2(0.0, 30.0);
        h.frame(vec![egui::Event::PointerMoved(end)]);
        h.frame(pointer(end, false));
        assert_eq!(h.app.sheet, SheetState::None, "real down-swipe closes peek");
        assert_eq!(h.app.layout, before);
        h.width = 1280.0;
        h.idle();
        assert_eq!(h.app.layout, before);
    }
}

#[test]
fn wide_squeeze_preserves_requested_widths_and_resize_restore() {
    let mut h = Harness::new(701.0);
    h.app.layout.cas_expanded = true;
    h.app.layout.controls_width = 333.0;
    h.app.layout.cas_width = 310.0;
    h.app.layout.presets_width = 211.0;
    let before = h.app.layout.clone();
    let out = h.idle();
    assert!(viewport(&out).width() >= 99.0);
    let input = bounds(&out, "Input (scalar or vector)").unwrap();
    let presets = bounds(&out, "Reset view").unwrap();
    assert!(input.right() <= viewport(&out).left());
    assert!(presets.left() >= viewport(&out).right());
    assert_eq!(h.app.layout, before);
    h.width = 360.0;
    h.idle();
    assert_eq!(h.app.layout, before);
    h.width = 1280.0;
    let restored = h.idle();
    assert!((viewport(&restored).left() - 685.0).abs() <= 1.0);
    // Right chrome: Presets 211 + the 32 pt Appearance rail + the 32 pt
    // Interaction rail.
    assert!((viewport(&restored).right() - 995.0).abs() <= 1.0);
    let input = bounds(&restored, "Input (scalar or vector)").unwrap();
    let presets = bounds(&restored, "Reset view").unwrap();
    assert!(input.right() < viewport(&restored).left());
    assert!(presets.left() > viewport(&restored).right());
    assert_eq!(h.app.layout, before);
    // Strip 32 + Controls 333 + CAS 310: drag the actual CAS edge.
    let start = egui::pos2(675.0, 500.0);
    let end = start + egui::vec2(35.0, 0.0);
    h.frame(pointer(start, true));
    h.frame(vec![egui::Event::PointerMoved(end)]);
    h.frame(pointer(end, false));
    assert!(
        (h.app.layout.cas_width - 345.0).abs() <= 1.0,
        "drag width {}",
        h.app.layout.cas_width
    );
    let dragged = h.app.layout.cas_width;
    let rail = h.rail("CAS");
    h.click(rail.center());
    h.click(rail.center());
    h.idle();
    let mut storage = crate::support::storage::MemoryStorage::default();
    eframe::App::save(&mut h.app, &mut storage);
    let loaded = VfApp::new_with_storage(Some(&storage));
    assert!((loaded.layout.cas_width - dragged).abs() <= 1.0);
    assert_eq!(loaded.layout.controls_width, 333.0);
    assert_eq!(loaded.layout.presets_width, 211.0);
    h.app = loaded;
    let restored = h.idle();
    assert!((viewport(&restored).left() - (32.0 + 333.0 + dragged + 10.0)).abs() <= 1.0);
    menu(&mut h, "View", "Reset layout");
    h.idle();
    assert!(!h.app.layout.cas_expanded);
    assert_eq!(h.app.layout.cas_width, 260.0);
    menu(&mut h, "View", "Toggle CAS");
    let reset = h.idle();
    assert!(
        (viewport(&reset).left() - 562.0).abs() <= 1.0,
        "reset forgets actual drag geometry"
    );
    assert_eq!(h.app.layout.cas_width, 260.0);
}

#[test]
fn cas_surfaces_remain_native_gated_and_controls_has_no_cas_ownership() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for (path, declaration) in [
        ("src/ui/mod.rs", "pub mod cas_panel;"),
        (
            "src/app/mod.rs",
            "pub cas: crate::ui::cas_panel::CasPanelState,",
        ),
        ("src/ui/layout.rs", "Cas,"),
        ("src/ui/menu.rs", "ToggleCas,"),
        ("src/app/panels.rs", "fn show_cas_body("),
    ] {
        let source = std::fs::read_to_string(root.join(path)).unwrap();
        let prefix = source.split(declaration).next().unwrap();
        assert!(
            prefix
                .trim_end()
                .ends_with("#[cfg(not(target_arch = \"wasm32\"))]"),
            "{path}: {declaration} must be native-only"
        );
    }
    let sidebar = std::fs::read_to_string(root.join("src/ui/sidebar.rs")).unwrap();
    assert!(!sidebar.contains("cas_panel"));
}
