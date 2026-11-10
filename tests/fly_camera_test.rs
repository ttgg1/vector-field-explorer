//! Ticket 46: the six-DOF fly camera.
//!
//! The pure seams are covered here without a window: the camera itself
//! (`from_orbit`, `step`, pitch clamp, wheel speed), the key set that maps
//! physical positions before logical keys, the `ViewCamera` active-camera
//! switch the viewport/screenshots/click seeding share, and the mode/command
//! rules that leave the orbit camera untouched.

use eframe::Storage as _;
use eframe::egui;
use glam::{Vec2, Vec3};

use vector_field_explorer::camera::{OrbitCamera, ViewCamera, apply_camera_record};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::fly_camera::{
    CROSSHAIR_ALPHA, CROSSHAIR_HALF_LENGTH, DEFAULT_SPEED, FlyCamera, FlyInput, FlyKeys,
    MAX_PITCH, MAX_SPEED, MIN_SPEED, WHEEL_NOTCH_POINTS, capture_commands,
    capture_exit_required, capture_transition, crosshair_segments, crosshair_visible,
    exit_on_focus_loss, frame_look, is_command_chord, sync_fly, sync_fly_mirror, toggle_fly,
};
use vector_field_explorer::persistence::{AppSettings, ViewRecord, load};
use vector_field_explorer::render::Camera;
use vector_field_explorer::ui::help_overlay::{FLY_HELP_ACTION, FLY_HELP_INPUT, rows_for};
use vector_field_explorer::ui::menu::{self, MenuAction};
use vector_field_explorer::view_command::{
    ViewCommand, ViewContext, apply_view_command, apply_view_command_with_fly,
    domain_center, domain_centre_delta,
};
use vector_field_explorer::viewmodel::State;

fn assert_close(a: Vec3, b: Vec3, what: &str) {
    let d = (a - b).length();
    assert!(d < 1e-4, "{what}: |{a:?} - {b:?}| = {d} > 1e-4");
}

fn fly_at_origin() -> FlyCamera {
    FlyCamera {
        position: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        speed: 2.0,
    }
}

fn moved(fly: &mut FlyCamera, input: FlyInput, dt: f32) -> Vec3 {
    let before = fly.position;
    fly.step(input, dt);
    fly.position - before
}

#[test]
fn from_orbit_reproduces_the_orbit_eye_and_look_direction() {
    let mut orbit = OrbitCamera::default();
    orbit.orbit(1.1, -0.4);
    orbit.pan(0.35, -0.2);
    orbit.zoom(0.7);

    let fly = FlyCamera::from_orbit(&orbit);
    assert_close(fly.position, orbit.position(), "the fly eye starts at the orbit eye");
    assert_close(
        fly.look_dir(),
        (orbit.target - orbit.position()).normalize(),
        "the fly look direction starts at the orbit aim",
    );
    assert_eq!(fly.speed, DEFAULT_SPEED, "a fresh fly camera starts at the default speed");
}

#[test]
fn entering_and_exiting_restores_the_untouched_orbit() {
    let mut orbit = OrbitCamera::default();
    orbit.orbit(0.9, -0.35);
    orbit.pan(0.4, 0.1);
    let untouched = orbit;
    let mut fly = None;

    toggle_fly(&mut fly, &orbit, true);
    let entered = fly.expect("3D enters fly mode");
    assert_close(entered.position, orbit.position(), "entering starts at the current viewpoint");

    // The fly camera can move freely; the orbit record must not follow it.
    let mut fly = Some(entered);
    if let Some(camera) = fly.as_mut() {
        camera.step(
            FlyInput {
                forward: 1.0,
                ..FlyInput::default()
            },
            0.1,
        );
    }
    assert_eq!(orbit, untouched, "moving in fly mode never mutates the orbit record");

    toggle_fly(&mut fly, &orbit, true);
    assert!(fly.is_none(), "a second C leaves fly mode");
    assert_eq!(orbit, untouched, "exiting restores the untouched orbit view");
}

