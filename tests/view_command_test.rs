// Ticket 18: the pure view-command seam shared by the keyboard, double-click,
// camera buttons and the menu, plus the pointer-button mapping and the
// click-vs-drag decision that the viewport uses.

use eframe::egui;
use glam::{Vec2, Vec3};
use vector_field_explorer::camera::OrbitCamera;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::ui::presets::CAMERA_BUTTONS;
use vector_field_explorer::view_command::{
    self, KEY_ORBIT_STEP, KEY_ZOOM_FACTOR, ViewCommand, ViewContext, apply_view_command,
    command_for_key, domain_center, fit_distance,
};
use vector_field_explorer::viewmodel::{
    ClickOutcome, DragButtons, DragChannel, ViewGesture, apply_camera_input, click_outcome,
    drag_channel,
};

/// The default 3D camera, moved somewhere wild: every reset/fit test starts
/// from a view that no default could accidentally match.
fn wild_camera() -> OrbitCamera {
    OrbitCamera {
        yaw: 2.1,
        pitch: -0.7,
        distance: 12.0,
        target: Vec3::new(7.0, -4.0, 3.0),
        ortho_zoom: 6.0,
        ..OrbitCamera::default()
    }
}

fn rectangular_domain() -> Domain {
    // size (4, 1, 2), centre (-1, 0, 1)
    Domain::new(Vec3::new(-3.0, -0.5, 0.0), Vec3::new(1.0, 0.5, 2.0))
}

// ---------------------------------------------------------------- reset / fit

#[test]
fn reset_returns_the_default_framing_centred_on_the_domain() {
    let square = Domain::default();
    let square_view = ViewContext::new(true, square);
    let reset = apply_view_command(wild_camera(), square_view, ViewCommand::Reset);
    assert_eq!(
        reset,
        OrbitCamera::default(),
        "a centred square domain resets to the exact default camera",
    );

    let rect = rectangular_domain();
    let rect_reset = apply_view_command(wild_camera(), ViewContext::new(true, rect), ViewCommand::Reset);
    assert_eq!(rect_reset.target, Vec3::new(-1.0, 0.0, 1.0), "reset centres on the domain");
    assert_eq!(rect_reset.yaw, OrbitCamera::default().yaw, "reset restores the default orientation");
    assert_eq!(rect_reset.pitch, OrbitCamera::default().pitch);
    assert_eq!(rect_reset.distance, OrbitCamera::default().distance);
    assert_eq!(rect_reset.ortho_zoom, OrbitCamera::default().ortho_zoom);
    assert_eq!(
        rect_reset,
        apply_view_command(wild_camera(), ViewContext::new(true, rect), ViewCommand::Reset),
        "reset is deterministic",
    );

    let flat_reset = apply_view_command(wild_camera(), ViewContext::new(false, rect), ViewCommand::Reset);
    assert_eq!(flat_reset, rect_reset, "the default front view is the same framing in 2D");
}

#[test]
fn fit_frames_square_and_rectangular_domains_deterministically() {
    let camera = wild_camera();
    let half_fov = camera.fov_y * 0.5;

    let square = Domain::default();
    let square_view = ViewContext::new(true, square);
    let fit = apply_view_command(camera, square_view, ViewCommand::Fit);
    let expected = (3.0_f32).sqrt() / half_fov.sin();
    assert!((fit.distance - expected).abs() < 1e-4, "unit cube fits its bounding sphere: {}", fit.distance);
    assert_eq!(fit.target, Vec3::ZERO, "fit centres on the square domain");
    assert_eq!(fit.yaw, camera.yaw, "fit keeps the orbit orientation");
    assert_eq!(fit.pitch, camera.pitch);
    assert_eq!(fit, apply_view_command(camera, square_view, ViewCommand::Fit), "fit is deterministic");

    let rect = rectangular_domain();
    let rect_view = ViewContext::new(true, rect);
    let fit = apply_view_command(camera, rect_view, ViewCommand::Fit);
    // Half the box diagonal (the bounding-sphere radius) over sin(half fov).
    let expected = (21.0_f32).sqrt() * 0.5 / half_fov.sin();
    assert!((fit.distance - expected).abs() < 1e-4, "rectangular box fits its diagonal: {}", fit.distance);
    assert_eq!(fit.target, domain_center(rect), "fit centres on the rectangular domain");
    assert!(
        (fit_distance(rect, camera.fov_y) - fit.distance).abs() < 1e-6,
        "the helper and the command agree",
    );
}

