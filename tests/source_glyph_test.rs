use glam::{DVec3, UVec3, Vec3};
use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::layer_style::{LayerId, LayerOverride, LayerStyles, PhysicsFamily};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::renderer::LineInstanceGPU;
use vector_field_explorer::source_glyph::{
    GlyphSign, SourceGlyph, source_glyph_lines, source_glyph_lines_with,
    source_glyph_lines_with_layer, source_glyphs,
};
use vector_field_explorer::source_scene::{
    ELECTRIC_COLOR, MAGNETIC_COLOR, SourceDisplay, SourceEntry, SourceKindTag, SourceScene,
};
use vector_field_explorer::sources::{FieldFamily, Source, SourceKind};
use vector_field_explorer::viewmodel::State;

fn all_kinds() -> SourceScene {
    let mut scene = SourceScene::default();
    for tag in SourceKindTag::ALL {
        scene.entries.push(SourceEntry {
            enabled: true,
            source: tag.default_source(),
        });
    }
    scene
}

fn point(charge: f64, center: DVec3) -> SourceScene {
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center,
            axis: DVec3::Z,
            kind: SourceKind::PointCharge { charge },
        },
    });
    scene
}

#[test]
fn glyphs_cover_every_kind_with_family_colour_and_sign() {
    let domain = Domain::default();
    let scene = all_kinds();
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs.len(), 8);
    for tag in SourceKindTag::ALL {
        assert!(
            glyphs.iter().any(|glyph| glyph.kind == tag),
            "missing {tag:?}"
        );
    }
    let electric = glyphs
        .iter()
        .find(|glyph| glyph.kind == SourceKindTag::PointCharge)
        .unwrap();
    assert_eq!(electric.family, FieldFamily::Electric);
    assert_eq!(electric.sign, GlyphSign::Positive);
    let magnetic = glyphs
        .iter()
        .find(|glyph| glyph.kind == SourceKindTag::CurrentLoop)
        .unwrap();
    assert_eq!(magnetic.family, FieldFamily::Magnetic);
    assert_eq!(magnetic.sign, GlyphSign::Positive);

    let lines = source_glyph_lines(&glyphs, &domain);
    assert!(
        lines.len() > glyphs.len() * 4,
        "each kind has visible geometry"
    );
    let amber = [255.0 / 255.0, 176.0 / 255.0, 64.0 / 255.0];
    assert!(lines.iter().any(|line| {
        (line.color[0] - amber[0]).abs() < 1e-6
            && (line.color[1] - amber[1]).abs() < 1e-6
            && (line.color[2] - amber[2]).abs() < 1e-6
    }));
    assert!(lines.iter().all(|line| {
        line.a[..3]
            .iter()
            .chain(&line.b[..3])
            .all(|value| value.is_finite())
            && line.color.iter().all(|value| value.is_finite())
    }));
}

#[test]
fn negative_strengths_choose_the_minus_marker() {
    let domain = Domain::default();
    let scene = point(-1.0, DVec3::ZERO);
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0].sign, GlyphSign::Negative);
}

#[test]
fn glyph_center_and_axis_follow_the_source() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::new(1.0, 0.0, 0.0),
            axis: DVec3::Y,
            kind: SourceKind::FiniteLineCharge {
                charge: 1.0,
                length: 2.0,
            },
        },
    });
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs[0].center, Vec3::new(1.0, 0.0, 0.0));
    assert!((glyphs[0].axis - Vec3::Y).length() < 1e-6);
    let lines = source_glyph_lines(&glyphs, &domain);
    let ys: Vec<f32> = lines
        .iter()
        .flat_map(|line| [line.a[1], line.b[1]])
        .collect();
    assert!(
        ys.iter().any(|y| (*y - 1.0).abs() < 1e-5),
        "line end tick at +Y"
    );
    assert!(
        ys.iter().any(|y| (*y + 1.0).abs() < 1e-5),
        "line end tick at -Y"
    );
}

