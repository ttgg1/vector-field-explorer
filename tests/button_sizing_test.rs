//! Exercise owning UI surfaces, not replicas of their sizing rules.
use eframe::egui;
use vector_field_explorer::{
    screenshot::Screenshot,
    sim_clock::SimClock,
    ui::{formula_bar, help_overlay, layout, style_panel, top_bar, transport},
    viewmodel::State,
};

fn context(font_size: f32) -> egui::Context {
    let ctx = crate::support::context();
    ctx.enable_accesskit();
    style_panel::apply(
        &ctx,
        &style_panel::AppStyle {
            font_size,
            ..Default::default()
        },
    );
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn frame(
    ctx: &egui::Context,
    compact: bool,
    events: Vec<egui::Event>,
    mut draw: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    frame_at(
        ctx,
        if compact { 360.0 } else { 1400.0 },
        6000.0,
        events,
        &mut draw,
    )
}

fn frame_at(
    ctx: &egui::Context,
    width: f32,
    height: f32,
    events: Vec<egui::Event>,
    mut draw: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            events,
            ..Default::default()
        },
        |ui| draw(ui),
    );
    output.textures_delta.clear();
    output
}

fn button(output: &egui::FullOutput, label: &str) -> egui::Rect {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.role() == egui::Role::Button && node.label() == Some(label)).then(|| {
                let b = node.bounds().expect("button bounds");
                egui::Rect::from_min_max(
                    egui::pos2(b.x0 as f32, b.y0 as f32),
                    egui::pos2(b.x1 as f32, b.y1 as f32),
                )
            })
        })
        .unwrap_or_else(|| panic!("missing real button {label}"))
}

fn assert_text_fits(output: &egui::FullOutput, label: &str, compact: bool) -> egui::Rect {
    let rect = button(output, label);
    assert_text_in_rect(output, label, rect, compact);
    rect
}

fn assert_text_in_rect(output: &egui::FullOutput, label: &str, rect: egui::Rect, compact: bool) {
    if compact {
        assert!(rect.height() >= 40.0, "{label} compact hitbox {rect:?}");
    }
    let text = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.text() == label && rect.intersects(text.visual_bounding_rect()) =>
            {
                Some((text, shape.clip_rect))
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "missing painted button text {label} at {rect:?}; paint {:?}",
                output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) if t.galley.text() == label =>
                            Some(t.visual_bounding_rect()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            )
        });
    let (text, clip) = text;
    assert!(
        clip.expand(1.0).contains_rect(rect),
        "{label} hitbox {rect:?} clipped by {clip:?}"
    );
    let ink = text.visual_bounding_rect();
    // Glyph meshes include antialiasing fringes; permit at most one physical
    // point of raster rounding, but require positive layout padding as well.
    assert!(
        rect.expand(1.0).contains_rect(ink),
        "{label} ink {ink:?} outside hitbox {rect:?}; pos {:?}, galley {:?}, mesh {:?}, clip {clip:?}",
        text.pos,
        text.galley.rect,
        text.galley.mesh_bounds
    );
    assert!(
        clip.expand(1.0).contains_rect(ink),
        "{label} ink {ink:?} clipped by {clip:?}"
    );
    assert!(
        rect.height() >= text.galley.size().y + 2.0,
        "{label} lacks vertical padding: {rect:?}"
    );
    assert!(
        rect.width() >= text.galley.size().x + 2.0,
        "{label} lacks horizontal padding: {rect:?}"
    );
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

#[test]
fn real_formula_transport_and_help_buttons_fit_and_grow() {
    for compact in [false, true] {
        let mut heights = Vec::new();
        for font in [8.0, 13.0, 24.0] {
            let ctx = context(font);
            let mut state = State::new(false);
            let mut bar = formula_bar::FormulaBarState::default();
            let mut clock = SimClock::default();
            clock.playing = false;
            let mut draw = |ui: &mut egui::Ui| {
                formula_bar::show(ui, &mut state, &mut bar, compact);
                transport::show(ui, &mut clock, &state, compact);
                layout::collapse_button(ui, layout::RailSide::Left, "Ctrl+B");
                help_overlay::show(ui.ctx(), egui::pos2(0.0, 1500.0), false);
            };
            for _ in 0..3 {
                frame(&ctx, compact, vec![], &mut draw);
            }
            let out = frame(&ctx, compact, vec![], &mut draw);
            let labels = [
                "3D view",
                "2D view",
                transport::play_label(false, compact),
                transport::reset_label(compact),
                layout::collapse_chevron(layout::RailSide::Left),
                "×",
            ];
            heights.push(labels.map(|label| assert_text_fits(&out, label, compact).height()));
        }
        for (index, small) in heights[0].iter().enumerate() {
            assert!(heights[1][index] >= *small);
            assert!(
                heights[2][index] > heights[1][index],
                "family {index} must grow when font 24 requires more room"
            );
        }
    }
}