#[test]
fn fit_clamps_the_distance_and_the_2d_view_returns_to_zoom_one() {
    let tiny = Domain::new(Vec3::splat(-0.001), Vec3::splat(0.001));
    let fit = apply_view_command(OrbitCamera::default(), ViewContext::new(true, tiny), ViewCommand::Fit);
    assert!((fit.distance - 0.5).abs() < 1e-5, "a tiny domain cannot pull the eye inside the near plane");

    let huge = Domain::new(Vec3::splat(-100.0), Vec3::splat(100.0));
    let fit = apply_view_command(OrbitCamera::default(), ViewContext::new(true, huge), ViewCommand::Fit);
    assert!((fit.distance - 20.0).abs() < 1e-5, "a huge domain clamps at the far eye distance");

    let rect = rectangular_domain();
    let zoomed = OrbitCamera { ortho_zoom: 3.5, ..wild_camera() };
    let flat = apply_view_command(zoomed, ViewContext::new(false, rect), ViewCommand::Fit);
    assert!((flat.ortho_zoom - 1.0).abs() < 1e-6, "2D fit shows the whole padded plane");
    assert_eq!(flat.target, domain_center(rect), "2D fit centres on the domain");
    assert_eq!(flat.distance, zoomed.distance, "2D fit keeps the eye distance");
}

// ---------------------------------------------------------------- keyboard

#[test]
fn arrow_keys_orbit_in_3d_and_pan_with_shift() {
    let camera = OrbitCamera::default();
    let view = ViewContext::new(true, Domain::default());
    let none = egui::Modifiers::NONE;

    assert_eq!(
        command_for_key(&camera, view, egui::Key::ArrowLeft, none),
        Some(ViewCommand::Orbit { dx: -KEY_ORBIT_STEP, dy: 0.0 }),
        "left arrow orbits left",
    );
    assert_eq!(
        command_for_key(&camera, view, egui::Key::ArrowUp, none),
        Some(ViewCommand::Orbit { dx: 0.0, dy: KEY_ORBIT_STEP }),
        "up arrow raises the eye",
    );
    match command_for_key(&camera, view, egui::Key::ArrowRight, egui::Modifiers::SHIFT) {
        Some(ViewCommand::Pan { dx, dy }) => {
            assert!(dx > 0.0, "shift+right pans right: {dx}");
            assert!(dy.abs() < 1e-6);
        }
        other => panic!("shift+arrow must pan in 3D, got {other:?}"),
    }
}

#[test]
fn arrow_keys_pan_in_2d_and_shift_pans_faster() {
    let camera = OrbitCamera::default();
    let view = ViewContext::new(false, Domain::default());
    let slow = command_for_key(&camera, view, egui::Key::ArrowRight, egui::Modifiers::NONE)
        .expect("2D right arrow pans");
    let fast = command_for_key(&camera, view, egui::Key::ArrowRight, egui::Modifiers::SHIFT)
        .expect("2D shift+right arrow pans");
    let (ViewCommand::Pan { dx: slow_dx, .. }, ViewCommand::Pan { dx: fast_dx, .. }) = (slow, fast)
    else {
        panic!("2D arrows must pan: {slow:?} {fast:?}");
    };
    assert!(slow_dx > 0.0);
    assert!(fast_dx > slow_dx * 2.0, "shift pans faster: {fast_dx} vs {slow_dx}");
    assert!(
        command_for_key(&camera, view, egui::Key::ArrowUp, egui::Modifiers::NONE).is_some(),
        "2D up arrow pans too",
    );
}