#[test]
fn family_selection_and_disabled_entries_filter_glyphs() {
    let domain = Domain::default();
    let mut scene = all_kinds();
    scene.entries[0].enabled = false;
    scene.set_display(SourceDisplay::Magnetic);
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs.len(), 3);
    assert!(
        glyphs
            .iter()
            .all(|glyph| glyph.family == FieldFamily::Magnetic)
    );
    assert!(!glyphs.iter().any(|glyph| glyph.index == 0));
}

#[test]
fn off_slice_glyphs_project_and_are_visually_distinct() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 2,
        },
        UVec3::new(5, 5, 5),
    );
    let mut scene = point(1.0, DVec3::new(0.0, 0.0, 2.0));
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::ZERO,
            axis: DVec3::Z,
            kind: SourceKind::PointCharge { charge: 1.0 },
        },
    });
    let glyphs = source_glyphs(&scene, &domain, Some(&frame));
    assert!(glyphs[0].off_slice);
    assert_eq!(glyphs[0].center.z, 0.0, "projected onto the selected plane");
    assert!(!glyphs[1].off_slice);
    let lines = source_glyph_lines(&glyphs, &domain);
    let off_alpha = lines
        .iter()
        .filter(|line| line.color[3] < 0.9)
        .map(|line| line.color[3])
        .fold(0.0, f32::max);
    assert!((off_alpha - 0.45).abs() < 1e-6);
    assert!(lines.iter().any(|line| (line.color[3] - 0.9).abs() < 1e-6));
}

// Issue #84 slice B1: layer/family styles apply to source and induction glyph
// lines. Neutral styles must reproduce the legacy output byte for byte.

fn layer_slot(layer: LayerId) -> usize {
    LayerId::ALL
        .iter()
        .position(|candidate| *candidate == layer)
        .expect("every layer is in LayerId::ALL")
}

fn family_slot(family: PhysicsFamily) -> usize {
    PhysicsFamily::ALL
        .iter()
        .position(|candidate| *candidate == family)
        .expect("every family is in PhysicsFamily::ALL")
}

fn styles_with(layer: LayerId, overrides: LayerOverride) -> LayerStyles {
    let mut styles = LayerStyles::default();
    styles.layers[layer_slot(layer)] = overrides;
    styles
}

fn family_styles(family: PhysicsFamily, overrides: LayerOverride) -> LayerStyles {
    let mut styles = LayerStyles::default();
    styles.families[family_slot(family)] = overrides;
    styles
}

fn all_kinds_at(z: f64) -> SourceScene {
    let mut scene = SourceScene::default();
    for tag in SourceKindTag::ALL {
        let mut source = tag.default_source();
        source.center = DVec3::new(source.center.x, source.center.y, z);
        scene.entries.push(SourceEntry {
            enabled: true,
            source,
        });
    }
    scene
}

fn plane_frame() -> PlaneFrame {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 2,
        },
        UVec3::new(5, 5, 5),
    )
}

fn same_line(a: &LineInstanceGPU, b: &LineInstanceGPU) -> bool {
    a.a == b.a && a.b == b.b && a.color == b.color
}

fn legacy_and_styled(
    glyphs: &[SourceGlyph],
    domain: &Domain,
    styles: &LayerStyles,
) -> (Vec<LineInstanceGPU>, Vec<LineInstanceGPU>) {
    (
        source_glyph_lines(glyphs, domain),
        source_glyph_lines_with(glyphs, domain, styles),
    )
}

fn rgb(color: [f32; 4]) -> [f32; 3] {
    [color[0], color[1], color[2]]
}

fn fixed(family: FieldFamily) -> [f32; 3] {
    let [red, green, blue] = match family {
        FieldFamily::Electric => ELECTRIC_COLOR,
        FieldFamily::Magnetic => MAGNETIC_COLOR,
    };
    [
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    ]
}

