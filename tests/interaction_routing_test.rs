//! Issue #88 slice A3: live bindings and camera feel actually drive input.
//!
//! The default profile must reproduce every existing shortcut; a remapped chord
//! must drive its action while the old chord goes inert; text focus, capture
//! mode and the reserved chords keep their existing safety rules; camera feel
//! scales the existing constants without changing their clamps; and fly input
//! stays on the physical-key path (ticket 70).

use eframe::egui;
use glam::Vec2;

use vector_field_explorer::VfApp;
use vector_field_explorer::camera::{OrbitCamera, feel_orbit, feel_zoom};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::fly_camera::{FlyKeys, fly_toggle_key};
use vector_field_explorer::interaction::{
    BindError, Bindings, CameraFeel, ChordModifiers, InteractionAction, KeyChord,
};
use vector_field_explorer::sim_clock::SimClock;
use vector_field_explorer::ui::menu::{self, MenuAction};
use vector_field_explorer::ui::transport;
use vector_field_explorer::view_command::{self, ViewCommand, ViewContext};
use vector_field_explorer::viewmodel::{State, ViewGesture, apply_camera_input};

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

fn with_modifiers(
    bindings: &Bindings,
    modifiers: egui::Modifiers,
    key: egui::Key,
) -> Vec<MenuAction> {
    let ctx = support::context();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::ModifiersChanged(modifiers));
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    let mut fired = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if fired.is_none() {
            fired = Some(menu::shortcut_actions(ctx, bindings));
        }
    });
    output.textures_delta.clear();
    fired.unwrap_or_default()
}

fn view_for(mode_3d: bool) -> ViewContext {
    ViewContext::new(mode_3d, Domain::default(), 1.0)
}

fn camera_commands(
    bindings: &Bindings,
    feel: CameraFeel,
    modifiers: egui::Modifiers,
    key: egui::Key,
) -> Vec<ViewCommand> {
    let ctx = support::context();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::ModifiersChanged(modifiers));
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    let camera = OrbitCamera::default();
    let view = view_for(true);
    let mut commands = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if commands.is_none() {
            commands = Some(view_command::view_key_commands_with_bindings(
                ctx, &camera, view, bindings, feel,
            ));
        }
    });
    output.textures_delta.clear();
    commands.unwrap_or_default()
}

#[test]
fn default_bindings_preserve_every_existing_shortcut() {
    let bindings = Bindings::default_bindings();
    let cases = [
        (
            egui::Modifiers::COMMAND,
            egui::Key::B,
            MenuAction::ToggleControls,
        ),
        (
            egui::Modifiers::COMMAND,
            egui::Key::P,
            MenuAction::TogglePresets,
        ),
        (
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::A,
            MenuAction::ToggleAppearancePanel,
        ),
        (
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::L,
            MenuAction::ToggleInteractionPanel,
        ),
        (egui::Modifiers::NONE, egui::Key::F1, MenuAction::ToggleHelp),
        (
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::S,
            MenuAction::ExportScreenshot,
        ),
        (
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::C,
            MenuAction::CopyFormula,
        ),
        (egui::Modifiers::COMMAND, egui::Key::Q, MenuAction::Quit),
    ];
    for (modifiers, key, expected) in cases {
        assert_eq!(
            with_modifiers(&bindings, modifiers, key),
            vec![expected],
            "{modifiers:?}+{key:?} must still dispatch {expected:?}",
        );
    }

    // The reserved pair still fires from its static table.
    assert_eq!(
        with_modifiers(&bindings, egui::Modifiers::COMMAND, egui::Key::Z),
        vec![MenuAction::Undo]
    );
    assert_eq!(
        with_modifiers(
            &bindings,
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::Z
        ),
        vec![MenuAction::Redo]
    );

    // Transport, view and tool defaults remain the documented keys.
    assert_eq!(
        bindings.chord(InteractionAction::PlayPause),
        Some(plain(egui::Key::Space))
    );
    assert_eq!(
        bindings.chord(InteractionAction::SeedStreamline),
        Some(plain(egui::Key::S))
    );
    assert_eq!(
        bindings.chord(InteractionAction::AddParticles),
        Some(plain(egui::Key::P))
    );
    assert_eq!(
        camera_commands(
            &bindings,
            CameraFeel::default(),
            egui::Modifiers::NONE,
            egui::Key::R
        ),
        vec![ViewCommand::Reset]
    );
    assert_eq!(
        camera_commands(
            &bindings,
            CameraFeel::default(),
            egui::Modifiers::NONE,
            egui::Key::ArrowLeft
        ),
        vec![ViewCommand::Orbit {
            dx: -view_command::KEY_ORBIT_STEP,
            dy: 0.0,
        }]
    );
}