#[test]
fn plus_minus_r_and_f_map_to_commands() {
    let camera = OrbitCamera::default();
    let view = ViewContext::new(true, Domain::default());
    let none = egui::Modifiers::NONE;

    assert_eq!(
        command_for_key(&camera, view, egui::Key::Plus, egui::Modifiers::SHIFT),
        Some(ViewCommand::Zoom { factor: KEY_ZOOM_FACTOR }),
        "shift+equals (plus) zooms in",
    );
    assert_eq!(
        command_for_key(&camera, view, egui::Key::Equals, none),
        Some(ViewCommand::Zoom { factor: KEY_ZOOM_FACTOR }),
        "plain equals zooms in too",
    );
    assert_eq!(
        command_for_key(&camera, view, egui::Key::Minus, none),
        Some(ViewCommand::Zoom { factor: 1.0 / KEY_ZOOM_FACTOR }),
        "minus zooms out",
    );
    assert_eq!(command_for_key(&camera, view, egui::Key::R, none), Some(ViewCommand::Reset));
    assert_eq!(command_for_key(&camera, view, egui::Key::F, none), Some(ViewCommand::Fit));
}

#[test]
fn command_modifiers_never_drive_the_camera() {
    // Ctrl+F is the browser/editor find; Ctrl+R is reload. Camera keys must
    // leave every command-modified key alone.
    let camera = OrbitCamera::default();
    let view = ViewContext::new(true, Domain::default());
    for modifiers in [
        egui::Modifiers::COMMAND,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
        egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    ] {
        for key in [
            egui::Key::R,
            egui::Key::F,
            egui::Key::ArrowLeft,
            egui::Key::Plus,
            egui::Key::Minus,
        ] {
            assert_eq!(
                command_for_key(&camera, view, key, modifiers),
                None,
                "{key:?} with {modifiers:?} must not drive the camera",
            );
        }
    }
    assert_eq!(
        command_for_key(&camera, view, egui::Key::R, egui::Modifiers::SHIFT),
        None,
        "shift+R is not reset",
    );
}

#[test]
fn keyboard_orbit_zoom_and_pan_respect_the_camera_clamps() {
    let view = ViewContext::new(true, Domain::default());
    let mut camera = OrbitCamera::default();
    for _ in 0..200 {
        // A negative `dy` raises the eye (the camera's pitch is the polar
        // angle from +Y), so this drives pitch to its upper clamp.
        camera = apply_view_command(camera, view, ViewCommand::Orbit { dx: 0.0, dy: -KEY_ORBIT_STEP });
    }
    assert!((camera.pitch - 1.55).abs() < 1e-5, "orbit pitch clamps at the pole: {}", camera.pitch);
    for _ in 0..400 {
        camera = apply_view_command(camera, view, ViewCommand::Orbit { dx: 0.0, dy: KEY_ORBIT_STEP });
    }
    assert!((camera.pitch + 1.55).abs() < 1e-5, "orbit pitch clamps at the other pole");

    let mut camera = OrbitCamera::default();
    for _ in 0..100 {
        camera = apply_view_command(camera, view, ViewCommand::Zoom { factor: KEY_ZOOM_FACTOR });
    }
    assert!((camera.distance - 0.5).abs() < 1e-5, "zoom-in clamps the orbit distance");
    for _ in 0..200 {
        camera = apply_view_command(camera, view, ViewCommand::Zoom { factor: 1.0 / KEY_ZOOM_FACTOR });
    }
    assert!((camera.distance - 20.0).abs() < 1e-5, "zoom-out clamps the orbit distance");

    let flat = ViewContext::new(false, Domain::default());
    let mut camera = OrbitCamera::default();
    for _ in 0..100 {
        camera = apply_view_command(camera, flat, ViewCommand::Zoom { factor: KEY_ZOOM_FACTOR });
    }
    assert!((camera.ortho_zoom - OrbitCamera::MAX_ORTHO_ZOOM).abs() < 1e-4, "2D zoom-in clamps");
    for _ in 0..300 {
        camera = apply_view_command(camera, flat, ViewCommand::Zoom { factor: 1.0 / KEY_ZOOM_FACTOR });
    }
    assert!((camera.ortho_zoom - OrbitCamera::MIN_ORTHO_ZOOM).abs() < 1e-5, "2D zoom-out clamps");
}