#[test]
fn neutral_glyph_styles_are_byte_identical() {
    let domain = Domain::default();
    let frame = plane_frame();
    let cases: [(SourceScene, Option<&PlaneFrame>, bool); 3] = [
        (all_kinds(), None, true),
        (all_kinds_at(0.0), Some(&frame), true),
        (all_kinds_at(2.0), Some(&frame), false),
    ];
    for (scene, plane, has_on_slice) in cases {
        let glyphs = source_glyphs(&scene, &domain, plane);
        assert!(!glyphs.is_empty());
        if has_on_slice {
            assert!(
                glyphs.iter().any(|glyph| !glyph.off_slice),
                "the case has on-slice geometry"
            );
        } else {
            assert!(
                glyphs.iter().all(|glyph| glyph.off_slice),
                "the case is fully off-slice"
            );
        }
        let legacy = source_glyph_lines(&glyphs, &domain);
        let styled = source_glyph_lines_with(&glyphs, &domain, &LayerStyles::default());
        assert_eq!(legacy.len(), styled.len());
        assert!(
            legacy.iter().zip(&styled).all(|(a, b)| same_line(a, b)),
            "neutral styles must be byte-identical"
        );
        assert!(legacy.len() > glyphs.len() * 4);
    }

    let glyphs = source_glyphs(&all_kinds_at(2.0), &domain, Some(&frame));
    assert!(glyphs.iter().all(|glyph| glyph.off_slice));
    let legacy = source_glyph_lines(&glyphs, &domain);
    let styled = source_glyph_lines_with(&glyphs, &domain, &LayerStyles::default());
    assert!(
        legacy.iter().zip(&styled).all(|(a, b)| same_line(a, b)),
        "neutral styles keep the off-slice dash and alpha"
    );
}

#[test]
fn source_layer_opacity_multiplies_glyph_alpha() {
    let domain = Domain::default();
    let glyphs = source_glyphs(&all_kinds_at(0.0), &domain, None);
    let styles = styles_with(
        LayerId::Sources,
        LayerOverride {
            opacity: Some(0.5),
            width: None,
            color: None,
        },
    );
    let (legacy, styled) = legacy_and_styled(&glyphs, &domain, &styles);
    assert_eq!(legacy.len(), styled.len());
    for (legacy, styled) in legacy.iter().zip(&styled) {
        assert!((styled.color[3] - legacy.color[3] * 0.5).abs() < 1e-6);
        assert_eq!(rgb(styled.color), rgb(legacy.color));
        assert_eq!(styled.b[3], legacy.b[3]);
    }
}

#[test]
fn source_layer_width_multiplies_glyph_width() {
    let domain = Domain::default();
    let glyphs = source_glyphs(&all_kinds_at(0.0), &domain, None);
    let styles = styles_with(
        LayerId::Sources,
        LayerOverride {
            opacity: None,
            width: Some(2.0),
            color: None,
        },
    );
    let (legacy, styled) = legacy_and_styled(&glyphs, &domain, &styles);
    assert_eq!(legacy.len(), styled.len());
    for (legacy, styled) in legacy.iter().zip(&styled) {
        assert!((styled.b[3] - legacy.b[3] * 2.0).abs() < 1e-6);
        assert_eq!(styled.color, legacy.color);
    }
}

