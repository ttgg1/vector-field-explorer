//! Six-DOF fly camera for the 3D view (ticket 46).
//!
//! The fly camera is a free-flight pose: a world position plus yaw/pitch, with
//! WASD/QE movement relative to the look direction, Shift boost and drag look.
//! It is deliberately **not** persisted and has no camera presets; entering fly
//! snapshots the current [`OrbitCamera`] eye, and exiting drops this camera so
//! the untouched orbit view returns. [`ViewCamera`](crate::camera::ViewCamera)
//! is the seam every consumer (viewport, screenshot, click seeding) uses to
//! read either camera without matching on the mode.
//!
//! Movement keys are read from `Event::Key` rather than `InputState::keys_down`,
//! because the latter is logical-key only. The physical QWERTY position comes
//! first, so a Dvorak/AZERTY layout still moves with WASD: egui-winit fills
//! `physical_key` natively, and eframe's web backend fills it from
//! `KeyboardEvent.code()` (`"KeyW"` → `Key::W`). A backend that omits it still
//! works through the logical-key fallback.

use eframe::egui;
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::camera::OrbitCamera;

/// Default world-units-per-second flight speed.
pub const DEFAULT_SPEED: f32 = 2.0;
/// Slowest wheel speed.
pub const MIN_SPEED: f32 = 0.5;
/// Fastest wheel speed.
pub const MAX_SPEED: f32 = 20.0;
/// The pitch limit, kept a margin away from the poles so the view basis stays
/// well-defined.
pub const MAX_PITCH: f32 = 1.55;
/// Radians of look per point of left-drag.
pub const LOOK_SENSITIVITY: f32 = 0.005;
/// Shift multiplies the step distance by exactly this.
pub const BOOST_MULTIPLIER: f32 = 4.0;
/// The largest frame time a single step honours, so a stall cannot teleport
/// the camera across the domain.
pub const MAX_DT: f32 = 0.1;
/// Perspective field of view, matching the orbit camera.
pub const FOV_Y: f32 = 55.0_f32.to_radians();
/// Perspective near plane.
pub const NEAR: f32 = 0.01;
/// Perspective far plane.
pub const FAR: f32 = 100.0;
/// `speed *= exp(WHEEL_RATE * notches)` per wheel notch.
pub const WHEEL_RATE: f32 = 0.1;
/// One conventional wheel notch in egui's scroll points. egui reports wheel
/// deltas in points (~50 per notch), so [`FlyCamera::adjust_speed`] divides the
/// incoming points by this before applying the per-notch [`WHEEL_RATE`]. This
/// is a notch normalization only: `top_bar::WHEEL_ZOOM_RATE` is a per-point
/// rate and is not the same unit.
pub const WHEEL_NOTCH_POINTS: f32 = 50.0;

/// A free-flight camera pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlyCamera {
    pub position: Vec3,
    /// Radians, `atan2(dir.z, dir.x)`.
    pub yaw: f32,
    /// Radians, `asin(dir.y)`, clamped to [`MAX_PITCH`] either way.
    pub pitch: f32,
    /// World units per second, clamped to [`MIN_SPEED`]..=[`MAX_SPEED`].
    pub speed: f32,
}

impl Default for FlyCamera {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            speed: DEFAULT_SPEED,
        }
    }
}

impl FlyCamera {
    /// Start exactly where the orbit camera is looking, keeping its direction.
    pub fn from_orbit(orbit: &OrbitCamera) -> Self {
        let position = orbit.position();
        let direction = (orbit.target - position).normalize_or_zero();
        let direction = if direction.length_squared() > 0.0 {
            direction
        } else {
            Vec3::NEG_Z
        };
        Self {
            position,
            yaw: direction.z.atan2(direction.x),
            // `normalize_or_zero` can still hand back `y` a hair outside
            // [-1, 1] from f32 rounding, which would make `asin` NaN.
            pitch: direction.y.clamp(-1.0, 1.0).asin().clamp(-MAX_PITCH, MAX_PITCH),
            speed: DEFAULT_SPEED,
        }
    }