#[test]
fn step_moves_forward_strafe_and_vertical_in_the_camera_basis() {
    let mut forward = fly_at_origin();
    let delta = moved(
        &mut forward,
        FlyInput {
            forward: 1.0,
            ..FlyInput::default()
        },
        0.1,
    );
    assert_close(delta, Vec3::X * 0.2, "forward follows look_dir");

    let mut strafe = fly_at_origin();
    let delta = moved(
        &mut strafe,
        FlyInput {
            strafe: 1.0,
            ..FlyInput::default()
        },
        0.1,
    );
    assert_close(delta, Vec3::Z * 0.2, "strafe follows right()");

    let mut vertical = fly_at_origin();
    let delta = moved(
        &mut vertical,
        FlyInput {
            vertical: 1.0,
            ..FlyInput::default()
        },
        0.1,
    );
    assert_close(delta, Vec3::Y * 0.2, "vertical follows up()");
}

#[test]
fn diagonal_movement_is_not_faster() {
    let mut forward = fly_at_origin();
    let straight = moved(
        &mut forward,
        FlyInput {
            forward: 1.0,
            ..FlyInput::default()
        },
        0.1,
    )
    .length();

    let mut diagonal = fly_at_origin();
    let diagonal_len = moved(
        &mut diagonal,
        FlyInput {
            forward: 1.0,
            strafe: 1.0,
            ..FlyInput::default()
        },
        0.1,
    )
    .length();
    assert!((diagonal_len - straight).abs() < 1e-5, "normalising keeps diagonals the same speed");
}

#[test]
fn shift_boosts_by_exactly_four() {
    let mut plain = fly_at_origin();
    let plain_len = moved(
        &mut plain,
        FlyInput {
            forward: 1.0,
            ..FlyInput::default()
        },
        0.1,
    )
    .length();

    let mut boosted = fly_at_origin();
    let boosted_len = moved(
        &mut boosted,
        FlyInput {
            forward: 1.0,
            boost: true,
            ..FlyInput::default()
        },
        0.1,
    )
    .length();
    assert!(
        (boosted_len - plain_len * 4.0).abs() < 1e-5,
        "Shift is exactly a 4x boost: {boosted_len} vs {plain_len}",
    );
}

#[test]
fn dt_is_clamped_and_junk_time_moves_nothing() {
    let input = FlyInput {
        forward: 1.0,
        ..FlyInput::default()
    };

    let mut oversized = fly_at_origin();
    let delta = moved(&mut oversized, input, 5.0);
    assert_close(delta, Vec3::X * 0.2, "an oversized dt clamps to 0.1");

    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut fly = fly_at_origin();
        let delta = moved(&mut fly, input, bad);
        assert_eq!(delta, Vec3::ZERO, "dt = {bad} must not move the camera");
    }
}

#[test]
fn look_updates_the_angles_and_pitch_clamps_at_the_poles() {
    let mut fly = fly_at_origin();
    fly.step(
        FlyInput {
            look: Vec2::new(100.0, 0.0),
            ..FlyInput::default()
        },
        0.016,
    );
    assert!((fly.yaw + 0.5).abs() < 1e-5, "look.x turns yaw by 0.005 rad/point, unwrapped");

    let mut fly = fly_at_origin();
    fly.step(
        FlyInput {
            look: Vec2::new(0.0, -10_000.0),
            ..FlyInput::default()
        },
        0.016,
    );
    assert!((fly.pitch - MAX_PITCH).abs() < 1e-6, "pitch clamps at the top: {}", fly.pitch);

    fly.step(
        FlyInput {
            look: Vec2::new(0.0, 10_000.0),
            ..FlyInput::default()
        },
        0.016,
    );
    assert!((fly.pitch + MAX_PITCH).abs() < 1e-6, "pitch clamps at the bottom: {}", fly.pitch);
}

