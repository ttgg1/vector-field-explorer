//! Orbit camera with perspective and orthographic projection plus NDC ray
//! unprojection (Task 6 of the Vector Field Explorer plan).

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::fly_camera::FlyCamera;
use crate::render::Camera;

/// Spherical orbit camera around a target point.
///
/// The same camera drives both views: the perspective orbit in 3D mode and a
/// fixed front view (looking down -Z, Y up) in 2D mode. `ortho_zoom` is the 2D
/// view's zoom factor (1.0 = fit the domain); the 3D zoom lives in `distance`,
/// so the two modes zoom independently without fighting over one field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub fov_y: f32,
    /// Multiplicative 2D zoom: `proj_ortho` divides the requested plane size by
    /// it, so 2.0 shows half as much world. Clamped to
    /// [`OrbitCamera::MIN_ORTHO_ZOOM`]..=[`OrbitCamera::MAX_ORTHO_ZOOM`].
    pub ortho_zoom: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.8,
            pitch: 0.9,
            distance: 3.2,
            target: Vec3::ZERO,
            fov_y: 55.0_f32.to_radians(),
            ortho_zoom: 1.0,
        }
    }
}

impl OrbitCamera {
    /// Closest the orbit eye may come to its target, world units.
    pub const MIN_DISTANCE: f32 = 0.5;
    /// Farthest the orbit eye may move from its target, world units.
    pub const MAX_DISTANCE: f32 = 20.0;
    const NEAR: f32 = 0.01;
    const FAR: f32 = 100.0;
    /// Tightest 2D zoom-in (a 20x magnification).
    pub const MIN_ORTHO_ZOOM: f32 = 0.05;
    /// Widest 2D zoom-out (50x the domain).
    pub const MAX_ORTHO_ZOOM: f32 = 50.0;

    /// Eye position derived from the spherical parameters. The three terms are
    /// the unit sphere direction, so `|position() - target| == distance` for
    /// every yaw/pitch (review I-2: the z term is `sin(yaw) * sin(pitch)`).
    pub fn position(&self) -> Vec3 {
        self.target
            + self.distance
                * Vec3::new(
                    self.yaw.cos() * self.pitch.sin(),
                    self.pitch.cos(),
                    self.yaw.sin() * self.pitch.sin(),
                )
    }

    /// Rotate the camera around the target (radians).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx;
        // keep a margin from the poles so look_at's up vector stays valid
        self.pitch = (self.pitch - dy).clamp(-1.55, 1.55);
    }

    /// Scale the orbit distance; clamped to 0.5..=20.0.
    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(Self::MIN_DISTANCE, Self::MAX_DISTANCE);
    }

    /// Multiply the 2D zoom, clamped so the view can never invert or vanish. A
    /// non-finite current zoom (corrupt storage, an unguarded preset) resets to
    /// 1.0 first, so one bad value cannot stick forever (review M-3).
    pub fn zoom_2d(&mut self, factor: f32) {
        if !self.ortho_zoom.is_finite() {
            self.ortho_zoom = 1.0;
        }
        if factor.is_finite() && factor > 0.0 {
            self.ortho_zoom = (self.ortho_zoom * factor).clamp(Self::MIN_ORTHO_ZOOM, Self::MAX_ORTHO_ZOOM);
        }
    }

    /// The camera's screen basis in world space: `(right, up)`, both
    /// perpendicular to the view direction. Panning moves the target along
    /// these, so the view slides in the plane the user sees.
    pub fn view_basis(&self) -> (Vec3, Vec3) {
        let forward = (self.target - self.position()).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        (right, up)
    }

    /// Move the orbit target `right`/`up` world units along the view plane.
    /// Panning never changes the orientation or the zoom.
    pub fn pan(&mut self, right: f32, up: f32) {
        let (right_axis, up_axis) = self.view_basis();
        self.target += right_axis * right + up_axis * up;
    }

    /// World-to-eye transform (look-at from the orbit position).
    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.position(), self.target, Vec3::Y)
    }

    /// The eye for a projection mode: the orbit position in 3D, and the fixed
    /// front-view eye (`target + Z * distance`) in 2D. The renderer's
    /// camera-facing line ribbons take this, so they match the projection.
    pub fn eye_position(&self, ortho: bool) -> Vec3 {
        if ortho { self.target + Vec3::Z * self.distance } else { self.position() }
    }

    /// World-to-eye transform for the 2D front view: the eye on +Z looking
    /// down -Z at the target, Y up. The z=0 domain plane is therefore seen
    /// face-on, not obliquely (review I-1).
    pub fn front_view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye_position(true), self.target, Vec3::Y)
    }

    /// Perspective projection (right-handed).
    pub fn proj_perspective(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect, Self::NEAR, Self::FAR)
    }

    /// Orthographic projection centered on the target, used with
    /// [`Self::front_view`] for the 2D mode; `plane_size` is the world height
    /// covered by the plane at the default zoom, divided by [`Self::ortho_zoom`]
    /// so the 2D wheel zoom works through every caller (the viewport and
    /// `click_seed` share it).
    pub fn proj_ortho(&self, aspect: f32, plane_size: f32) -> Mat4 {
        let zoom = if self.ortho_zoom.is_finite() && self.ortho_zoom > 0.0 {
            self.ortho_zoom
        } else {
            1.0
        };
        let half_h = plane_size * 0.5 / zoom;
        let half_w = half_h * aspect;
        let far = self.distance + 100.0; // always cover the target plane
        Mat4::orthographic_rh(-half_w, half_w, -half_h, half_h, Self::NEAR, far)
    }

    /// Full view-projection; `Some(plane_size)` selects the 2D orthographic
    /// front view, which the viewport, screenshots and `click_seed` all share.
    pub fn vp_matrix(&self, aspect: f32, ortho_plane_size: Option<f32>) -> Mat4 {
        match ortho_plane_size {
            Some(size) => self.proj_ortho(aspect, size) * self.front_view(),
            None => self.proj_perspective(aspect) * self.view(),
        }
    }

    /// Ray through `ndc_xy` (in [-1,1]^2); direction intentionally unnormalized.
    ///
    /// Built analytically from the camera basis rather than by inverting the
    /// view-projection: at long eye distances the f32 inverse drifts the centre
    /// ray by ~1e-3, which would make a centre click miss the point it aims at.
    /// The origin is the eye and the direction is scaled so `t == 1` lands on
    /// the far plane (view depth `FAR`, which is farther than Euclidean `FAR`
    /// for off-centre rays).
    pub fn ndc_ray(&self, aspect: f32, ndc_xy: Vec2) -> (Vec3, Vec3) {
        let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        let half_h = (self.fov_y * 0.5).tan();
        let camera_dir = Vec3::new(ndc_xy.x * half_h * aspect, ndc_xy.y * half_h, -1.0) * Self::FAR;
        let direction = (self.view().inverse() * Vec4::new(camera_dir.x, camera_dir.y, camera_dir.z, 0.0))
            .truncate();
        (self.position(), direction)
    }
}

