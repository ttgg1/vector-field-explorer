use glam::{DVec3, UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::source_glyph::{GlyphSign, source_glyph_lines, source_glyphs};
use vector_field_explorer::source_scene::{SourceDisplay, SourceEntry, SourceKindTag, SourceScene};
use vector_field_explorer::sources::{FieldFamily, Source, SourceKind};

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