#[test]
fn real_appearance_selectables_and_swatch_hitboxes_respond_to_fonts() {
    for compact in [false, true] {
        let mut sizes = Vec::new();
        for font in [8.0, 13.0, 24.0] {
            let ctx = context(font);
            let mut style = style_panel::AppStyle {
                font_size: font,
                ..Default::default()
            };
            let mut draw = |ui: &mut egui::Ui| {
                style_panel::show_controls(ui, &mut style);
            };
            frame(&ctx, compact, vec![], &mut draw);
            let out = frame(&ctx, compact, vec![], &mut draw);
            assert_text_fits(&out, "Light", compact);
            assert_text_fits(&out, "Dark", compact);
            let color = out
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.role() == egui::Role::ColorWell).then(|| node.bounds().unwrap())
                })
                .unwrap();
            assert!(
                color.y1 - color.y0 >= if compact { 40.0 } else { 19.0 },
                "custom picker button must share the compact floor"
            );
            if font == 24.0 {
                assert!(
                    color.y1 - color.y0 > 40.0,
                    "custom picker button must grow at the maximum font"
                );
            }
            for preset in style_panel::ACCENT_PRESETS {
                let rect = button(&out, preset.name);
                assert!(rect.height() >= if compact { 40.0 } else { 19.0 });
                let fills: Vec<_> = out
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(painted) if painted.fill == preset.color => {
                            Some((painted.rect, shape.clip_rect))
                        }
                        _ => None,
                    })
                    .collect();
                assert!(!fills.is_empty(), "actual swatch paint");
                for (paint, clip) in fills {
                    assert!(rect.contains_rect(paint));
                    assert!(clip.contains_rect(paint));
                }
            }
            sizes.push(button(&out, "Teal").size());
        }
        assert!(
            sizes[2].x > sizes[1].x && sizes[2].y > sizes[1].y,
            "swatches must grow with their font-derived icon/padding"
        );
    }
}

#[test]
fn real_top_bar_and_open_menu_buttons_fit_supported_fonts() {
    for (compact, width) in [(false, 1400.0), (true, 320.0), (true, 360.0), (true, 390.0)] {
        for font in [8.0, 13.0, 24.0] {
            let ctx = context(font);
            let mut style = style_panel::AppStyle {
                font_size: font,
                ..Default::default()
            };
            let screenshot = Screenshot::new();
            let mut appearance = false;
            let mut draw = |ui: &mut egui::Ui| {
                egui::Panel::top("button_test_top").show(ui, |ui| {
                    if compact {
                        top_bar::show_compact(ui, &screenshot, false);
                    } else {
                        top_bar::show(ui, &mut style, &screenshot, &mut appearance, false);
                    }
                });
            };
            frame_at(&ctx, width, 1600.0, vec![], &mut draw);
            let out = frame_at(&ctx, width, 1600.0, vec![], &mut draw);
            let opener = if compact {
                top_bar::HAMBURGER_LABEL
            } else {
                "File"
            };
            for label in [opener, "Screenshot"] {
                let rect = assert_text_fits(&out, label, compact);
                assert!(
                    rect.left() >= 0.0 && rect.right() <= width,
                    "{label} exceeds available width: {rect:?}"
                );
            }
            if compact {
                let labels = [opener, "GitHub", "Screenshot"];
                for (index, label) in labels.iter().enumerate() {
                    let rect = button(&out, label);
                    for other in &labels[index + 1..] {
                        assert!(
                            !rect.intersects(button(&out, other)),
                            "{label} overlaps {other} at font {font}"
                        );
                    }
                }
            }
            let pos = button(&out, opener).center();
            frame_at(&ctx, width, 1600.0, pointer(pos, true), &mut draw);
            frame_at(&ctx, width, 1600.0, pointer(pos, false), &mut draw);
            for _ in 0..3 {
                frame_at(&ctx, width, 1600.0, vec![], &mut draw);
            }
            let out = frame_at(&ctx, width, 1600.0, vec![], &mut draw);
            assert_text_fits(&out, "Export screenshot (SVG)…", compact);
        }
    }
}