#[test]
fn remapped_view_chord_drives_its_action_and_the_old_chord_does_nothing() {
    let mut bindings = Bindings::default_bindings();
    bindings
        .assign(InteractionAction::ViewLeft, plain(egui::Key::J))
        .expect("J is free");

    assert_eq!(
        camera_commands(
            &bindings,
            CameraFeel::default(),
            egui::Modifiers::NONE,
            egui::Key::J
        ),
        vec![ViewCommand::Orbit {
            dx: -view_command::KEY_ORBIT_STEP,
            dy: 0.0,
        }],
        "the remapped chord drives ViewLeft",
    );
    assert!(
        camera_commands(
            &bindings,
            CameraFeel::default(),
            egui::Modifiers::NONE,
            egui::Key::ArrowLeft
        )
        .is_empty(),
        "the old arrow chord is now unbound and inert",
    );
}

#[test]
fn remapping_zoom_in_removes_the_equals_alias() {
    let mut bindings = Bindings::default_bindings();
    assert_eq!(
        camera_commands(
            &bindings,
            CameraFeel::default(),
            egui::Modifiers::NONE,
            egui::Key::Equals
        ),
        vec![ViewCommand::Zoom {
            factor: view_command::KEY_ZOOM_FACTOR,
        }],
        "the default Equals alias zooms in",
    );

    bindings
        .assign(InteractionAction::ZoomIn, plain(egui::Key::Y))
        .expect("Y is free");
    assert!(
        camera_commands(
            &bindings,
            CameraFeel::default(),
            egui::Modifiers::NONE,
            egui::Key::Equals
        )
        .is_empty(),
        "after remapping ZoomIn, Equals no longer zooms",
    );
    assert_eq!(
        camera_commands(
            &bindings,
            CameraFeel::default(),
            egui::Modifiers::NONE,
            egui::Key::Y
        ),
        vec![ViewCommand::Zoom {
            factor: view_command::KEY_ZOOM_FACTOR,
        }],
        "the new ZoomIn chord drives the action",
    );
}

#[test]
fn single_key_chords_stay_suppressed_while_a_text_field_has_focus() {
    let ctx = support::context();
    let mut text = String::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
    });
    output.textures_delta.clear();

    let mut input = egui::RawInput::default();
    input
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    input.events.push(egui::Event::Key {
        key: egui::Key::R,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let bindings = Bindings::default_bindings();
    let mut fired = None;
    let mut output = ctx.run_ui(input, |ui| {
        if fired.is_none() {
            fired = Some(menu::shortcut_actions(ui.ctx(), &bindings));
        }
        ui.text_edit_singleline(&mut text);
    });
    output.textures_delta.clear();
    assert_eq!(fired.unwrap_or_default(), Vec::<MenuAction>::new());

    // The label is derived from the bound chord; the text gate suppressed
    // dispatch without dropping the binding.
    let action = menu::interaction_action_for(MenuAction::ToggleControls)
        .expect("the labelled action is remappable");
    assert_eq!(bindings.chord(action), Some(command(egui::Key::B)));
    assert_eq!(
        menu::shortcut_label(MenuAction::ToggleControls, &bindings),
        Some(menu::chord_shortcut_label(command(egui::Key::B))),
        "the label names the chord that is still bound"
    );
}

#[test]
fn global_quit_still_fires_during_text_entry() {
    let ctx = support::context();
    let mut text = String::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
    });
    output.textures_delta.clear();

    let mut input = egui::RawInput::default();
    input
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    input.events.push(egui::Event::Key {
        key: egui::Key::Q,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    let bindings = Bindings::default_bindings();
    let mut fired = None;
    let mut output = ctx.run_ui(input, |ui| {
        if fired.is_none() {
            fired = Some(menu::shortcut_actions(ui.ctx(), &bindings));
        }
        ui.text_edit_singleline(&mut text);
    });
    output.textures_delta.clear();
    assert_eq!(fired.unwrap_or_default(), vec![MenuAction::Quit]);
}

#[test]
fn reserved_dismiss_undo_redo_keep_their_chords_after_a_remap() {
    let mut bindings = Bindings::default_bindings();
    bindings
        .assign(InteractionAction::ToggleControls, plain(egui::Key::K))
        .expect("K is free");

    assert_eq!(
        bindings.assign(InteractionAction::ToggleControls, command(egui::Key::Z)),
        Err(BindError::Reserved),
        "Mod+Z stays reserved for undo",
    );
    assert_eq!(
        bindings.assign(InteractionAction::ToggleControls, plain(egui::Key::Escape)),
        Err(BindError::Reserved),
        "Escape stays reserved for dismiss",
    );
    assert_eq!(bindings.chord(InteractionAction::Dismiss), None);
    assert_eq!(
        with_modifiers(&bindings, egui::Modifiers::COMMAND, egui::Key::Z),
        vec![MenuAction::Undo]
    );
    assert_eq!(
        with_modifiers(
            &bindings,
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::Z
        ),
        vec![MenuAction::Redo]
    );
}

#[test]
fn capture_mode_blocks_the_underlying_shortcuts() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.interaction_expanded = true;
    let before = app.orbit;
    app.interaction_panel
        .begin_capture(InteractionAction::ToggleControls);
    assert!(app.interaction_panel.capturing.is_some());

    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::R,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| eframe::App::ui(&mut app, ui, &mut native),
    );
    output.textures_delta.clear();

    assert_eq!(
        app.orbit, before,
        "R during chord capture must not reset the view"
    );
    let conflict = app
        .interaction_panel
        .conflict
        .expect("the panel consumed R as the captured chord");
    assert_eq!(conflict.owner, InteractionAction::ResetView);
}

