#[cfg(test)]
mod tests {
    use crate::render::Camera;

    #[test]
    fn test_zoom_multiplicative() {
        let mut cam = Camera { position: [0.0, 0.0], scale: 1.0 };
        cam.zoom(2.0);
        assert!((cam.scale - 2.0).abs() < 1e-6);
        cam.zoom(0.5);
        assert!((cam.scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_pan_adds_position() {
        let mut cam = Camera { position: [1.0, 2.0], scale: 1.0 };
        cam.pan(3.0, -4.0);
        assert_eq!(cam.position, [4.0, -2.0]);
    }
}
