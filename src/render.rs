// Basic rendering utilities and Camera abstraction.

/// Simple 2D camera for zoom & pan.
pub struct Camera {
    /// Position in world space (center of view).
    pub position: [f32; 2],
    /// Scale factor (1.0 = 100% viewport size). Positive values only.
    pub scale: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { position: [0.0, 0.0], scale: 1.0 }
    }
}

impl Camera {
    /// Convert world coordinates to screen space (normalized device coords)
    pub fn world_to_ndc(&self, world_pos: [f32; 2]) -> [f32; 2] {
        let dx = world_pos[0] - self.position[0];
        let dy = world_pos[1] - self.position[1];
        // Assume viewport size is 2 units wide after scaling
        [dx / (self.scale * 1.0), dy / (self.scale * 1.0)]
    }

    /// Zoom by a multiplicative delta.
    pub fn zoom(&mut self, delta: f32) {
        // Prevent negative or zero scale
        let new_scale = self.scale * delta;
        self.scale = if new_scale > 0.01 { new_scale } else { 0.01 };
    }

    /// Pan by adding dx, dy to the camera position.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.position[0] += dx;
        self.position[1] += dy;
    }
}
