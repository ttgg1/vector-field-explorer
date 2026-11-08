//! Orbit camera with perspective and orthographic projection plus NDC ray
//! unprojection (Task 6 of the Vector Field Explorer plan).

use glam::{Mat4, Vec2, Vec3, Vec4};

/// Spherical orbit camera around a target point.
///
/// The same camera drives both views: the perspective orbit in 3D mode and the
/// orthographic front view in 2D mode. `ortho_zoom` is the 2D view's zoom
/// factor (1.0 = fit the domain); the 3D zoom lives in `distance`, so the two
/// modes zoom independently without fighting over one field.
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
    const MIN_DISTANCE: f32 = 0.5;
    const MAX_DISTANCE: f32 = 20.0;
    const NEAR: f32 = 0.01;
    /// Tightest 2D zoom-in (a 20x magnification).
    pub const MIN_ORTHO_ZOOM: f32 = 0.05;
    /// Widest 2D zoom-out (50x the domain).
    pub const MAX_ORTHO_ZOOM: f32 = 50.0;

    /// Eye position derived from the spherical parameters.
    pub fn position(&self) -> Vec3 {
        self.target
            + self.distance
                * Vec3::new(
                    self.yaw.cos() * self.pitch.sin(),
                    self.pitch.cos(),
                    self.yaw.sin() * self.pitch.cos(),
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

    /// Multiply the 2D zoom, clamped so the view can never invert or vanish.
    pub fn zoom_2d(&mut self, factor: f32) {
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

    /// Perspective projection (right-handed).
    pub fn proj_perspective(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect, Self::NEAR, 100.0)
    }

    /// Orthographic projection centered on the target, facing along the view
    /// direction; `plane_size` is the world height covered by the plane at the
    /// default zoom, divided by [`Self::ortho_zoom`] so the 2D wheel zoom works
    /// through every caller (the viewport and `click_seed` share it).
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

    /// Full view-projection; `Some(plane_size)` selects the orthographic mode.
    pub fn vp_matrix(&self, aspect: f32, ortho_plane_size: Option<f32>) -> Mat4 {
        match ortho_plane_size {
            Some(size) => self.proj_ortho(aspect, size) * self.view(),
            None => self.proj_perspective(aspect) * self.view(),
        }
    }

    /// Ray through `ndc_xy` (in [-1,1]^2); direction intentionally unnormalized.
    pub fn ndc_ray(&self, aspect: f32, ndc_xy: Vec2) -> (Vec3, Vec3) {
        let inv_vp = self.vp_matrix(aspect, None).inverse();
        let unproject = |z: f32| {
            let p = inv_vp * Vec4::new(ndc_xy.x, ndc_xy.y, z, 1.0);
            Vec3::new(p.x, p.y, p.z) / p.w
        };
        let near = unproject(-1.0);
        let far = unproject(1.0);
        (near, far - near)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: Vec3, b: Vec3, what: &str) {
        let d = (a - b).length();
        assert!(d < 1e-4, "{what}: |{a:?} - {b:?}| = {d} > 1e-4");
    }

    fn ndc(m: Mat4, p: Vec4) -> Vec3 {
        let v = m * p;
        Vec3::new(v.x, v.y, v.z) / v.w
    }

    #[test]
    fn target_maps_to_vp_center() {
        let c = OrbitCamera::default();
        let vp = c.proj_perspective(1.0) * c.view();
        let ndc = ndc(vp, Vec4::new(0.0, 0.0, 0.0, 1.0));
        // x/y = screen center; z within the frustum (plain perspective_rh puts
        // the target near the far plane when near is tiny — depth-zero exactness
        // would require near/far symmetric about the orbit distance).
        assert_close(Vec3::new(ndc.x, ndc.y, 0.0), Vec3::ZERO, "vp center x/y");
        assert!(ndc.z.abs() < 1.0, "target must lie inside the depth range");
    }

    #[test]
    fn ndc_ray_at_center_looks_at_target() {
        let c = OrbitCamera::default();
        let (origin, dir) = c.ndc_ray(1.0, Vec2::ZERO);
        let to_target = (c.target - origin).normalize();
        assert_close(dir.normalize(), to_target, "ray direction");
    }

    #[test]
    fn zoom_clamps_distance() {
        let mut c = OrbitCamera::default();
        c.zoom(100.0);
        assert!((c.distance - 20.0).abs() < 1e-6);
        c.zoom(0.001);
        assert!((c.distance - 0.5).abs() < 1e-6);
    }

    #[test]
    fn ortho_maps_target_to_center() {
        let c = OrbitCamera::default();
        let m = c.proj_ortho(1.0, 2.0) * c.view();
        let ndc = ndc(m, Vec4::new(0.0, 0.0, 0.0, 1.0));
        // x/y = center (see the perspective-center test's ruling on z).
        assert_close(Vec3::new(ndc.x, ndc.y, 0.0), Vec3::ZERO, "ortho center x/y");
        assert!(ndc.z.abs() < 1.0, "target must lie inside the depth range");
    }

    #[test]
    fn orbit_changes_view() {
        let mut c = OrbitCamera::default();
        let before = c.view();
        c.orbit(0.5, 0.2);
        assert_ne!(before, c.view());
    }

    #[test]
    fn view_basis_is_orthonormal_and_perpendicular_to_the_view() {
        let c = OrbitCamera::default();
        let (right, up) = c.view_basis();
        let forward = (c.target - c.position()).normalize();
        assert!((right.length() - 1.0).abs() < 1e-5, "right is a unit vector");
        assert!((up.length() - 1.0).abs() < 1e-5, "up is a unit vector");
        assert!(right.dot(up).abs() < 1e-5, "the basis is orthogonal");
        assert!(right.dot(forward).abs() < 1e-5, "right is perpendicular to the view");
        assert!(up.dot(forward).abs() < 1e-5, "up is perpendicular to the view");
    }

    #[test]
    fn pan_moves_the_target_in_the_view_plane_only() {
        let mut c = OrbitCamera::default();
        let forward = (c.target - c.position()).normalize();
        let before = c.target;
        c.pan(1.0, 0.0);
        let moved = c.target - before;
        assert!((moved.length() - 1.0).abs() < 1e-5, "one world unit right");
        assert!(moved.dot(forward).abs() < 1e-5, "a pan never changes the view direction");
        assert!((c.distance - OrbitCamera::default().distance).abs() < 1e-6, "pan keeps the zoom");
        let mut c2 = OrbitCamera::default();
        c2.pan(0.0, 2.0);
        let (_, up) = c2.view_basis();
        assert!(up.dot(c2.target) > 0.0, "a positive up pan moves the target up the screen");
    }

    #[test]
    fn ortho_zoom_scales_the_visible_plane_and_clamps() {
        let mut c = OrbitCamera::default();
        let (_, up) = c.view_basis();
        let plane = 2.0;
        // At zoom 1 the half-height point sits on the top edge (ndc y = 1).
        let edge = c.target + up * (plane * 0.5);
        let edge_ndc = ndc(c.proj_ortho(1.0, plane) * c.view(), Vec4::new(edge.x, edge.y, edge.z, 1.0));
        assert!((edge_ndc.y - 1.0).abs() < 1e-4, "default zoom fits the plane exactly");

        // Zooming 2x halves the visible world: the same point is now off-screen,
        // and half the old height lands on the top edge.
        c.zoom_2d(2.0);
        let half = c.target + up * (plane * 0.25);
        let half_ndc = ndc(c.proj_ortho(1.0, plane) * c.view(), Vec4::new(half.x, half.y, half.z, 1.0));
        assert!((half_ndc.y - 1.0).abs() < 1e-4, "2x zoom shows half the world height");
        assert!((c.ortho_zoom - 2.0).abs() < 1e-6);

        c.zoom_2d(1e9);
        assert!((c.ortho_zoom - OrbitCamera::MAX_ORTHO_ZOOM).abs() < 1e-6, "zoom-in clamps");
        c.zoom_2d(1e-9);
        assert!((c.ortho_zoom - OrbitCamera::MIN_ORTHO_ZOOM).abs() < 1e-6, "zoom-out clamps");
        let before = c.ortho_zoom;
        c.zoom_2d(f32::NAN);
        assert!((c.ortho_zoom - before).abs() < 1e-6, "a nonsense zoom factor is ignored");
    }
}