/// The active view camera: the orbit camera normally, or the fly camera while
/// ticket 46's free-flight mode is on. The viewport, screenshot capture and
/// click seeding read one of these instead of matching on the mode themselves,
/// so the framing they paint/seed is always the framing the user sees.
#[derive(Debug, Clone, Copy)]
pub enum ViewCamera<'a> {
    Orbit(&'a OrbitCamera),
    Fly(&'a FlyCamera),
}

impl ViewCamera<'_> {
    /// World-to-eye transform, for the painter's depth sort.
    pub fn view(&self) -> Mat4 {
        match self {
            ViewCamera::Orbit(camera) => camera.view(),
            ViewCamera::Fly(camera) => camera.view(),
        }
    }

    /// Full view-projection. `ortho_plane_size` selects the 2D orthographic
    /// front view; the fly camera is 3D-only and always uses its perspective.
    pub fn vp_matrix(&self, aspect: f32, ortho_plane_size: Option<f32>) -> Mat4 {
        match self {
            ViewCamera::Orbit(camera) => camera.vp_matrix(aspect, ortho_plane_size),
            ViewCamera::Fly(camera) => camera.vp_matrix(aspect),
        }
    }

    /// The eye the renderer's camera-facing ribbons orient against. `ortho` is
    /// the 2D mode for the orbit camera; the fly camera is always perspective.
    pub fn eye_position(&self, ortho: bool) -> Vec3 {
        match self {
            ViewCamera::Orbit(camera) => camera.eye_position(ortho),
            ViewCamera::Fly(camera) => camera.position,
        }
    }

    /// Ray through `ndc_xy` (in `[-1,1]^2`); direction intentionally
    /// unnormalized, see [`OrbitCamera::ndc_ray`].
    pub fn ndc_ray(&self, aspect: f32, ndc_xy: Vec2) -> (Vec3, Vec3) {
        match self {
            ViewCamera::Orbit(camera) => camera.ndc_ray(aspect, ndc_xy),
            ViewCamera::Fly(camera) => camera.ndc_ray(aspect, ndc_xy),
        }
    }

    /// Whether free flight is the active camera.
    pub fn is_fly(&self) -> bool {
        matches!(self, ViewCamera::Fly(_))
    }
}

/// Apply a saved 2D camera record (a preset's target and zoom) to the live
/// orbit camera. Applying a preset leaves fly mode first: presets are orbit
/// views and never carry a fly pose, so the user gets the saved framing back
/// rather than a stale free-flight view. Non-finite components are ignored, so
/// a hand-edited preset file cannot poison the camera.
pub fn apply_camera_record(
    record: &Camera,
    orbit: &mut OrbitCamera,
    fly: &mut Option<FlyCamera>,
) {
    *fly = None;
    if record.position[0].is_finite() {
        orbit.target.x = record.position[0];
    }
    if record.position[1].is_finite() {
        orbit.target.y = record.position[1];
    }
    if record.scale.is_finite() {
        orbit.ortho_zoom =
            record.scale.clamp(OrbitCamera::MIN_ORTHO_ZOOM, OrbitCamera::MAX_ORTHO_ZOOM);
    }
}