#[test]
fn escape_is_consumed_by_an_armed_capture() {
    let ctx = support::context();
    let mut app = VfApp::new();
    app.layout.interaction_expanded = true;
    app.layout.help_visible = true;
    app.interaction_panel
        .begin_capture(InteractionAction::ToggleControls);
    assert!(app.interaction_panel.capturing.is_some());

    let mut native = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| eframe::App::ui(&mut app, ui, &mut native),
    );
    output.textures_delta.clear();

    assert_eq!(
        app.interaction_panel.capturing, None,
        "Escape cancels the armed capture"
    );
    assert_eq!(app.interaction_panel.conflict, None);
    assert_eq!(
        app.interaction_panel.message.as_deref(),
        Some("Cancelled the rebind"),
    );
    assert!(
        app.layout.help_visible,
        "the captured Escape must not also dismiss the help overlay"
    );
}

#[test]
fn remapped_transport_chord_toggles_and_space_goes_inert() {
    let mut bindings = Bindings::default_bindings();
    bindings
        .assign(InteractionAction::PlayPause, plain(egui::Key::K))
        .expect("K is free");
    let mut state = State::new(true);
    state.set_particles(true);
    let mut clock = SimClock::default();

    let ctx = support::context();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key {
        key: egui::Key::K,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let mut toggled = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if toggled.is_none() {
            toggled = Some(transport::toggle_on_bindings(
                ctx, &state, &mut clock, &bindings,
            ));
        }
    });
    output.textures_delta.clear();
    assert_eq!(toggled, Some(true), "the remapped chord toggles playback");
    assert!(!clock.playing, "and pauses the running clock");

    let ctx = support::context();
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let mut toggled = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if toggled.is_none() {
            toggled = Some(transport::toggle_on_bindings(
                ctx, &state, &mut clock, &bindings,
            ));
        }
    });
    output.textures_delta.clear();
    assert_eq!(toggled, Some(false), "the old Space chord is inert");
    assert!(!clock.playing, "and playback is unchanged");
}

