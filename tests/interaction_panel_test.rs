//! Issue #88 slice A2b: the Interaction panel.
//!
//! The panel exposes the A1 model: one row per remappable binding with a
//! capture mode and atomic conflict replace, the two camera-feel multipliers
//! and three invert flags, and the built-in/user profile library with portable
//! import/export. Every edit commits directly to the live records; panel drafts
//! and messages stay session-only.

use eframe::egui;

use vector_field_explorer::VfApp;
use vector_field_explorer::interaction::{
    Bindings, CameraFeel, ChordModifiers, InteractionAction, InteractionProfile, KeyChord,
    MAX_CAMERA_MULTIPLIER, MIN_CAMERA_MULTIPLIER, ProfileDocument, ProfileLibrary,
};
use vector_field_explorer::ui::interaction_panel::InteractionPanel;
use vector_field_explorer::ui::layout::{
    DEFAULT_INTERACTION_WIDTH, LayoutMode, LayoutState, MAX_PANEL_WIDTH, SheetState,
};
use vector_field_explorer::ui::menu::{self, MenuAction, SHORTCUTS};

use crate::support;

fn chord(key: egui::Key, command: bool, shift: bool, alt: bool) -> KeyChord {
    KeyChord {
        key,
        modifiers: ChordModifiers {
            command,
            shift,
            alt,
        },
    }
}

fn plain(key: egui::Key) -> KeyChord {
    chord(key, false, false, false)
}

fn command(key: egui::Key) -> KeyChord {
    chord(key, true, false, false)
}

fn context() -> egui::Context {
    let ctx = support::context();
    ctx.enable_accesskit();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn painted_texts(output: &egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

fn accesskit_labels(output: &egui::FullOutput) -> Vec<String> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().map(str::to_owned))
        .collect()
}

fn painted(output: &egui::FullOutput) -> String {
    painted_texts(output)
        .into_iter()
        .chain(accesskit_labels(output))
        .collect::<Vec<_>>()
        .join("\n")
}

fn button_rects(output: &egui::FullOutput, label: &str) -> Vec<egui::Rect> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    let mut rects: Vec<egui::Rect> = update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            if node.role() != egui::Role::Button || node.label() != Some(label) {
                return None;
            }
            let bounds = node.bounds()?;
            Some(egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            ))
        })
        .collect();
    rects.sort_by(|a, b| {
        a.center()
            .y
            .total_cmp(&b.center().y)
            .then(a.center().x.total_cmp(&b.center().x))
    });
    rects
}

fn nth_button_center(output: &egui::FullOutput, label: &str, index: usize) -> Option<egui::Pos2> {
    button_rects(output, label)
        .get(index)
        .map(egui::Rect::center)
}

fn button_disabled(output: &egui::FullOutput, label: &str, index: usize) -> Option<bool> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    let mut nodes: Vec<(f64, f64, bool)> = update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            if node.role() != egui::Role::Button || node.label() != Some(label) {
                return None;
            }
            let (y, x) = node.bounds().map_or((0.0, 0.0), |bounds| {
                ((bounds.y0 + bounds.y1) / 2.0, (bounds.x0 + bounds.x1) / 2.0)
            });
            Some((y, x, node.is_disabled()))
        })
        .collect();
    nodes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    nodes.get(index).map(|(_, _, disabled)| *disabled)
}

struct PanelHarness {
    ctx: egui::Context,
    panel: InteractionPanel,
    live: InteractionProfile,
    profiles: ProfileLibrary,
    width: f32,
    compact: bool,
    time: f64,
}