#[test]
fn family_colour_overrides_replace_the_fixed_family_colour() {
    let domain = Domain::default();
    let glyphs = source_glyphs(&all_kinds(), &domain, None);
    let override_rgb = Rgb([12, 34, 56]);
    let styles = family_styles(
        PhysicsFamily::Electric,
        LayerOverride {
            opacity: None,
            width: None,
            color: Some(override_rgb),
        },
    );
    let (legacy, styled) = legacy_and_styled(&glyphs, &domain, &styles);
    let override_floats = [
        f32::from(override_rgb.0[0]) / 255.0,
        f32::from(override_rgb.0[1]) / 255.0,
        f32::from(override_rgb.0[2]) / 255.0,
    ];
    let mut saw_electric = false;
    let mut saw_magnetic = false;
    for (legacy, styled) in legacy.iter().zip(&styled) {
        let before = rgb(legacy.color);
        if before == fixed(FieldFamily::Electric) {
            saw_electric = true;
            assert_eq!(rgb(styled.color), override_floats);
        } else {
            saw_magnetic = true;
            assert_eq!(before, fixed(FieldFamily::Magnetic));
            assert_eq!(rgb(styled.color), before);
        }
        assert_eq!(styled.color[3], legacy.color[3], "colour keeps layer alpha");
    }
    assert!(saw_electric && saw_magnetic);
}

#[test]
fn induction_layer_applies_to_the_conductor_glyph() {
    let domain = Domain::default();
    let mut scene = SourceScene::default();
    scene.entries.push(SourceEntry {
        enabled: true,
        source: SourceKindTag::CurrentLoop.default_source(),
    });
    let glyphs = source_glyphs(&scene, &domain, None);
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0].family, FieldFamily::Magnetic);

    let mut styles = LayerStyles::default();
    styles.layers[layer_slot(LayerId::Sources)] = LayerOverride {
        opacity: Some(0.1),
        width: Some(0.5),
        color: None,
    };
    styles.layers[layer_slot(LayerId::Induction)] = LayerOverride {
        opacity: Some(0.5),
        width: Some(2.0),
        color: None,
    };
    let induction = source_glyph_lines_with_layer(&glyphs, &domain, &styles, LayerId::Induction);
    let sources = source_glyph_lines_with(&glyphs, &domain, &styles);
    let legacy = source_glyph_lines(&glyphs, &domain);
    for (legacy, induction) in legacy.iter().zip(&induction) {
        assert!((induction.color[3] - legacy.color[3] * 0.5).abs() < 1e-6);
        assert!((induction.b[3] - legacy.b[3] * 2.0).abs() < 1e-6);
    }
    for (legacy, sources) in legacy.iter().zip(&sources) {
        assert!((sources.color[3] - legacy.color[3] * 0.1).abs() < 1e-6);
        assert!((sources.b[3] - legacy.b[3] * 0.5).abs() < 1e-6);
    }
}

#[test]
fn layer_beats_family_per_attribute() {
    let domain = Domain::default();
    let glyphs = source_glyphs(&all_kinds(), &domain, None);
    let family_rgb = Rgb([200, 100, 50]);
    let mut styles = LayerStyles::default();
    styles.families[family_slot(PhysicsFamily::Electric)] = LayerOverride {
        opacity: Some(0.2),
        width: Some(3.0),
        color: Some(family_rgb),
    };
    styles.layers[layer_slot(LayerId::Sources)] = LayerOverride {
        opacity: Some(0.5),
        width: None,
        color: None,
    };
    let (legacy, styled) = legacy_and_styled(&glyphs, &domain, &styles);
    let family_floats = [
        f32::from(family_rgb.0[0]) / 255.0,
        f32::from(family_rgb.0[1]) / 255.0,
        f32::from(family_rgb.0[2]) / 255.0,
    ];
    let mut saw_electric = false;
    for (legacy, styled) in legacy.iter().zip(&styled) {
        if rgb(legacy.color) == fixed(FieldFamily::Electric) {
            saw_electric = true;
            assert!(
                (styled.color[3] - legacy.color[3] * 0.5).abs() < 1e-6,
                "layer opacity wins"
            );
            assert!(
                (styled.b[3] - legacy.b[3] * 3.0).abs() < 1e-6,
                "family width survives"
            );
            assert_eq!(rgb(styled.color), family_floats, "family colour survives");
        }
    }
    assert!(saw_electric);
}