#[test]
fn camera_feel_scales_orbit_and_zoom_without_changing_clamps() {
    let feel = CameraFeel {
        orbit_sensitivity: 2.0,
        zoom_speed: 2.0,
        ..CameraFeel::default()
    };
    assert_eq!(
        feel_orbit(Vec2::new(10.0, -4.0), feel),
        Vec2::new(20.0, -8.0)
    );
    let scaled_zoom = feel_zoom(1.2, feel);
    assert!((scaled_zoom - 1.44).abs() < 1e-5, "got {scaled_zoom}");

    // The multiplier scales the existing orbit step, not a new constant.
    let mut bindings = Bindings::default_bindings();
    bindings
        .assign(InteractionAction::ViewLeft, plain(egui::Key::J))
        .unwrap();
    assert_eq!(
        camera_commands(&bindings, feel, egui::Modifiers::NONE, egui::Key::J),
        vec![ViewCommand::Orbit {
            dx: -2.0 * view_command::KEY_ORBIT_STEP,
            dy: 0.0,
        }]
    );

    // Extreme feel still lands on the existing camera clamps.
    let wild = CameraFeel {
        orbit_sensitivity: 4.0,
        zoom_speed: 4.0,
        ..CameraFeel::default()
    };
    let gesture = ViewGesture {
        drag: feel_orbit(Vec2::new(0.0, 10_000.0), wild),
        zoom: feel_zoom(0.0001, wild),
        ..ViewGesture::default()
    };
    let camera = apply_camera_input(OrbitCamera::default(), true, 2.0, gesture);
    assert!((camera.pitch - -1.55).abs() < 1e-6, "pitch clamp survives");
    assert!(
        (camera.distance - OrbitCamera::MAX_DISTANCE).abs() < 1e-5,
        "distance clamp survives, got {}",
        camera.distance
    );
}

#[test]
fn camera_feel_invert_flags_flip_orbit_and_zoom_directions() {
    let inverted = CameraFeel {
        invert_yaw: true,
        invert_pitch: true,
        invert_zoom: true,
        ..CameraFeel::default()
    };
    assert_eq!(
        feel_orbit(Vec2::new(3.0, 5.0), inverted),
        Vec2::new(-3.0, -5.0)
    );
    let zoom = feel_zoom(1.5, inverted);
    assert!((zoom - 1.0 / 1.5).abs() < 1e-6, "got {zoom}");

    // Sanitizing an out-of-range live record happens before any scaling.
    let corrupt = CameraFeel {
        orbit_sensitivity: f32::NAN,
        zoom_speed: 1000.0,
        ..CameraFeel::default()
    };
    assert_eq!(
        feel_orbit(Vec2::new(2.0, 2.0), corrupt),
        Vec2::new(2.0, 2.0)
    );
    assert_eq!(
        feel_zoom(2.0, corrupt),
        2.0_f32.powf(vector_field_explorer::interaction::MAX_CAMERA_MULTIPLIER)
    );
    assert_eq!(
        CameraFeel {
            zoom_speed: f32::NAN,
            ..corrupt
        }
        .sanitized()
        .zoom_speed,
        1.0,
        "a non-finite multiplier sanitizes to the neutral 1.0",
    );
    assert_eq!(
        feel_zoom(1.0, corrupt),
        1.0,
        "a 1.0 wheel factor is unchanged even by a clamped speed",
    );
}

#[test]
fn fly_keys_stay_physical_on_a_non_qwerty_layout() {
    // Bone maps physical A to logical R; the fly camera must still strafe and
    // the physical C position must still toggle fly.
    let mut keys = FlyKeys::default();
    keys.apply_key(egui::Key::R, Some(egui::Key::A), true);
    assert!(keys.left, "the physical A position strafes");
    keys.apply_key(egui::Key::R, Some(egui::Key::A), false);
    assert!(!keys.left);

    let event = egui::Event::Key {
        key: egui::Key::R,
        physical_key: Some(egui::Key::C),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    assert_eq!(
        fly_toggle_key(&[event]),
        Some(egui::Key::R),
        "the physical C position toggles fly even when it reports logical R"
    );
}

#[test]
fn menu_shortcut_labels_follow_the_active_bindings() {
    let mut bindings = Bindings::default_bindings();
    bindings
        .assign(InteractionAction::ToggleControls, command(egui::Key::K))
        .unwrap();
    bindings
        .assign(
            InteractionAction::ToggleInteraction,
            chord(egui::Key::M, true, true, false),
        )
        .unwrap();

    assert_eq!(
        menu::shortcut_label(MenuAction::ToggleControls, &bindings),
        Some("Ctrl+K".to_owned())
    );
    assert_eq!(
        menu::shortcut_label(MenuAction::ToggleInteractionPanel, &bindings),
        Some("Ctrl+Shift+M".to_owned())
    );
}