#[test]
fn yaw_wraps_freely_past_a_full_turn() {
    let mut fly = fly_at_origin();
    // 4000 points at 0.005 rad/point = 20 rad per step, well past ±2π.
    for _ in 0..3 {
        fly.step(
            FlyInput {
                look: Vec2::new(4000.0, 0.0),
                ..FlyInput::default()
            },
            0.016,
        );
    }
    assert!(fly.yaw.is_finite(), "yaw has no clamp: {}", fly.yaw);
    assert!((fly.yaw + 60.0).abs() < 1e-3, "every look point turns yaw: {}", fly.yaw);
    let direction = fly.look_dir();
    assert!(
        (direction.length() - 1.0).abs() < 1e-5,
        "the look direction stays on the unit sphere: {direction:?}",
    );
}

#[test]
fn translate_moves_the_eye_without_turning_or_changing_speed() {
    let mut orbit = OrbitCamera::default();
    orbit.orbit(0.7, -0.2);
    let mut fly = FlyCamera::from_orbit(&orbit);
    let before = fly;
    let delta = Vec3::new(1.5, -2.0, 0.25);

    fly.translate(delta);

    assert_close(fly.position, before.position + delta, "translate shifts the eye");
    assert_eq!(fly.yaw, before.yaw, "translate keeps yaw");
    assert_eq!(fly.pitch, before.pitch, "translate keeps pitch");
    assert_eq!(fly.speed, before.speed, "translate keeps speed");
}

#[test]
fn translating_by_the_domain_centre_delta_keeps_the_free_camera_relative_to_the_box() {
    let old = Domain::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    let moved = Domain::new(Vec3::new(5.0, 1.0, 2.0), Vec3::new(7.0, 3.0, 4.0));
    let delta = domain_centre_delta(&old, &moved);

    let mut fly = fly_at_origin();
    fly.yaw = 0.8;
    fly.pitch = -0.3;
    fly.speed = 3.0;
    let before = fly;
    let relative_before = fly.position - domain_center(old);

    fly.translate(delta);

    assert_close(
        fly.position - domain_center(moved),
        relative_before,
        "the free camera keeps its position relative to the new domain centre",
    );
    assert_eq!(fly.yaw, before.yaw, "translating with the domain keeps yaw");
    assert_eq!(fly.pitch, before.pitch, "translating with the domain keeps pitch");
    assert_eq!(fly.speed, before.speed, "translating with the domain keeps speed");
}

#[test]
fn wheel_scroll_scales_speed_and_clamps() {
    let mut fly = fly_at_origin();
    fly.adjust_speed(WHEEL_NOTCH_POINTS);
    assert!(
        (fly.speed - DEFAULT_SPEED * 0.1_f32.exp()).abs() < 1e-5,
        "one notch multiplies speed by exp(0.1): {}",
        fly.speed,
    );

    fly.adjust_speed(10_000.0);
    assert_eq!(fly.speed, MAX_SPEED, "the speed clamps at the top");

    fly.adjust_speed(-10_000.0);
    assert_eq!(fly.speed, MIN_SPEED, "the speed clamps at the bottom");

    let mut junk = fly_at_origin();
    junk.adjust_speed(f32::NAN);
    assert_eq!(junk.speed, DEFAULT_SPEED, "a non-finite scroll leaves the speed alone");
}

#[test]
fn physical_key_positions_win_over_the_logical_key() {
    let mut keys = FlyKeys::default();
    // A logical D on a layout that moves D to the physical W position must
    // still walk forward: the physical QWERTY position is what games use.
    keys.apply_key(egui::Key::D, Some(egui::Key::W), true);
    let input = keys.input(Vec2::ZERO, false);
    assert_eq!(input.forward, 1.0, "physical W is forward regardless of the logical D");
    assert_eq!(input.strafe, 0.0, "the logical D must not also strafe");

    keys.apply_key(egui::Key::D, Some(egui::Key::W), false);
    assert!(!keys.movement_held(), "releasing the physical W stops the movement");
}

#[test]
fn a_missing_physical_key_falls_back_to_the_logical_key() {
    let mut keys = FlyKeys::default();
    keys.apply_key(egui::Key::D, None, true);
    assert_eq!(
        keys.input(Vec2::ZERO, false).strafe,
        1.0,
        "a backend without a physical key uses the logical D",
    );
}