    /// Unit view direction from yaw/pitch.
    pub fn look_dir(&self) -> Vec3 {
        Vec3::new(
            self.pitch.cos() * self.yaw.cos(),
            self.pitch.sin(),
            self.pitch.cos() * self.yaw.sin(),
        )
    }

    /// Unit right vector (`cross(look_dir, Y)`), the strafe basis.
    pub fn right(&self) -> Vec3 {
        self.look_dir().cross(Vec3::Y).normalize_or_zero()
    }

    /// Unit up vector (`cross(right, look_dir)`), the vertical basis.
    pub fn up(&self) -> Vec3 {
        self.right().cross(self.look_dir())
    }

    /// Move and look for one frame. `dt` is clamped to `0..=MAX_DT`; a
    /// non-finite or non-positive `dt` moves nothing (looking still applies,
    /// because it is not tied to the clock).
    pub fn step(&mut self, input: FlyInput, dt: f32) {
        let look = Vec2::new(finite_or_zero(input.look.x), finite_or_zero(input.look.y));
        self.yaw -= look.x * LOOK_SENSITIVITY;
        self.pitch = (self.pitch - look.y * LOOK_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);

        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(MAX_DT);

        let forward = finite_or_zero(input.forward).clamp(-1.0, 1.0);
        let strafe = finite_or_zero(input.strafe).clamp(-1.0, 1.0);
        let vertical = finite_or_zero(input.vertical).clamp(-1.0, 1.0);

        let direction =
            self.look_dir() * forward + self.right() * strafe + self.up() * vertical;
        let length = direction.length();
        if length <= 1e-6 {
            return;
        }
        let direction = direction / length;
        let speed = if self.speed.is_finite() {
            self.speed.clamp(MIN_SPEED, MAX_SPEED)
        } else {
            DEFAULT_SPEED
        };
        let boost = if input.boost { BOOST_MULTIPLIER } else { 1.0 };
        self.position += direction * speed * dt * boost;
    }

    /// Scale the speed by `exp(WHEEL_RATE * notches)`, clamped to the usable
    /// range. `scroll_points` is egui's wheel delta in points; one conventional
    /// notch is [`WHEEL_NOTCH_POINTS`]. Non-finite scroll leaves the speed alone.
    pub fn adjust_speed(&mut self, scroll_points: f32) {
        if !scroll_points.is_finite() {
            return;
        }
        let factor = (WHEEL_RATE * scroll_points / WHEEL_NOTCH_POINTS).exp();
        if factor.is_finite() {
            let speed = if self.speed.is_finite() { self.speed } else { DEFAULT_SPEED };
            self.speed = (speed * factor).clamp(MIN_SPEED, MAX_SPEED);
        }
    }

    /// World-to-eye transform for this pose.
    pub fn view(&self) -> Mat4 {
        let direction = self.look_dir();
        // A pose pitched to the pole has no usable basis; look straight up
        // from an axis that keeps `look_at_rh` valid.
        let up = if direction.cross(Vec3::Y).length() < 1e-4 {
            Vec3::Z
        } else {
            self.up()
        };
        Mat4::look_at_rh(self.position, self.position + direction, up)
    }

    /// Perspective projection, matching the orbit camera's field of view and
    /// near/far planes.
    pub fn proj_perspective(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(FOV_Y, aspect, NEAR, FAR)
    }

    /// Full view-projection for the viewport and screenshot capture.
    pub fn vp_matrix(&self, aspect: f32) -> Mat4 {
        self.proj_perspective(aspect) * self.view()
    }

    /// Ray through `ndc_xy` (in `[-1,1]^2`); direction intentionally
    /// unnormalized, matching [`OrbitCamera::ndc_ray`]'s contract.
    pub fn ndc_ray(&self, aspect: f32, ndc_xy: Vec2) -> (Vec3, Vec3) {
        let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        let half_h = (FOV_Y * 0.5).tan();
        let camera_dir = Vec3::new(ndc_xy.x * half_h * aspect, ndc_xy.y * half_h, -1.0) * FAR;
        let direction = (self.view().inverse()
            * Vec4::new(camera_dir.x, camera_dir.y, camera_dir.z, 0.0))
        .truncate();
        (self.position, direction)
    }