#[test]
fn orbit_commands_are_inert_in_2d() {
    let home = OrbitCamera::default();
    let flat = apply_view_command(home, ViewContext::new(false, Domain::default()), ViewCommand::Orbit {
        dx: 1.0,
        dy: 1.0,
    });
    assert_eq!(flat, home, "2D never orbits");
}

#[test]
fn camera_keys_are_inert_while_a_text_field_has_focus() {
    let camera = OrbitCamera::default();
    let view = ViewContext::new(true, Domain::default());

    // A focused formula field keeps R for typing.
    let typing = run_camera_keys(true, egui::Key::R, egui::Modifiers::NONE, camera, view);
    assert!(typing.is_empty(), "R must not reset while the formula field has focus");

    // Outside a text field the same key resets.
    let idle = run_camera_keys(false, egui::Key::R, egui::Modifiers::NONE, camera, view);
    assert_eq!(idle, vec![ViewCommand::Reset], "R resets outside a text field");

    // Arrows are text navigation while typing, orbit/pan otherwise.
    assert!(run_camera_keys(true, egui::Key::ArrowLeft, egui::Modifiers::NONE, camera, view).is_empty());
    assert_eq!(
        run_camera_keys(false, egui::Key::ArrowLeft, egui::Modifiers::NONE, camera, view),
        vec![ViewCommand::Orbit { dx: -KEY_ORBIT_STEP, dy: 0.0 }],
    );
}

/// Run one egui pass with one camera key pressed, optionally with a focused
/// text field, and report what [`view_command::view_key_commands`] produced.
fn run_camera_keys(
    focus_text: bool,
    key: egui::Key,
    modifiers: egui::Modifiers,
    camera: OrbitCamera,
    view: ViewContext,
) -> Vec<ViewCommand> {
    let ctx = egui::Context::default();
    if focus_text {
        let mut text = String::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.text_edit_singleline(&mut text).request_focus();
        });
        output.textures_delta.clear();
    }
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::ModifiersChanged(modifiers));
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    let mut commands = None;
    let mut output = ctx.run_pass(input, |ctx| {
        if commands.is_none() {
            commands = Some(view_command::view_key_commands(ctx, &camera, view));
        }
    });
    output.textures_delta.clear();
    commands.unwrap_or_default()
}

// ---------------------------------------------------------------- pointers

fn gesture_for(mode_3d: bool, buttons: DragButtons, delta: Vec2) -> ViewGesture {
    match drag_channel(mode_3d, buttons) {
        Some(DragChannel::Orbit) => ViewGesture { drag: delta, ..ViewGesture::default() },
        Some(DragChannel::Pan) => ViewGesture { pan: delta, ..ViewGesture::default() },
        None => ViewGesture::default(),
    }
}

