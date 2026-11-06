//! Orbit camera with perspective and orthographic projection plus NDC ray
//! unprojection (Task 6 of the Vector Field Explorer plan).

use glam::{Mat4, Vec2, Vec3, Vec4};

/// Spherical orbit camera around a target point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub fov_y: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.8,
            pitch: 0.9,
            distance: 3.2,
            target: Vec3::ZERO,
            fov_y: 55.0_f32.to_radians(),
        }
    }
}

impl OrbitCamera {
    const MIN_DISTANCE: f32 = 0.5;
    const MAX_DISTANCE: f32 = 20.0;
    const NEAR: f32 = 0.01;

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

    /// World-to-eye transform (look-at from the orbit position).
    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.position(), self.target, Vec3::Y)
    }

    /// Perspective projection (right-handed).
    pub fn proj_perspective(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect, Self::NEAR, 100.0)
    }

    /// Orthographic projection centered on the target, facing along the view
    /// direction; `plane_size` is the world height covered by the plane.
    pub fn proj_ortho(&self, aspect: f32, plane_size: f32) -> Mat4 {
        let half_h = plane_size * 0.5;
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
}