    /// Shift the eye by `delta` (ticket 47's domain recentre).
    pub fn translate(&mut self, delta: Vec3) {
        self.position += delta;
    }
}

/// One frame of fly input: normalized movement axes, boost and look delta in
/// points. The app fills this from [`FlyKeys`] and the viewport drag.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FlyInput {
    /// Forward (+) / backward (-), `-1..=1`.
    pub forward: f32,
    /// Right (+) / left (-), `-1..=1`.
    pub strafe: f32,
    /// Up (+) / down (-), `-1..=1`.
    pub vertical: f32,
    /// Shift is held.
    pub boost: bool,
    /// Look delta in points; `x` turns yaw, `y` pitches.
    pub look: Vec2,
}

/// The set of movement keys currently held. Accumulated from `Event::Key`
/// because `InputState::keys_down` is logical-key only. Boost is deliberately
/// not in this set: it comes from `InputState::modifiers.shift` once per frame,
/// so a missed Shift key-up cannot leave the camera stuck in boost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlyKeys {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub down: bool,
    pub up: bool,
}

impl FlyKeys {
    /// Forget every held key (window focus lost, Esc, a command chord, leaving
    /// fly mode).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Whether a movement key is held, i.e. whether the app must keep
    /// repainting so movement continues without new input.
    pub fn movement_held(&self) -> bool {
        self.forward
            || self.backward
            || self.left
            || self.right
            || self.down
            || self.up
    }

    /// Apply one key event. `physical_key` is the QWERTY position when the
    /// backend provides it (native, and web via `KeyboardEvent.code()`);
    /// otherwise `logical` is used. Shift is not handled here: boost comes from
    /// the frame's modifiers (see [`is_command_chord`]).
    pub fn apply_key(
        &mut self,
        logical: egui::Key,
        physical_key: Option<egui::Key>,
        pressed: bool,
    ) {
        match physical_key.unwrap_or(logical) {
            egui::Key::W => self.forward = pressed,
            egui::Key::S => self.backward = pressed,
            egui::Key::A => self.left = pressed,
            egui::Key::D => self.right = pressed,
            egui::Key::Q => self.down = pressed,
            egui::Key::E => self.up = pressed,
            _ => {}
        }
    }

    /// Fold the held keys, the frame's `boost` state and this frame's look
    /// delta into [`FlyInput`].
    pub fn input(&self, look: Vec2, boost: bool) -> FlyInput {
        FlyInput {
            forward: axis(self.forward, self.backward),
            strafe: axis(self.right, self.left),
            vertical: axis(self.up, self.down),
            boost,
            look,
        }
    }
}

/// Whether the current modifiers belong to a menu/browser command chord
/// (Ctrl/Cmd/Alt). While one is held the app clears [`FlyKeys`] and feeds no
/// key events, so `Ctrl+S`, `Cmd+W` and friends never fly the camera.
pub fn is_command_chord(modifiers: egui::Modifiers) -> bool {
    modifiers.ctrl || modifiers.command || modifiers.alt || modifiers.mac_cmd
}

/// `+1` when `positive` is held, `-1` when `negative` is, else `0`.
fn axis(positive: bool, negative: bool) -> f32 {
    match (positive, negative) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    }
}

/// Enter/leave fly mode. Only 3D may fly; in 2D the key is ignored. Entering
/// snapshots the current orbit eye and aim; leaving drops the fly camera and
/// leaves the orbit record exactly as it was.
pub fn toggle_fly(fly: &mut Option<FlyCamera>, orbit: &OrbitCamera, is_3d: bool) {
    if !is_3d {
        return;
    }
    *fly = match fly.take() {
        Some(_) => None,
        None => Some(FlyCamera::from_orbit(orbit)),
    };
}

/// Keep fly mode consistent with the projection: leaving 3D drops the fly
/// camera, returning to the untouched orbit view.
pub fn sync_fly(fly: &mut Option<FlyCamera>, is_3d: bool) {
    if !is_3d {
        *fly = None;
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}