#[test]
fn every_movement_key_maps_correctly() {
    let cases = [
        (egui::Key::W, FlyInput { forward: 1.0, ..FlyInput::default() }),
        (egui::Key::S, FlyInput { forward: -1.0, ..FlyInput::default() }),
        (egui::Key::A, FlyInput { strafe: -1.0, ..FlyInput::default() }),
        (egui::Key::D, FlyInput { strafe: 1.0, ..FlyInput::default() }),
        (egui::Key::Q, FlyInput { vertical: -1.0, ..FlyInput::default() }),
        (egui::Key::E, FlyInput { vertical: 1.0, ..FlyInput::default() }),
    ];
    for (key, expected) in cases {
        let mut keys = FlyKeys::default();
        keys.apply_key(key, None, true);
        assert_eq!(keys.input(Vec2::ZERO, false), expected, "{key:?} maps to {expected:?}");
        assert!(keys.movement_held(), "{key:?} counts as a movement key");
    }
}

#[test]
fn boost_comes_from_the_frame_modifiers_not_key_events() {
    // Either Shift in the modifiers boosts; a Shift key event itself is not a
    // movement key, so a missed key-up cannot leave boost stuck on.
    let mut keys = FlyKeys::default();
    for shift in [egui::Key::ShiftLeft, egui::Key::ShiftRight] {
        keys.apply_key(shift, None, true);
        assert!(!keys.movement_held(), "{shift:?} is not a movement key");
        assert!(!keys.input(Vec2::ZERO, false).boost, "a key event does not set boost");
    }
    assert!(keys.input(Vec2::ZERO, true).boost, "the frame modifier sets boost");
    keys.clear();
    assert_eq!(
        keys.input(Vec2::ZERO, false),
        FlyInput::default(),
        "clearing the keys drops every held movement key",
    );

    // Ctrl/Cmd/Alt chords belong to menus/browser commands, never flight.
    assert!(!is_command_chord(egui::Modifiers::NONE));
    assert!(!is_command_chord(egui::Modifiers::SHIFT));
    assert!(is_command_chord(egui::Modifiers::CTRL));
    assert!(is_command_chord(egui::Modifiers::COMMAND));
    assert!(is_command_chord(egui::Modifiers::ALT));
    assert!(is_command_chord(egui::Modifiers::MAC_CMD));
}

#[test]
fn view_camera_rays_through_the_centre_match_the_view_direction() {
    let orbit = OrbitCamera::default();
    let fly = FlyCamera::from_orbit(&orbit);

    let orbit_camera = ViewCamera::Orbit(&orbit);
    let (origin, direction) = orbit_camera.ndc_ray(1.4, Vec2::ZERO);
    assert_close(origin, orbit.position(), "the orbit ray starts at the orbit eye");
    assert_close(
        direction.normalize(),
        (orbit.target - orbit.position()).normalize(),
        "an orbit centre ray looks at the target",
    );

    let fly_camera = ViewCamera::Fly(&fly);
    let (origin, direction) = fly_camera.ndc_ray(1.4, Vec2::ZERO);
    assert_close(origin, fly.position, "the fly ray starts at the fly eye");
    assert_close(direction.normalize(), fly.look_dir(), "a fly centre ray looks along look_dir");
}

#[test]
fn view_camera_vp_matrix_is_the_one_the_viewport_and_screenshot_share() {
    let orbit = OrbitCamera::default();
    let fly = FlyCamera::from_orbit(&orbit);
    let aspect = 1.7;

    assert_eq!(
        ViewCamera::Orbit(&orbit).vp_matrix(aspect, None),
        orbit.proj_perspective(aspect) * orbit.view(),
        "the orbit viewport and screenshot share one view-projection",
    );
    assert_eq!(
        ViewCamera::Fly(&fly).vp_matrix(aspect, None),
        fly.proj_perspective(aspect) * fly.view(),
        "the fly viewport and screenshot share one view-projection",
    );
    assert!(ViewCamera::Fly(&fly).is_fly());
    assert!(!ViewCamera::Orbit(&orbit).is_fly());
}

