//! Task 1 source scene catalogue, CRUD and validation coverage.

use glam::{DVec3, UVec3, Vec3};
use vector_field_explorer::color_mapping::{
    ColorMapping, ColorPalette, ColorQuantity, ColorSample, ColorStats,
};
use vector_field_explorer::color_ramp::ScaleMode;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::sampling::{ViewOptions, magnitude_range};
use vector_field_explorer::source_scene::{
    ELECTRIC_COLOR, FieldMode, MAGNETIC_COLOR, MAX_SOFTENING, MIN_SOFTENING, SourceDisplay,
    SourceEntry, SourceKindTag, SourceScene, SourceSceneError, sample_source_scene,
};
use vector_field_explorer::sources::{Source, SourceKind};

fn view(
    colormap: Colormap,
    color_range: Option<(f32, f32)>,
    color_mapping: Option<ColorMapping>,
) -> ViewOptions {
    ViewOptions {
        colormap,
        color_range,
        color_mapping,
    }
}

fn mapping(quantity: ColorQuantity, manual_range: Option<(f32, f32)>) -> ColorMapping {
    ColorMapping {
        palette: ColorPalette::Builtin(Colormap::Turbo),
        mode: ScaleMode::Linear,
        quantity,
        manual_range,
    }
}

fn point_charge_scene() -> SourceScene {
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::new(0.3, 0.1, 0.2),
            axis: DVec3::Z,
            kind: SourceKindTag::PointCharge.default_source().kind,
        },
    });
    scene
}

fn sampled(
    scene: &SourceScene,
    view: &ViewOptions,
) -> Vec<vector_field_explorer::sampling::ArrowData> {
    sample_source_scene(
        scene,
        &Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0)),
        UVec3::new(5, 5, 5),
        None,
        view,
    )
    .arrows
}

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

#[test]
fn source_arrows_follow_the_active_colour_mapping() {
    let mut scene = point_charge_scene();
    scene.set_display(SourceDisplay::Electric);
    let legacy = sampled(&scene, &view(Colormap::Turbo, None, None));
    assert!(!legacy.is_empty(), "the colour comparison is not vacuous");

    let colour_mapping = mapping(ColorQuantity::ComponentX, None);
    let mapped = sampled(
        &scene,
        &view(Colormap::Turbo, None, Some(colour_mapping.clone())),
    );
    assert_eq!(legacy.len(), mapped.len());
    assert!(
        legacy
            .iter()
            .zip(&mapped)
            .any(|(old, new)| old.color != new.color),
        "the signed component mapping paints source arrows differently"
    );

    let samples: Vec<ColorSample> = mapped
        .iter()
        .map(|arrow| ColorSample {
            magnitude: arrow.mag,
            dir: arrow.dir,
        })
        .collect();
    let stats = ColorStats::from_samples(&samples);
    for (arrow, sample) in mapped.iter().zip(&samples) {
        assert_eq!(
            arrow.color,
            colour_mapping.sample(sample, &stats).rgb,
            "source arrow colour comes from the mapping and the family stats"
        );
    }
}

#[test]
fn source_legacy_colours_are_byte_identical_without_a_mapping() {
    let mut scene = point_charge_scene();
    scene.set_display(SourceDisplay::Electric);
    for range in [None, Some((0.2, 0.8))] {
        let view = view(Colormap::Viridis, range, None);
        let arrows = sampled(&scene, &view);
        assert!(!arrows.is_empty(), "the parity check is not vacuous");
        let (min, max) = view.color_range.unwrap_or_else(|| magnitude_range(&arrows));
        for arrow in &arrows {
            assert_eq!(
                arrow.color,
                view.colormap.colormap(arrow.mag, min, max),
                "the no-mapping path keeps the legacy colormap bytes"
            );
        }
    }
}

#[test]
fn both_display_keeps_fixed_family_colours_with_a_mapping() {
    let mut scene = point_charge_scene();
    scene
        .entries
        .push(SourceEntry {
            enabled: true,
            source: SourceKindTag::StraightWire.default_source(),
        });
    scene.set_display(SourceDisplay::Both);
    let arrows = sampled(
        &scene,
        &view(
            Colormap::Turbo,
            None,
            Some(mapping(ColorQuantity::ComponentX, Some((-1.0, 1.0)))),
        ),
    );
    assert!(!arrows.is_empty(), "Both drew arrows");
    let mut saw_electric = false;
    let mut saw_magnetic = false;
    for arrow in &arrows {
        match arrow.color {
            ELECTRIC_COLOR => saw_electric = true,
            MAGNETIC_COLOR => saw_magnetic = true,
            other => panic!("Both must ignore the mapping and keep family colours, got {other:?}"),
        }
    }
    assert!(saw_electric, "the electric family is drawn");
    assert!(saw_magnetic, "the magnetic family is drawn");
}
