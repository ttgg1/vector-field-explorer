#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load_preset() {
        let dir = tempdir().unwrap();
        let cam = Camera { position: [1.0, 2.0], scale: 3.0 };
        export::save_preset(dir.path(), "test", &cam).expect("save" );
        let presets = export::load_presets(dir.path()).expect("load");
        assert_eq!(presets.len(),1);
        assert_eq!(presets[0].name, "test");
        assert_eq!(presets[0].position, [1.0,2.0]);
        assert_eq!(presets[0].scale, 3.0);
    }
}