#[test]
fn out_of_range_or_non_finite_glyph_styles_are_defensively_repaired() {
    let domain = Domain::default();
    let glyphs = source_glyphs(&all_kinds_at(0.0), &domain, None);
    let legacy = source_glyph_lines(&glyphs, &domain);
    let cases = [(-2.0_f32, 0.05_f32), (f32::INFINITY, 1.0), (99.0, 4.0)];
    for (written, multiplier) in cases {
        let mut state = State::new(true);
        state.layer_styles.layers[layer_slot(LayerId::Sources)] = LayerOverride {
            opacity: Some(f32::NAN),
            width: Some(written),
            color: None,
        };
        let styled = source_glyph_lines_with(&glyphs, &domain, state.layer_styles());
        assert_eq!(legacy.len(), styled.len());
        for (legacy, styled) in legacy.iter().zip(&styled) {
            assert!(styled.color[3].is_finite());
            assert!((0.0..=1.0).contains(&styled.color[3]));
            assert!(styled.b[3].is_finite());
            assert!((styled.b[3] - legacy.b[3] * multiplier).abs() < 1e-6);
        }
    }

    // A non-finite opacity is repaired as a multiplier, so the glyph keeps the
    // neutral base alpha (0.9 on-slice, 0.45 off-slice) instead of brightening
    // to fully opaque.
    let mut state = State::new(true);
    state.layer_styles.layers[layer_slot(LayerId::Sources)].opacity = Some(f32::NAN);
    let frame = plane_frame();
    let cases = [
        (source_glyphs(&all_kinds_at(0.0), &domain, None), true),
        (
            source_glyphs(&all_kinds_at(2.0), &domain, Some(&frame)),
            false,
        ),
    ];
    for (glyphs, has_on_slice) in cases {
        let legacy = source_glyph_lines(&glyphs, &domain);
        let styled = source_glyph_lines_with(&glyphs, &domain, state.layer_styles());
        assert_eq!(legacy.len(), styled.len());
        if has_on_slice {
            assert!(legacy.iter().any(|line| (line.color[3] - 0.9).abs() < 1e-6));
        } else {
            assert!(glyphs.iter().all(|glyph| glyph.off_slice));
            assert!(legacy
                .iter()
                .all(|line| (line.color[3] - 0.45).abs() < 1e-6));
        }
        for (legacy, styled) in legacy.iter().zip(&styled) {
            assert!(styled.color[3].is_finite());
            assert!((0.0..=1.0).contains(&styled.color[3]));
            assert!(
                (styled.color[3] - legacy.color[3]).abs() < 1e-6,
                "a non-finite opacity repairs to the neutral multiplier, so the \
                 styled alpha equals the legacy base alpha"
            );
        }
    }
}

#[test]
fn off_slice_alpha_still_multiplies() {
    let domain = Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0));
    let frame = plane_frame();
    let mut scene = point(1.0, DVec3::new(0.0, 0.0, 2.0));
    scene.entries.push(SourceEntry {
        enabled: true,
        source: Source {
            center: DVec3::ZERO,
            axis: DVec3::Z,
            kind: SourceKind::PointCharge { charge: 1.0 },
        },
    });
    let glyphs = source_glyphs(&scene, &domain, Some(&frame));
    assert!(glyphs[0].off_slice);
    assert!(!glyphs[1].off_slice);
    let styles = styles_with(
        LayerId::Sources,
        LayerOverride {
            opacity: Some(0.5),
            width: None,
            color: None,
        },
    );
    let (legacy, styled) = legacy_and_styled(&glyphs, &domain, &styles);
    assert!(
        legacy
            .iter()
            .any(|line| (line.color[3] - 0.45).abs() < 1e-6)
    );
    assert!(legacy.iter().any(|line| (line.color[3] - 0.9).abs() < 1e-6));
    for (legacy, styled) in legacy.iter().zip(&styled) {
        assert!((styled.color[3] - legacy.color[3] * 0.5).abs() < 1e-6);
    }
}