#[test]
fn right_drag_orbits_in_3d_and_pans_in_2d() {
    let right = DragButtons { secondary: true, ..DragButtons::default() };
    assert_eq!(drag_channel(true, right), Some(DragChannel::Orbit), "3D right-drag orbits");
    assert_eq!(drag_channel(false, right), Some(DragChannel::Pan), "2D right-drag pans");

    let home = OrbitCamera::default();
    let delta = Vec2::new(90.0, -40.0);
    let orbited = apply_camera_input(home, true, 2.7, gesture_for(true, right, delta));
    assert_ne!(orbited.yaw, home.yaw, "3D right-drag changes the orbit yaw");
    assert_ne!(orbited.pitch, home.pitch, "3D right-drag changes the orbit pitch");
    assert_eq!(orbited.target, home.target, "3D right-drag does not pan");

    let panned = apply_camera_input(home, false, 2.7, gesture_for(false, right, delta));
    assert_eq!(panned.yaw, home.yaw, "2D right-drag never orbits");
    assert_eq!(panned.pitch, home.pitch);
    assert_ne!(panned.target, home.target, "2D right-drag pans");
}

#[test]
fn middle_and_shift_right_drag_pan_in_3d() {
    let middle = DragButtons { middle: true, ..DragButtons::default() };
    let shift_right = DragButtons { secondary: true, shift: true, ..DragButtons::default() };
    assert_eq!(drag_channel(true, middle), Some(DragChannel::Pan), "3D middle-drag pans");
    assert_eq!(drag_channel(true, shift_right), Some(DragChannel::Pan), "shift+right pans in 3D");

    let home = OrbitCamera::default();
    let delta = Vec2::new(60.0, 25.0);
    for buttons in [middle, shift_right] {
        let panned = apply_camera_input(home, true, 2.7, gesture_for(true, buttons, delta));
        assert_eq!(panned.yaw, home.yaw, "{buttons:?} never orbits");
        assert_eq!(panned.pitch, home.pitch);
        assert_ne!(panned.target, home.target, "{buttons:?} pans");
    }
}

#[test]
fn left_drag_orbits_in_3d_and_pans_in_2d() {
    let left = DragButtons { primary: true, ..DragButtons::default() };
    assert_eq!(drag_channel(true, left), Some(DragChannel::Orbit));
    assert_eq!(drag_channel(false, left), Some(DragChannel::Pan));
    assert_eq!(drag_channel(true, DragButtons::default()), None, "no buttons, no drag");
}

// ---------------------------------------------------------------- clicks

#[test]
fn click_seeds_only_when_the_gesture_never_became_a_drag() {
    assert_eq!(click_outcome(true, false, false), ClickOutcome::Seed, "a plain click seeds");
    assert_eq!(click_outcome(true, false, true), ClickOutcome::None, "a drag never seeds");
    assert_eq!(click_outcome(false, false, false), ClickOutcome::None, "no click, no seed");
    assert_eq!(
        click_outcome(true, true, false),
        ClickOutcome::ResetView,
        "the second click of a double-click resets instead of seeding",
    );
    assert_eq!(click_outcome(false, true, false), ClickOutcome::ResetView);
}

// ---------------------------------------------------------------- buttons

#[test]
fn camera_buttons_run_the_same_commands_as_their_keys() {
    let labels: Vec<&str> = CAMERA_BUTTONS.iter().map(|(label, _)| *label).collect();
    assert_eq!(labels, vec!["Reset view", "Fit domain"], "the camera action buttons are pinned");

    let camera = wild_camera();
    let view = ViewContext::new(true, rectangular_domain());
    for (label, command) in CAMERA_BUTTONS {
        let key = match command {
            ViewCommand::Reset => egui::Key::R,
            ViewCommand::Fit => egui::Key::F,
            other => panic!("{label} uses a non-button command: {other:?}"),
        };
        let by_key = command_for_key(&camera, view, key, egui::Modifiers::NONE)
            .unwrap_or_else(|| panic!("{key:?} must map to a command"));
        assert_eq!(*command, by_key, "{label} matches its keyboard binding");
        assert_eq!(
            apply_view_command(camera, view, *command),
            apply_view_command(camera, view, by_key),
            "{label} and {key:?} produce the same camera",
        );
    }
}
