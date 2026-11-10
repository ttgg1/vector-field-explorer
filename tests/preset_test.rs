#[cfg(test)]
mod tests {
    use vector_field_explorer::{Camera, export};
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

    #[test]
    fn preset_json_round_trips_the_shared_shape() {
        let presets = vec![
            export::CameraPreset { name: "front".into(), position: [1.0, -2.5], scale: 3.0 },
            export::CameraPreset { name: "side".into(), position: [0.0, 4.0], scale: 1.5 },
        ];
        let json = export::presets_to_json(&presets).expect("encode");
        assert_eq!(export::presets_from_json(&json), Some(presets));
    }

    #[test]
    fn an_empty_preset_json_list_round_trips() {
        let json = export::presets_to_json(&[]).expect("encode empty");
        assert_eq!(export::presets_from_json(&json), Some(Vec::new()));
    }

    #[test]
    fn corrupt_preset_json_falls_back_to_none() {
        assert_eq!(export::presets_from_json("not json"), None);
        assert_eq!(export::presets_from_json("{}"), None);
        assert_eq!(
            export::presets_from_json(r#"[{"name":"broken"}]"#),
            None,
            "a missing field must not panic the browser load",
        );
    }
}
