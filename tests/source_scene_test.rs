//! Task 1 source scene catalogue, CRUD and validation coverage.

use glam::DVec3;
use vector_field_explorer::source_scene::{
    FieldMode, MAX_SOFTENING, MIN_SOFTENING, SourceDisplay, SourceKindTag, SourceScene,
    SourceSceneError,
};
use vector_field_explorer::sources::SourceKind;

#[test]
fn the_default_scene_is_empty_in_formula_mode() {
    let scene = SourceScene::default();
    assert_eq!(FieldMode::default(), FieldMode::Formula);
    assert_eq!(scene.display, SourceDisplay::Both);
    assert_eq!(scene.softening, 0.1);
    assert!(!scene.allow_inaccurate);
    assert!(scene.entries.is_empty());
    assert!(scene.validate().is_ok());
}

#[test]
fn all_eight_kinds_have_valid_defaults_and_a_family() {
    assert_eq!(SourceKindTag::ALL.len(), 8);
    for tag in SourceKindTag::ALL {
        let source = tag.default_source();
        let mut scene = SourceScene::default();
        scene.add(source).unwrap();
        scene.validate().unwrap();
        assert_eq!(scene.entries[0].source, source);
        assert_eq!(source.kind.family(), tag.family());
        assert!(tag.label().len() > 3);
    }
}

#[test]
fn add_remove_enable_and_reset_change_only_the_scene() {
    let mut scene = SourceScene::default();
    let mut source = SourceKindTag::ChargedRing.default_source();
    source.center = DVec3::new(2.0, 3.0, 4.0);
    let index = scene.add(source).unwrap();
    assert_eq!(index, 0);

    assert!(scene.set_enabled(0, false));
    assert!(!scene.entries[0].enabled);
    assert!(scene.set_enabled(0, true));

    assert!(scene.reset_entry(0));
    assert_eq!(scene.entries[0].source.center, DVec3::new(2.0, 3.0, 4.0));
    assert_eq!(
        scene.entries[0].source.kind,
        SourceKindTag::ChargedRing.default_source().kind
    );

    let removed = scene.remove(0).unwrap();
    assert_eq!(removed.source.center, DVec3::new(2.0, 3.0, 4.0));
    assert!(scene.remove(0).is_none());
}

#[test]
fn source_limit_stops_the_sixty_fifth_entry() {
    let mut scene = SourceScene::default();
    for _ in 0..64 {
        scene
            .add(SourceKindTag::PointCharge.default_source())
            .unwrap();
    }
    assert_eq!(
        scene.add(SourceKindTag::PointCharge.default_source()),
        Err(SourceSceneError::Limit { max: 64 })
    );
    assert_eq!(scene.entries.len(), 64);
}

#[test]
fn invalid_geometry_is_rejected_with_a_parameter_name() {
    let mut scene = SourceScene::default();
    let mut source = SourceKindTag::ChargedRing.default_source();
    let SourceKind::ChargedRing { radius, .. } = &mut source.kind else {
        unreachable!()
    };
    *radius = 0.0;
    assert_eq!(
        scene.add(source),
        Err(SourceSceneError::InvalidSource {
            index: 0,
            parameter: "radius",
        })
    );

    let mut nonfinite = SourceKindTag::PointCharge.default_source();
    if let SourceKind::PointCharge { charge } = &mut nonfinite.kind {
        *charge = f64::NAN;
    }
    assert!(matches!(
        scene.add(nonfinite),
        Err(SourceSceneError::InvalidSource { .. })
    ));
    assert!(scene.entries.is_empty());
}

#[test]
fn softening_is_bounded_and_never_nan() {
    let mut scene = SourceScene::default();
    assert!(scene.set_softening(MIN_SOFTENING).is_ok());
    assert!(scene.set_softening(MAX_SOFTENING).is_ok());
    assert_eq!(
        scene.set_softening(1e-9),
        Err(SourceSceneError::SofteningOutOfRange)
    );
    assert_eq!(
        scene.set_softening(f64::NAN),
        Err(SourceSceneError::SofteningOutOfRange)
    );
    scene.reset();
    assert_eq!(scene.softening, 0.1);
    assert_eq!(scene.display, SourceDisplay::Both);
    assert!(!scene.allow_inaccurate);
}

#[test]
fn disabled_entries_are_not_part_of_the_validated_scene() {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    scene.set_enabled(0, false);
    assert_eq!(scene.enabled_sources().count(), 0);
    assert!(scene.validate().is_ok());
    scene.set_display(SourceDisplay::Electric);
    assert_eq!(scene.display, SourceDisplay::Electric);
    assert!(!scene.entries[0].enabled);
}