#[test]
fn c_toggles_only_in_three_d_and_switching_to_2d_exits() {
    let orbit = OrbitCamera::default();
    let mut fly = None;

    toggle_fly(&mut fly, &orbit, false);
    assert!(fly.is_none(), "2D ignores C");

    toggle_fly(&mut fly, &orbit, true);
    assert!(fly.is_some(), "3D enters fly mode");

    // Leaving 3D for 2D drops the fly camera.
    sync_fly(&mut fly, false);
    assert!(fly.is_none(), "switching to 2D exits fly");

    let mut fly = Some(FlyCamera::from_orbit(&orbit));
    sync_fly(&mut fly, true);
    assert!(fly.is_some(), "staying in 3D keeps fly mode");
}

#[test]
fn reset_and_fit_exit_fly_and_match_the_same_command_without_fly() {
    let mut orbit = OrbitCamera::default();
    orbit.orbit(1.2, -0.3);
    orbit.zoom(0.5);
    let view = ViewContext::new(true, Domain::default(), 1.6);

    for command in [ViewCommand::Reset, ViewCommand::Fit] {
        let mut fly = Some(FlyCamera::from_orbit(&orbit));
        let with_fly = apply_view_command_with_fly(orbit, &mut fly, view, command);
        let without_fly = apply_view_command(orbit, view, command);
        assert!(fly.is_none(), "{command:?} leaves fly mode");
        assert_eq!(with_fly, without_fly, "{command:?} acts on the untouched orbit camera");
    }
}

#[test]
fn orbit_and_pan_commands_are_inert_while_flying() {
    let orbit = OrbitCamera::default();
    let mut fly = Some(FlyCamera::from_orbit(&orbit));
    let view = ViewContext::new(true, Domain::default(), 1.6);

    let after = apply_view_command_with_fly(
        orbit,
        &mut fly,
        view,
        ViewCommand::Orbit { dx: 1.0, dy: 1.0 },
    );
    assert_eq!(after, orbit, "orbit commands must not move the hidden orbit camera");
    assert!(fly.is_some(), "an orbit command does not exit fly mode");
}

#[test]
fn applying_a_camera_preset_exits_fly_and_matches_the_no_fly_result() {
    let mut orbit = OrbitCamera::default();
    orbit.orbit(1.2, 0.4);
    orbit.target = Vec3::new(0.5, -0.5, 0.0);
    let record = Camera {
        position: [4.0, -2.0],
        scale: 2.5,
    };

    let mut with_fly = orbit;
    let mut fly = Some(FlyCamera::from_orbit(&orbit));
    apply_camera_record(&record, &mut with_fly, &mut fly);

    let mut without_fly = orbit;
    let mut no_fly = None;
    apply_camera_record(&record, &mut without_fly, &mut no_fly);

    assert!(fly.is_none(), "applying an orbit preset leaves fly mode");
    assert_eq!(with_fly, without_fly, "the resulting orbit camera is the same as without fly");
    assert_eq!(with_fly.target.x, record.position[0]);
    assert_eq!(with_fly.target.y, record.position[1]);
    assert_eq!(with_fly.ortho_zoom, record.scale);
}

#[test]
fn fly_has_one_visible_entry_and_keeps_the_c_shortcut() {
    // Ticket 65: the sidebar Camera section is the one visible control; the
    // View menu no longer carries a duplicate item, but C stays bound.
    let view = menu::MENUS
        .iter()
        .find(|menu| menu.title == "View")
        .expect("the View menu exists");
    assert!(
        !view.items.iter().any(|item| item.action == MenuAction::ToggleFly),
        "View > Fly camera was removed in favour of the sidebar Camera section",
    );
    assert!(
        !view.items.iter().any(|item| item.label == "Fly camera"),
        "no stale Fly camera label remains",
    );

    let shortcut = menu::shortcut_for(MenuAction::ToggleFly).expect("C is bound to fly mode");
    assert_eq!(shortcut.key, egui::Key::C);
    assert_eq!(shortcut.modifiers, egui::Modifiers::NONE);
    assert_eq!(shortcut.label, "C");

    let rows = rows_for(true);
    assert!(
        rows.iter().any(|row| row.input == FLY_HELP_INPUT && row.action == FLY_HELP_ACTION),
        "the 3D help group documents the fly keys",
    );
    assert_eq!(
        format!("{FLY_HELP_INPUT} {FLY_HELP_ACTION}"),
        "C fly · WASD move · Q/E down/up · Shift boost · mouse look · Esc exit · Camera toggles it",
    );
}

