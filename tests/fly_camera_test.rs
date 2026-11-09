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
    DEFAULT_SPEED, FlyCamera, FlyInput, FlyKeys, MAX_PITCH, MAX_SPEED, MIN_SPEED,
    WHEEL_NOTCH_POINTS, is_command_chord, sync_fly, toggle_fly,
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
fn the_view_menu_and_help_expose_fly_mode() {
    let view = menu::MENUS
        .iter()
        .find(|menu| menu.title == "View")
        .expect("the View menu exists");
    let item = view
        .items
        .iter()
        .find(|item| item.label == "Fly camera")
        .expect("View > Fly camera exists");
    assert_eq!(item.action, MenuAction::ToggleFly);
    assert!(!menu::item_enabled(item, false), "the item is disabled in 2D");
    assert!(menu::item_enabled(item, true), "the item is enabled in 3D");
    assert!(!menu::item_checked(item, false), "no checkmark while orbit is active");
    assert!(menu::item_checked(item, true), "the checkmark follows the active fly camera");

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
        "C fly · WASD move · Q/E down/up · Shift boost · drag look · Esc exit",
    );
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
