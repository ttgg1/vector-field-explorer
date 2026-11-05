#[cfg(test)]
mod tests {
    use crate::render::Camera;

    #[test]
    fn world_to_ndc_basic() {
        let cam = Camera { position: [0.0, 0.0], scale: 1.0 };
        let ndc = cam.world_to_ndc([1.0, 2.0]);
        assert!((ndc[0] - 1.0).abs() < 1e-6);
        assert!((ndc[1] - 2.0).abs() < 1e-6);
    }
}