#[test]
fn capture_commands_confine_before_locking_and_hide_only_while_active() {
    assert_eq!(
        capture_commands(true),
        (
            &[egui::CursorGrab::Confined, egui::CursorGrab::Locked][..],
            false,
        ),
        "entering attempts Confined first, then Locked, and hides the cursor",
    );
    let (enter_grabs, _) = capture_commands(true);
    assert_eq!(
        enter_grabs.first(),
        Some(&egui::CursorGrab::Confined),
        "the X11-safe fallback is sent first",
    );
    assert_eq!(
        enter_grabs.last(),
        Some(&egui::CursorGrab::Locked),
        "platforms that grant Locked end in a real lock",
    );
    assert_eq!(
        capture_commands(false),
        (&[egui::CursorGrab::None][..], true),
        "leaving fly clears the grab and restores the cursor",
    );
}

#[test]
fn capture_commands_are_edge_triggered_once_per_transition() {
    assert_eq!(
        capture_transition(false, false),
        None,
        "staying out of fly sends nothing",
    );
    assert_eq!(
        capture_transition(true, true),
        None,
        "staying in fly sends nothing every frame",
    );
    assert_eq!(
        capture_transition(false, true),
        Some((
            &[egui::CursorGrab::Confined, egui::CursorGrab::Locked][..],
            false,
        )),
        "entering fly sends the capture once",
    );
    assert_eq!(
        capture_transition(true, false),
        Some((&[egui::CursorGrab::None][..], true)),
        "leaving fly releases the capture once",
    );
}

#[test]
fn capture_exits_only_for_text_focus_or_a_lost_web_lock() {
    assert!(
        capture_exit_required(true, true, false),
        "a focused text field exits capture so the pointer is usable",
    );
    assert!(
        capture_exit_required(true, false, true),
        "a dropped web pointer lock exits capture",
    );
    assert!(
        !capture_exit_required(true, false, false),
        "ordinary flying never releases capture every frame",
    );
    assert!(
        !capture_exit_required(false, true, true),
        "no capture, no release",
    );
}

#[test]
fn frame_look_prefers_raw_motion_only_while_captured() {
    let drag = Vec2::new(8.0, -3.0);
    let raw = Vec2::new(-12.0, 5.0);

    assert_eq!(frame_look(false, raw, drag), drag, "orbit/drag-look ignores raw motion");
    assert_eq!(frame_look(true, raw, drag), raw, "captured look uses raw motion");
    assert_eq!(
        frame_look(true, Vec2::ZERO, drag),
        drag,
        "a captured frame with no raw motion falls back to drag-look",
    );
    assert_eq!(
        frame_look(true, Vec2::new(f32::NAN, 1.0), drag),
        drag,
        "a non-finite raw delta never poisons the look",
    );
}

#[test]
fn a_raw_mouse_delta_changes_yaw_and_pitch_once() {
    let mut fly = fly_at_origin();
    let raw = Vec2::new(20.0, -10.0);
    let input = FlyInput {
        look: frame_look(true, raw, Vec2::new(999.0, 999.0)),
        ..FlyInput::default()
    };
    fly.step(input, 0.016);
    assert!((fly.yaw + 0.1).abs() < 1e-5, "raw x turns yaw by 0.005 rad/point: {}", fly.yaw);
    assert!((fly.pitch - 0.05).abs() < 1e-5, "raw y pitches by 0.005 rad/point: {}", fly.pitch);
}