#[test]
fn real_sidebar_buttons_fit_in_wide_and_compact_panels() {
    use vector_field_explorer::ui::sidebar::{self, Controls};
    for compact in [false, true] {
        for font in [8.0, 13.0, 24.0] {
            let ctx = context(font);
            let mut state = State::new(false);
            let mut controls = Controls::default();
            let mut draw = |ui: &mut egui::Ui| {
                egui::Panel::left("button_sidebar")
                    .exact_size(if compact { 360.0 } else { 460.0 })
                    .show(ui, |ui| {
                        sidebar::show_compact(ui, &mut state, &mut controls, compact);
                    });
            };
            for _ in 0..3 {
                frame(&ctx, compact, vec![], &mut draw);
            }
            let out = frame(&ctx, compact, vec![], &mut draw);
            for label in ["Reset", "Magnitude", "Proportional", "Uniform"] {
                assert_text_fits(&out, label, compact);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn real_presets_actions_selectables_and_confirmation_buttons_fit() {
    use vector_field_explorer::{render::Camera, ui::presets::PresetPanel};
    for compact in [false, true] {
        for font in [8.0, 13.0, 24.0] {
            let ctx = context(font);
            let dir = tempfile::tempdir().unwrap();
            let mut panel = PresetPanel::new_with_dir(dir.path().to_path_buf()).unwrap();
            let mut camera = Camera::default();
            panel.save_current("Home", &camera).unwrap();
            panel.request_delete("Home");
            let mut state = State::new(false);
            let mut draw = |ui: &mut egui::Ui| {
                egui::Panel::left("button_presets")
                    .exact_size(if compact { 360.0 } else { 460.0 })
                    .show(ui, |ui| {
                        panel.show_compact(ui, &mut camera, &mut state, compact);
                    });
            };
            for _ in 0..3 {
                frame(&ctx, compact, vec![], &mut draw);
            }
            let out = frame(&ctx, compact, vec![], &mut draw);
            for label in [
                "Reset view",
                "Fit domain",
                "Home",
                "Delete",
                "Cancel",
                "Save current view",
            ] {
                assert_text_fits(&out, label, compact);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn real_cas_buttons_and_selector_fit_supported_fonts_and_compact_floor() {
    use vector_field_explorer::ui::cas_panel::{self, CasPanelState};
    for compact in [false, true] {
        for font in [8.0, 13.0, 24.0] {
            let ctx = context(font);
            let mut state = State::new(false);
            let mut panel = CasPanelState::default();
            let mut draw = |ui: &mut egui::Ui| {
                egui::Panel::left("button_cas")
                    .exact_size(if compact { 360.0 } else { 460.0 })
                    .show(ui, |ui| {
                        if compact {
                            ui.spacing_mut().interact_size.y = 40.0;
                        }
                        cas_panel::show(ui, &mut state, &mut panel);
                    });
            };
            let out = frame(&ctx, compact, vec![], &mut draw);
            let header = out
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some("Symbolic tools (native)"))
                        .then(|| node.bounds().unwrap())
                })
                .unwrap();
            let pos = egui::pos2(
                ((header.x0 + header.x1) / 2.0) as f32,
                ((header.y0 + header.y1) / 2.0) as f32,
            );
            frame(&ctx, compact, pointer(pos, true), &mut draw);
            frame(&ctx, compact, pointer(pos, false), &mut draw);
            for _ in 0..3 {
                frame(&ctx, compact, vec![], &mut draw);
            }
            let out = frame(&ctx, compact, vec![], &mut draw);
            for label in ["Reset", "Copy", "x", "y", "z", "t"] {
                assert_text_fits(&out, label, compact);
            }
            let combo = out
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.role() == egui::Role::ComboBox).then(|| node.bounds().unwrap())
                })
                .unwrap();
            let rect = egui::Rect::from_min_max(
                egui::pos2(combo.x0 as f32, combo.y0 as f32),
                egui::pos2(combo.x1 as f32, combo.y1 as f32),
            );
            assert_text_in_rect(&out, "Differentiate", rect, compact);
        }
    }
}

#[test]
fn real_compact_app_tabs_keep_large_font_ink_inside_the_panel() {
    for font in [8.0, 13.0, 24.0] {
        let ctx = context(font);
        let mut app = vector_field_explorer::VfApp::new();
        app.style.font_size = font;
        let mut eframe = eframe::Frame::_new_kittest();
        let mut draw = |ui: &mut egui::Ui| eframe::App::ui(&mut app, ui, &mut eframe);
        for _ in 0..3 {
            frame_at(&ctx, 360.0, 800.0, vec![], &mut draw);
        }
        let out = frame_at(&ctx, 360.0, 800.0, vec![], &mut draw);
        assert_text_fits(&out, "Controls", true);
        assert_text_fits(&out, "Presets", true);
        let pos = button(&out, "Controls").center();
        frame_at(&ctx, 360.0, 800.0, pointer(pos, true), &mut draw);
        frame_at(&ctx, 360.0, 800.0, pointer(pos, false), &mut draw);
        for _ in 0..3 {
            frame_at(&ctx, 360.0, 800.0, vec![], &mut draw);
        }
        let out = frame_at(&ctx, 360.0, 800.0, vec![], &mut draw);
        assert_text_fits(&out, "✕", true);
    }
}

fn app_frame(
    app: &mut vector_field_explorer::VfApp,
    ctx: &egui::Context,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut native_frame = eframe::Frame::_new_kittest();
    frame_at(ctx, width, 800.0, events, |ui| {
        eframe::App::ui(app, ui, &mut native_frame);
    })
}

fn settled_app_frame(
    app: &mut vector_field_explorer::VfApp,
    ctx: &egui::Context,
    width: f32,
) -> egui::FullOutput {
    for _ in 0..3 {
        app_frame(app, ctx, width, vec![]);
    }
    app_frame(app, ctx, width, vec![])
}

#[test]
fn compact_app_full_transport_labels_fit_paused_and_playing_and_keep_clock_actions() {
    for width in [320.0, 360.0] {
        for font in [8.0, 13.0, 24.0] {
            for playing in [false, true] {
                let ctx = context(font);
                let mut app = vector_field_explorer::VfApp::new();
                app.style.font_size = font;
                app.layout.help_visible = false;
                app.state.set_formula("(x+t, y)".into());
                app.clock.playing = playing;
                app.clock.time = 12.0;
                app.clock.set_speed(1.5);

                let out = settled_app_frame(&mut app, &ctx, width);
                // The actual app passes compact=false to the transport: keep
                // those full labels rather than substituting an icon fixture.
                let play = assert_text_fits(&out, transport::play_label(playing, false), true);
                let reset = assert_text_fits(&out, "↺ Reset time", true);
                assert!(!play.intersects(reset));
                assert_eq!(app.clock.playing, playing);
                assert_eq!(app.clock.speed, 1.5);
                assert!(app.clock.time >= 12.0);

                app_frame(&mut app, &ctx, width, pointer(play.center(), true));
                app_frame(&mut app, &ctx, width, pointer(play.center(), false));
                assert_eq!(app.clock.playing, !playing, "actual Play/Pause click");
                let out = settled_app_frame(&mut app, &ctx, width);
                assert_text_fits(&out, transport::play_label(!playing, false), true);
                let reset = assert_text_fits(&out, "↺ Reset time", true);
                app_frame(&mut app, &ctx, width, pointer(reset.center(), true));
                app_frame(&mut app, &ctx, width, pointer(reset.center(), false));
                assert_eq!(app.clock.time, 0.0, "actual Reset time click");
                assert_eq!(app.clock.playing, !playing);
                assert_eq!(app.clock.speed, 1.5);
                assert_eq!(app.state.formula, "(x+t, y)");
            }
        }
    }
}

#[test]
fn wide_app_menu_and_screenshot_hitboxes_do_not_overlap_at_the_breakpoint() {
    for width in [701.0, 960.0, 1280.0] {
        for font in [8.0, 13.0, 24.0] {
            let ctx = context(font);
            let mut app = vector_field_explorer::VfApp::new();
            app.style.font_size = font;
            app.layout.help_visible = false;
            let out = settled_app_frame(&mut app, &ctx, width);
            let labels = [
                "File",
                "Edit",
                "View",
                "Help",
                top_bar::APPEARANCE_BUTTON_LABEL,
                "Screenshot as SVG",
                "Screenshot",
            ];
            let rects = labels.map(|label| assert_text_fits(&out, label, false));
            for (index, rect) in rects.iter().enumerate() {
                for (other_index, other) in rects.iter().enumerate().skip(index + 1) {
                    assert!(
                        !rect.intersects(*other),
                        "{} {rect:?} overlaps {} {other:?} at width {width}, font {font}",
                        labels[index],
                        labels[other_index],
                    );
                }
            }
        }
    }
}

#[test]
fn assembled_wide_app_presets_camera_buttons_fit_the_space_left_by_controls() {
    for width in [701.0, 960.0, 1280.0] {
        for font in [8.0, 13.0, 24.0] {
            let ctx = context(font);
            let mut app = vector_field_explorer::VfApp::new();
            app.style.font_size = font;
            app.layout.help_visible = false;
            let out = settled_app_frame(&mut app, &ctx, width);
            // Do not give Presets an isolated generous width: the real Controls
            // panel must consume its share of the same 701-point root first.
            let reset = assert_text_fits(&out, "Reset view", false);
            let fit = assert_text_fits(&out, "Fit domain", false);
            assert!(!reset.intersects(fit));
            assert!(
                fit.right() <= width,
                "Fit domain must remain inside the actual app"
            );
        }
    }
}