impl PanelHarness {
    fn new() -> Self {
        Self {
            ctx: context(),
            panel: InteractionPanel::default(),
            live: InteractionProfile::default(),
            profiles: ProfileLibrary::default(),
            width: 420.0,
            compact: false,
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.width, 3200.0),
            )),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let mut output = {
            let panel = &mut self.panel;
            let live = &mut self.live;
            let profiles = &mut self.profiles;
            let compact = self.compact;
            self.ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    panel.show(ui, live, profiles, compact);
                });
            })
        };
        output.textures_delta.clear();
        output
    }

    fn idle(&mut self) -> egui::FullOutput {
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) {
        self.frame(pointer(pos, true));
        self.frame(pointer(pos, false));
    }

    fn click_label(&mut self, label: &str) -> egui::FullOutput {
        let out = self.idle();
        let pos = nth_button_center(&out, label, 0)
            .unwrap_or_else(|| panic!("the {label} button is painted"));
        self.click(pos);
        self.idle()
    }

    fn click_nth_label(&mut self, label: &str, index: usize) -> egui::FullOutput {
        let out = self.idle();
        let pos = nth_button_center(&out, label, index)
            .unwrap_or_else(|| panic!("{label} button {index} is painted"));
        self.click(pos);
        self.idle()
    }

    fn press(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> egui::FullOutput {
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }])
    }
}

fn render_panel(panel: &mut InteractionPanel, live: &mut InteractionProfile) -> egui::FullOutput {
    let mut profiles = ProfileLibrary::default();
    render_panel_with(panel, live, &mut profiles, 420.0, false)
}

fn render_panel_with(
    panel: &mut InteractionPanel,
    live: &mut InteractionProfile,
    profiles: &mut ProfileLibrary,
    width: f32,
    compact: bool,
) -> egui::FullOutput {
    let ctx = context();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 3200.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel.show(ui, live, profiles, compact);
            });
        },
    );
    output.textures_delta.clear();
    output
}

#[test]
fn panel_lists_every_remappable_action_with_its_default_chord() {
    let mut panel = InteractionPanel::default();
    let mut live = InteractionProfile::default();
    let output = render_panel(&mut panel, &mut live);
    let text = painted(&output);

    for action in InteractionAction::REMAPPABLE {
        assert!(
            text.contains(action.label()),
            "the panel renders the {} row:\n{text}",
            action.label()
        );
        let chord = live
            .bindings
            .chord(action)
            .expect("every remappable action has a default");
        assert!(
            text.contains(&chord.label()),
            "the {} row shows its default chord {}:\n{text}",
            action.label(),
            chord.label()
        );
    }
    assert_eq!(
        button_rects(&output, "Rebind").len(),
        InteractionAction::REMAPPABLE.len(),
        "one Rebind control per remappable row"
    );
    assert_eq!(
        button_rects(&output, "Reset action").len(),
        InteractionAction::REMAPPABLE.len(),
        "one Reset action control per remappable row"
    );
}