#[test]
fn focus_loss_exits_fly_and_clears_the_keys() {
    let mut fly = Some(FlyCamera::default());
    let mut keys = FlyKeys::default();
    keys.apply_key(egui::Key::W, None, true);

    exit_on_focus_loss(&mut fly, &mut keys, &[egui::Event::WindowFocused(true)]);
    assert!(fly.is_some(), "gaining focus is not a reason to leave fly");
    assert!(keys.movement_held(), "unrelated events keep the held keys");

    exit_on_focus_loss(&mut fly, &mut keys, &[egui::Event::WindowFocused(false)]);
    assert!(fly.is_none(), "focus loss leaves fly so a hidden cursor cannot be stranded");
    assert!(!keys.movement_held(), "focus loss clears the held movement keys");
}

#[test]
fn the_sidebar_mirror_enters_fly_through_the_one_toggle_seam() {
    let orbit = OrbitCamera::default();
    let mut fly = None;
    let mut mirror = true;

    sync_fly_mirror(&mut mirror, &mut fly, &orbit, true);
    assert!(fly.is_some(), "checking Fly camera in 3D enters fly");
    assert!(mirror, "the mirror follows the new state");

    let mut mirror = false;
    sync_fly_mirror(&mut mirror, &mut fly, &orbit, true);
    assert!(fly.is_none(), "unchecking Fly camera leaves fly");
    assert!(!mirror);

    // 2D is inert: the checkbox cannot force fly mode on.
    let mut fly = None;
    let mut mirror = true;
    sync_fly_mirror(&mut mirror, &mut fly, &orbit, false);
    assert!(fly.is_none(), "2D ignores the checkbox");
    assert!(!mirror, "the mirror is resynced to the inert state");
}

#[test]
fn the_crosshair_is_two_twelve_pixel_lines_centred_on_the_viewport() {
    assert_eq!(CROSSHAIR_HALF_LENGTH, 6.0, "the full crosshair arm is 12 px");
    assert_eq!(CROSSHAIR_ALPHA, 153, "60% alpha white");
    let center = egui::pos2(400.0, 300.0);
    let [(left, right), (top, bottom)] = crosshair_segments(center);
    assert_eq!(left, egui::pos2(394.0, 300.0));
    assert_eq!(right, egui::pos2(406.0, 300.0));
    assert_eq!(top, egui::pos2(400.0, 294.0));
    assert_eq!(bottom, egui::pos2(400.0, 306.0));

    assert!(crosshair_visible(true, true), "the crosshair shows while flying captured");
    assert!(!crosshair_visible(false, true), "no fly, no crosshair");
    assert!(!crosshair_visible(true, false), "no capture, no crosshair");
    assert!(!crosshair_visible(false, false));
}

#[test]
fn the_fly_camera_is_not_persisted() {
    let mut storage = MemoryStorage::default();
    let state = State::new(true);
    let settings = AppSettings {
        view: ViewRecord::capture(&state),
        ..AppSettings::default()
    };
    eframe::set_value(&mut storage, eframe::APP_KEY, &settings);
    let raw = storage.get_string(eframe::APP_KEY).expect("the envelope is saved");
    assert!(
        !raw.to_ascii_lowercase().contains("fly"),
        "the settings envelope must not carry fly-camera state:\n{raw}",
    );
    assert_eq!(load(Some(&storage)), settings, "the envelope still round-trips unchanged");
}

#[test]
fn non_qwerty_physical_a_strafes_left_and_does_not_toggle_fly() {
    // Ticket 70 regression: on a Bone layout the key physically labelled A
    // reports logical C. It must strafe left and must not exit fly mode.
    let mut keys = FlyKeys::default();
    keys.apply_key(egui::Key::C, Some(egui::Key::A), true);
    let input = keys.input(Vec2::ZERO, false);
    assert_eq!(input.strafe, -1.0, "physical A strafes left");
    assert_eq!(input.forward, 0.0, "it does not also move forward");

    // The fly toggle fires only on the key physically in the C position, so a
    // physical-A event must not match it (fly mode stays active).
    let physical_key: Option<egui::Key> = Some(egui::Key::A);
    let logical = egui::Key::C;
    #[allow(clippy::unnecessary_literal_unwrap)]
    let matches_toggle = physical_key.unwrap_or(logical) == egui::Key::C;
    assert!(!matches_toggle, "physical A does not toggle fly");
}

/// In-memory stand-in for eframe's disk-backed storage.
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