#[test]
fn rebinding_captures_a_chord_and_assigns_it() {
    let mut h = PanelHarness::new();
    h.click_nth_label("Rebind", 0);
    assert_eq!(
        h.panel.capturing,
        Some(InteractionAction::ToggleControls),
        "Rebind enters capture mode for its row"
    );
    let capturing = h.idle();
    assert_eq!(
        button_disabled(&capturing, "Rebind", 1),
        Some(true),
        "other Rebind buttons are disabled while capturing"
    );

    let out = h.press(egui::Key::J, egui::Modifiers::NONE);

    assert_eq!(h.panel.capturing, None, "a captured key ends capture mode");
    assert_eq!(
        h.live.bindings.chord(InteractionAction::ToggleControls),
        Some(plain(egui::Key::J)),
        "the captured chord is assigned to the row"
    );
    assert!(
        painted(&out).contains("Bound"),
        "the assignment is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn rebinding_captures_modifier_chords_and_escape_cancels() {
    let mut h = PanelHarness::new();
    h.click_nth_label("Rebind", 1);
    h.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(h.panel.capturing, None, "Escape leaves capture mode");
    assert_eq!(
        h.live.bindings.chord(InteractionAction::TogglePresets),
        Some(command(egui::Key::P)),
        "cancelling changes nothing"
    );

    h.click_nth_label("Rebind", 1);
    h.press(
        egui::Key::Y,
        egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    );
    assert_eq!(
        h.live.bindings.chord(InteractionAction::TogglePresets),
        Some(chord(egui::Key::Y, true, true, false)),
        "Mod and Shift are captured into the chord"
    );
}

#[test]
fn rebinding_to_an_in_use_chord_offers_replace_and_swaps_atomically() {
    let mut h = PanelHarness::new();
    h.click_nth_label("Rebind", 0);
    let out = h.press(egui::Key::P, egui::Modifiers::COMMAND);

    assert_eq!(
        h.live.bindings.chord(InteractionAction::ToggleControls),
        Some(command(egui::Key::B)),
        "an in-use chord changes nothing before Replace"
    );
    assert_eq!(
        h.live.bindings.chord(InteractionAction::TogglePresets),
        Some(command(egui::Key::P)),
        "the owner keeps its chord"
    );
    let text = painted(&out);
    assert!(
        text.contains("Presets panel"),
        "the conflict names the owning action:\n{text}"
    );
    assert!(
        text.contains("Replace") && text.contains("Cancel"),
        "the conflict offers Replace and Cancel:\n{text}"
    );

    let out = h.click_label("Replace");

    assert_eq!(
        h.live.bindings.chord(InteractionAction::ToggleControls),
        Some(command(egui::Key::P)),
        "Replace takes the requested chord"
    );
    assert_eq!(
        h.live.bindings.chord(InteractionAction::TogglePresets),
        Some(command(egui::Key::B)),
        "Replace swaps the previous chord to the owner, keeping both bound"
    );
    assert_eq!(h.panel.conflict, None, "the conflict is resolved");
    assert!(
        painted(&out).contains("Bound"),
        "the swap is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn cancelling_a_conflict_leaves_both_bindings_untouched() {
    let mut h = PanelHarness::new();
    h.click_nth_label("Rebind", 0);
    h.press(egui::Key::P, egui::Modifiers::COMMAND);
    h.click_label("Cancel");

    assert_eq!(
        h.live.bindings.chord(InteractionAction::ToggleControls),
        Some(command(egui::Key::B))
    );
    assert_eq!(
        h.live.bindings.chord(InteractionAction::TogglePresets),
        Some(command(egui::Key::P))
    );
    assert_eq!(h.panel.conflict, None);
}

#[test]
fn reserved_actions_are_read_only() {
    let mut panel = InteractionPanel::default();
    let mut live = InteractionProfile::default();
    let output = render_panel(&mut panel, &mut live);
    let text = painted(&output);

    for (action, fixed) in [
        (InteractionAction::Dismiss, "Escape"),
        (InteractionAction::Undo, "Mod+Z"),
        (InteractionAction::Redo, "Mod+Shift+Z"),
    ] {
        assert!(
            text.contains(action.label()),
            "the reserved {} row is shown:\n{text}",
            action.label()
        );
        assert!(
            text.contains(fixed),
            "the reserved {} row shows its fixed chord {fixed}:\n{text}",
            action.label()
        );
    }
    assert_eq!(
        button_rects(&output, "Rebind").len(),
        InteractionAction::REMAPPABLE.len(),
        "reserved rows have no Rebind control"
    );
    assert_eq!(
        button_rects(&output, "Reset action").len(),
        InteractionAction::REMAPPABLE.len(),
        "reserved rows have no Reset action control"
    );
}

#[test]
fn reset_action_restores_the_documented_default() {
    let mut h = PanelHarness::new();
    h.live
        .bindings
        .assign(InteractionAction::ToggleControls, plain(egui::Key::J))
        .expect("J is free");
    let out = h.click_nth_label("Reset action", 0);

    assert_eq!(
        h.live.bindings.chord(InteractionAction::ToggleControls),
        Some(command(egui::Key::B)),
        "Reset action restores Mod+B"
    );
    assert!(
        painted(&out).contains("Reset"),
        "the reset is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn camera_feel_edits_clamp_apply_and_reset() {
    let mut h = PanelHarness::new();
    let out = h.idle();
    let text = painted(&out);
    for label in [
        "Orbit sensitivity",
        "Zoom speed",
        "Invert yaw",
        "Invert pitch",
        "Invert zoom",
        "Reset camera feel",
    ] {
        assert!(text.contains(label), "the panel renders {label:?}:\n{text}");
    }

    h.panel.commit_camera_feel(
        &mut h.live,
        CameraFeel {
            orbit_sensitivity: 10.0,
            zoom_speed: 0.0,
            invert_yaw: true,
            invert_pitch: true,
            invert_zoom: false,
        },
    );
    assert_eq!(
        h.live.camera_feel.orbit_sensitivity, MAX_CAMERA_MULTIPLIER,
        "an over-range multiplier clamps to the maximum"
    );
    assert_eq!(
        h.live.camera_feel.zoom_speed, MIN_CAMERA_MULTIPLIER,
        "an under-range multiplier clamps to the minimum"
    );
    assert!(h.live.camera_feel.invert_yaw && h.live.camera_feel.invert_pitch);

    h.panel.commit_camera_feel(
        &mut h.live,
        CameraFeel {
            orbit_sensitivity: f32::NAN,
            ..CameraFeel::default()
        },
    );
    assert_eq!(
        h.live.camera_feel.orbit_sensitivity, 1.0,
        "a non-finite multiplier sanitizes to the default"
    );

    let out = h.click_label("Reset camera feel");
    assert_eq!(
        h.live.camera_feel,
        CameraFeel::default(),
        "the reset restores the default feel"
    );
    assert!(
        painted(&out).contains("Reset"),
        "the feel reset is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn applying_a_user_profile_replaces_bindings_and_feel_only() {
    let mut h = PanelHarness::new();
    h.live.name = "Live".to_owned();
    let mut work = InteractionProfile {
        name: "Work".to_owned(),
        bindings: Bindings::default_bindings(),
        camera_feel: CameraFeel {
            orbit_sensitivity: 2.5,
            zoom_speed: 0.5,
            invert_yaw: true,
            ..CameraFeel::default()
        },
    };
    work.bindings
        .assign(InteractionAction::ToggleControls, plain(egui::Key::J))
        .expect("J is free");
    h.profiles = ProfileLibrary::new(vec![work.clone()]).expect("the profile is valid");

    let out = h.click_nth_label("Apply", 1);

    assert_eq!(
        h.live.bindings, work.bindings,
        "applying replaces the binding table"
    );
    assert_eq!(
        h.live.camera_feel, work.camera_feel,
        "applying replaces camera feel"
    );
    assert_eq!(
        h.live.name, "Live",
        "applying a profile does not rename the live record"
    );
    assert_eq!(
        h.profiles.user(),
        [work],
        "applying never mutates the profile library"
    );
    assert!(
        painted(&out).contains("Applied Work"),
        "the applied profile is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn saving_a_profile_validates_the_name_and_replaces_in_place() {
    let mut h = PanelHarness::new();
    h.panel.profile_name = "   ".to_owned();
    let out = h.click_label("Save current interaction");
    assert!(h.profiles.is_empty(), "an empty name never saves");
    assert!(
        painted(&out).contains("name"),
        "the empty name is rejected inline:\n{}",
        painted(&out)
    );

    h.panel.profile_name = "Default".to_owned();
    let out = h.click_label("Save current interaction");
    assert!(
        h.profiles.is_empty(),
        "the built-in name never becomes a user profile"
    );
    assert!(
        painted(&out).contains("built-in"),
        "the built-in rejection is painted verbatim:\n{}",
        painted(&out)
    );

    h.panel.profile_name = "First".to_owned();
    h.click_label("Save current interaction");
    assert_eq!(h.profiles.len(), 1);

    h.panel.commit_camera_feel(
        &mut h.live,
        CameraFeel {
            zoom_speed: 2.0,
            ..CameraFeel::default()
        },
    );
    h.panel.profile_name = "First".to_owned();
    let out = h.click_label("Save current interaction");

    assert_eq!(h.profiles.len(), 1, "a same-named save replaces in place");
    assert_eq!(
        h.profiles.user()[0].camera_feel.zoom_speed,
        2.0,
        "the replacement captures the live feel"
    );
    assert!(
        painted(&out).contains("Replaced"),
        "the replacement is reported:\n{}",
        painted(&out)
    );
}

#[test]
fn restoring_defaults_repairs_a_damaged_live_record() {
    let mut h = PanelHarness::new();
    h.live.bindings = serde_json::from_str(
        r#"{"quit":{"key":"Q","modifiers":{"command":true,"shift":false,"alt":false}}}"#,
    )
    .expect("a partial stored table parses");
    h.live.camera_feel = CameraFeel {
        orbit_sensitivity: f32::NAN,
        zoom_speed: 12.0,
        ..CameraFeel::default()
    };

    h.click_label("Restore defaults");

    assert_eq!(
        h.live.bindings,
        Bindings::default_bindings(),
        "Restore defaults installs the documented table even from a damaged record"
    );
    assert_eq!(
        h.live.camera_feel,
        CameraFeel::default(),
        "Restore defaults installs the default feel"
    );
    assert!(h.live.bindings.validate().is_ok());
}

#[test]
fn import_export_round_trip_reports_new_profiles() {
    let mut h = PanelHarness::new();
    let alpha = InteractionProfile {
        name: "Alpha".to_owned(),
        ..InteractionProfile::default()
    };
    h.profiles = ProfileLibrary::new(vec![alpha.clone()]).expect("the profile is valid");

    let before = h.idle();
    assert_eq!(
        button_disabled(&before, "Copy", 0),
        Some(true),
        "Copy is disabled before an export fills the buffer"
    );
    let exported = h.click_label("Export text");
    let document = ProfileDocument::from_json(&h.panel.export_text).expect("the export parses");
    assert_eq!(document.profiles.len(), 1);
    assert_eq!(document.profiles[0], alpha);

    let mut updated = alpha.clone();
    updated.camera_feel.zoom_speed = 3.0;
    let beta = InteractionProfile {
        name: "Beta".to_owned(),
        ..InteractionProfile::default()
    };
    h.panel.import_text = ProfileDocument {
        version: 1,
        profiles: vec![updated.clone(), beta.clone()],
    }
    .to_json()
    .expect("the document serializes");
    let out = h.click_label("Apply text");
    assert_eq!(
        h.profiles.user(),
        [updated.clone(), beta],
        "the import merges in document order"
    );
    assert!(
        painted(&out).contains("Imported 1 new user profile"),
        "the delta over the starting library is reported:\n{}",
        painted(&out)
    );
    assert!(
        painted(&exported).contains("Alpha"),
        "the exported text is painted in the read-only area:\n{}",
        painted(&exported)
    );

    h.panel.import_text = ProfileDocument {
        version: 1,
        profiles: vec![updated],
    }
    .to_json()
    .expect("the document serializes");
    let out = h.click_label("Apply text");
    let text = painted(&out);
    assert!(
        text.contains("no new profiles"),
        "a zero-delta replace reports honestly:\n{text}"
    );
    assert_eq!(h.profiles.len(), 2, "the replacement grows nothing");

    h.panel.import_text = ProfileDocument {
        version: 1,
        profiles: Vec::new(),
    }
    .to_json()
    .expect("the empty document serializes");
    let out = h.click_label("Apply text");
    let text = painted(&out);
    assert!(
        text.contains("Nothing to import"),
        "an empty document says nothing is there to import:\n{text}"
    );
    assert!(
        !text.contains("no new profiles"),
        "an empty document is not painted as a replacement:\n{text}"
    );
}

#[test]
fn compact_interaction_tab_opens_and_switches_the_single_sheet() {
    let ctx = context();
    let mut app = VfApp::new();
    for _ in 0..3 {
        run_app_frame(&ctx, &mut app, Vec::new());
    }

    let interaction = button_center(&run_app_frame(&ctx, &mut app, Vec::new()), "Interaction")
        .expect("Interaction tab");
    run_app_frame(&ctx, &mut app, pointer(interaction, true));
    run_app_frame(&ctx, &mut app, pointer(interaction, false));
    assert_eq!(
        app.sheet,
        vector_field_explorer::ui::layout::SheetState::Interaction,
        "the Interaction tab opens its compact sheet"
    );

    let presets =
        button_center(&run_app_frame(&ctx, &mut app, Vec::new()), "Presets").expect("Presets tab");
    run_app_frame(&ctx, &mut app, pointer(presets, true));
    run_app_frame(&ctx, &mut app, pointer(presets, false));
    assert_eq!(
        app.sheet,
        vector_field_explorer::ui::layout::SheetState::Presets,
        "opening Presets closes the Interaction sheet"
    );

    let interaction = button_center(&run_app_frame(&ctx, &mut app, Vec::new()), "Interaction")
        .expect("Interaction tab");
    run_app_frame(&ctx, &mut app, pointer(interaction, true));
    run_app_frame(&ctx, &mut app, pointer(interaction, false));
    assert_eq!(
        app.sheet,
        vector_field_explorer::ui::layout::SheetState::Interaction,
        "switching back reopens only the Interaction sheet"
    );
}

#[test]
fn switching_the_compact_sheet_away_cancels_capture() {
    let ctx = context();
    let mut app = VfApp::new();
    for _ in 0..3 {
        run_app_frame(&ctx, &mut app, Vec::new());
    }
    app.sheet = SheetState::Interaction;
    run_app_frame(&ctx, &mut app, Vec::new());
    app.interaction_panel
        .begin_capture(InteractionAction::ToggleControls);
    run_app_frame(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.interaction_panel.capturing,
        Some(InteractionAction::ToggleControls),
        "the open Interaction sheet keeps capture armed"
    );

    app.sheet = SheetState::Presets;
    run_app_frame(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.interaction_panel.capturing, None,
        "switching the compact sheet away from Interaction cancels capture"
    );
    assert_eq!(app.interaction_panel.conflict, None);

    app.sheet = SheetState::Interaction;
    run_app_frame(&ctx, &mut app, Vec::new());
    run_app_frame(
        &ctx,
        &mut app,
        vec![key_event(egui::Key::J, egui::Modifiers::NONE)],
    );
    assert_eq!(
        app.interaction
            .bindings
            .chord(InteractionAction::ToggleControls),
        Some(command(egui::Key::B)),
        "a key press after reopening the sheet does not bind"
    );
}

#[test]
fn collapsing_the_wide_panel_cancels_capture() {
    let ctx = context();
    let mut app = VfApp::new();
    let wide = egui::vec2(1400.0, 900.0);
    app.layout.interaction_expanded = true;
    for _ in 0..3 {
        run_app_frame_sized(&ctx, &mut app, Vec::new(), wide);
    }
    assert_eq!(
        app.layout_mode,
        LayoutMode::Wide,
        "the wide fixture draws the persistent panels"
    );
    app.interaction_panel
        .begin_capture(InteractionAction::ToggleControls);
    run_app_frame_sized(&ctx, &mut app, Vec::new(), wide);
    assert_eq!(
        app.interaction_panel.capturing,
        Some(InteractionAction::ToggleControls),
        "the open wide panel keeps capture armed"
    );

    app.layout.interaction_expanded = false;
    run_app_frame_sized(&ctx, &mut app, Vec::new(), wide);
    assert_eq!(
        app.interaction_panel.capturing, None,
        "collapsing the panel to its rail cancels capture"
    );
    assert_eq!(app.interaction_panel.conflict, None);

    app.layout.interaction_expanded = true;
    run_app_frame_sized(&ctx, &mut app, Vec::new(), wide);
    run_app_frame_sized(
        &ctx,
        &mut app,
        vec![key_event(egui::Key::J, egui::Modifiers::NONE)],
        wide,
    );
    assert_eq!(
        app.interaction
            .bindings
            .chord(InteractionAction::ToggleControls),
        Some(command(egui::Key::B)),
        "a key press after reopening the panel does not bind"
    );
}

#[test]
fn typing_in_the_focused_name_field_during_capture_never_binds() {
    let mut h = PanelHarness::new();
    let field = text_input_center(&h.idle()).expect("the profile-name field is painted");
    h.click(field);
    h.idle();
    assert!(
        h.ctx.memory(|memory| memory.focused()).is_some(),
        "clicking the name field gives it keyboard focus"
    );
    h.panel.begin_capture(InteractionAction::ToggleControls);

    h.frame(vec![
        key_event(egui::Key::J, egui::Modifiers::NONE),
        egui::Event::Text("j".to_owned()),
    ]);

    assert_eq!(
        h.panel.profile_name, "j",
        "the typed letter edits the focused name field"
    );
    assert_eq!(
        h.panel.capturing,
        Some(InteractionAction::ToggleControls),
        "a focused text field leaves capture armed for the next real chord"
    );
    assert_eq!(
        h.live.bindings.chord(InteractionAction::ToggleControls),
        Some(command(egui::Key::B)),
        "typing into the name field never binds a chord"
    );
}

#[test]
fn layout_state_defaults_and_clamps_the_interaction_panel() {
    let mut layout = LayoutState::default();
    assert!(
        !layout.interaction_expanded,
        "the Interaction panel starts collapsed to its rail"
    );
    assert_eq!(layout.interaction_width, DEFAULT_INTERACTION_WIDTH);

    layout.toggle_interaction();
    assert!(layout.interaction_expanded);
    layout.toggle_interaction();
    assert!(!layout.interaction_expanded);

    layout.interaction_expanded = true;
    layout.interaction_width = 212.5;
    let restored: LayoutState =
        crate::support::storage::from_ron(&crate::support::storage::ron_string(&layout));
    assert_eq!(restored, layout, "the interaction layout fields round trip");

    let clamped = LayoutState {
        interaction_width: 5000.0,
        ..Default::default()
    }
    .sanitized();
    assert_eq!(clamped.interaction_width, MAX_PANEL_WIDTH);

    let corrupt = LayoutState {
        interaction_width: f32::NAN,
        ..Default::default()
    }
    .sanitized();
    assert_eq!(corrupt.interaction_width, DEFAULT_INTERACTION_WIDTH);

    layout.reset();
    assert_eq!(layout, LayoutState::default());
}

#[test]
fn view_menu_offers_the_interaction_panel_and_displays_its_shortcut() {
    let view = menu::MENUS
        .iter()
        .find(|menu| menu.title == "View")
        .expect("the View menu exists");
    let toggle = view
        .items
        .iter()
        .find(|item| item.action == MenuAction::ToggleInteractionPanel)
        .expect("View > Toggle Interaction exists");
    assert_eq!(toggle.label, "Toggle Interaction");
    assert!(
        view.items
            .iter()
            .any(|item| item.action == MenuAction::OpenInteraction),
        "View carries the open-interaction command too"
    );
    assert_eq!(
        menu::shortcut_label(
            MenuAction::ToggleInteractionPanel,
            &Bindings::default_bindings()
        ),
        Some("Ctrl+Shift+L".to_owned()),
        "the view item displays the documented shortcut"
    );
    assert!(
        !SHORTCUTS
            .iter()
            .any(|shortcut| shortcut.action == MenuAction::ToggleInteractionPanel),
        "the shortcut is displayed only until A3 routes input"
    );
}

fn run_app_frame(
    ctx: &egui::Context,
    app: &mut VfApp,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    run_app_frame_sized(ctx, app, events, egui::vec2(600.0, 800.0))
}

fn run_app_frame_sized(
    ctx: &egui::Context,
    app: &mut VfApp,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) -> egui::FullOutput {
    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| eframe::App::ui(app, ui, &mut native),
    );
    output.textures_delta.clear();
    output
}

fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn button_center(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        if node.role() != egui::Role::Button || node.label() != Some(label) {
            return None;
        }
        let bounds = node.bounds()?;
        Some(egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        ))
    })
}

/// The centre of the panel's single-line text field (the profile name).
fn text_input_center(output: &egui::FullOutput) -> Option<egui::Pos2> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update.nodes.iter().find_map(|(_, node)| {
        if node.role() != egui::Role::TextInput {
            return None;
        }
        let bounds = node.bounds()?;
        Some(egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        ))
    })
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
