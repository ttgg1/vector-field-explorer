use glam::{DVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::object_scene::{
    MAX_DRAWN_TRIANGLES, MAX_OBJECTS, MeshPrimitive, MeshScene, MeshVertex, STATIC_TRIANGLES,
    STATIC_VERTICES, VERTEX_BYTES, object_transform, primitive_mesh,
};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame};
use vector_field_explorer::source_scene::{
    ELECTRIC_COLOR, MAGNETIC_COLOR, SourceDisplay, SourceEntry, SourceScene,
};
use vector_field_explorer::sources::{Source, SourceKind};

/// Barycentric corner markers in `push_triangle` order.
const CORNERS: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[test]
fn primitive_triangle_counts_match_the_budget() {
    assert_eq!(MeshPrimitive::Sphere.triangles(), 768);
    assert_eq!(MeshPrimitive::Cylinder.triangles(), 96);
    assert_eq!(MeshPrimitive::Torus.triangles(), 768);
    assert_eq!(MeshPrimitive::Tube.triangles(), 1152);
    assert_eq!(MeshPrimitive::Disc.triangles(), 48);
    assert_eq!(STATIC_TRIANGLES, 2_832);
    assert_eq!(STATIC_VERTICES, 8_496);
    // Secondary arithmetic check: the byte budget follows from the constants.
    assert_eq!(STATIC_VERTICES * VERTEX_BYTES, 305_856);
}

#[test]
fn vertex_layout_is_thirty_six_bytes_and_aligns_to_four() {
    // Pin the on-GPU vertex stride to the constant the renderer will use.
    assert_eq!(std::mem::size_of::<[f32; 3]>(), 12);
    assert_eq!(std::mem::size_of::<MeshVertex>(), VERTEX_BYTES);
    assert!(std::mem::align_of::<MeshVertex>() <= 4);
}

#[test]
fn every_primitive_mesh_has_finite_vertices_and_unit_normals() {
    for primitive in MeshPrimitive::ALL {
        let mesh = primitive_mesh(primitive);
        assert_eq!(mesh.vertices.len(), primitive.vertices());
        for v in mesh.vertices {
            assert!(v.position.iter().all(|x| x.is_finite()));
            let len =
                (v.normal[0] * v.normal[0] + v.normal[1] * v.normal[1] + v.normal[2] * v.normal[2])
                    .sqrt();
            assert!((len - 1.0).abs() < 1e-4, "{primitive:?} normal {len}");
        }
    }
}

#[test]
fn barycentric_corners_are_one_hot_per_triangle() {
    for primitive in MeshPrimitive::ALL {
        let mesh = primitive_mesh(primitive);
        for (triangle, corners) in mesh.vertices.as_chunks::<3>().0.iter().enumerate() {
            for (corner, v) in corners.iter().enumerate() {
                assert_eq!(
                    v.barycentric, CORNERS[corner],
                    "{primitive:?} triangle {triangle} corner {corner}"
                );
            }
        }
    }
}

#[test]
fn every_triangle_winds_ccw_from_outside_and_is_non_degenerate() {
    // Convention: each triangle `(p0, p1, p2)` is counter-clockwise seen from
    // outside the body, so `(p1 - p0) x (p2 - p0)` points along the stored
    // outward per-vertex normals. Flipping any triangle's vertex order makes the
    // dot product negative; collapsing two corners makes the cross vanish.
    for primitive in MeshPrimitive::ALL {
        let mesh = primitive_mesh(primitive);
        for (triangle, tri) in mesh.vertices.as_chunks::<3>().0.iter().enumerate() {
            let [v0, v1, v2] = *tri;
            let geometric = cross(sub(v1.position, v0.position), sub(v2.position, v0.position));
            let length = dot(geometric, geometric).sqrt();
            assert!(
                length > 1e-8,
                "{primitive:?} triangle {triangle} is degenerate (cross length {length})"
            );
            let outward = [
                v0.normal[0] + v1.normal[0] + v2.normal[0],
                v0.normal[1] + v1.normal[1] + v2.normal[1],
                v0.normal[2] + v1.normal[2] + v2.normal[2],
            ];
            let orientation = dot(geometric, outward);
            assert!(
                orientation > 0.0,
                "{primitive:?} triangle {triangle} winds inward (dot {orientation})"
            );
        }
    }
}

#[test]
fn generated_geometry_matches_static_budget() {
    // Tie the documented constants to the meshes actually generated.
    let per_primitive_triangles: usize = MeshPrimitive::ALL
        .iter()
        .map(|primitive| primitive.triangles())
        .sum();
    assert_eq!(per_primitive_triangles, STATIC_TRIANGLES);

    let vertices: usize = MeshPrimitive::ALL
        .iter()
        .map(|primitive| primitive_mesh(*primitive).vertices.len())
        .sum();
    let triangles: usize = MeshPrimitive::ALL
        .iter()
        .map(|primitive| primitive_mesh(*primitive).vertices.len() / 3)
        .sum();
    assert_eq!(vertices, STATIC_VERTICES);
    assert_eq!(triangles, STATIC_TRIANGLES);
}

#[test]
fn sphere_vertices_lie_on_the_unit_sphere() {
    for v in primitive_mesh(MeshPrimitive::Sphere).vertices {
        let r = (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
        assert!((r - 1.0).abs() < 1e-4);
    }
}

#[test]
fn cylinder_spans_unit_height_and_has_caps() {
    let mesh = primitive_mesh(MeshPrimitive::Cylinder);
    assert!(
        mesh.vertices
            .iter()
            .any(|v| (v.position[1] - 0.5).abs() < 1e-6)
    );
    assert!(
        mesh.vertices
            .iter()
            .any(|v| (v.position[1] + 0.5).abs() < 1e-6)
    );
    assert!(
        mesh.vertices
            .iter()
            .any(|v| (v.normal[1] - 1.0).abs() < 1e-6)
    );
}

#[test]
fn disc_is_two_sided_and_flat() {
    let mesh = primitive_mesh(MeshPrimitive::Disc);
    assert!(mesh.vertices.iter().all(|v| v.position[1].abs() < 1e-6));
    assert!(mesh.vertices.iter().any(|v| v.normal[1] > 0.9));
    assert!(mesh.vertices.iter().any(|v| v.normal[1] < -0.9));
}

#[test]
fn tube_has_twelve_visible_turns() {
    // Name kept to match the plan's contract; the body now proves the turn
    // count directly instead of only the height span.
    let mesh = primitive_mesh(MeshPrimitive::Tube);

    let min = mesh
        .vertices
        .iter()
        .map(|v| v.position[1])
        .fold(f32::INFINITY, f32::min);
    let max = mesh
        .vertices
        .iter()
        .map(|v| v.position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((max - min - 1.0).abs() < 1e-4, "tube spans unit height");
    assert_eq!(MeshPrimitive::Tube.triangles(), 12 * 8 * 6 * 2);

    // The generator emits 8 sides x 2 triangles x 3 vertices per helical step.
    // Average each step's ring (offsets cancel) to recover the path angle, then
    // unwrap and total the azimuthal advance: 12 turns == 24*pi radians.
    let vertices_per_step = 8 * 2 * 3;
    assert_eq!(
        mesh.vertices.len() % vertices_per_step,
        0,
        "tube vertices split into whole steps"
    );
    let mut first: Option<f32> = None;
    let mut previous: Option<f32> = None;
    let mut total = 0.0_f32;
    for step in mesh.vertices.chunks_exact(vertices_per_step) {
        let count = step.len() as f32;
        let x = step.iter().map(|v| v.position[0]).sum::<f32>() / count;
        let z = step.iter().map(|v| v.position[2]).sum::<f32>() / count;
        let angle = z.atan2(x);
        if let Some(previous) = previous {
            let mut delta = angle - previous;
            delta -= std::f32::consts::TAU * (delta / std::f32::consts::TAU).round();
            total += delta;
        } else {
            first = Some(angle);
        }
        previous = Some(angle);
    }
    // Close the loop from the last step back to the first so all 12 turns count.
    if let (Some(first), Some(previous)) = (first, previous) {
        let mut delta = first - previous;
        delta -= std::f32::consts::TAU * (delta / std::f32::consts::TAU).round();
        total += delta;
    }
    let expected = std::f32::consts::TAU * 12.0;
    assert!(
        (total.abs() - expected).abs() < 0.1,
        "tube wraps 12 turns: total azimuth {total} rad, expected {expected}"
    );
}

// --- Scene mapping, slice clip and fingerprint (Task 2) -------------------

const CENTER: [f64; 3] = [0.1, 0.2, 0.3];

fn source(kind: SourceKind, center: [f64; 3], axis: [f64; 3]) -> Source {
    Source {
        center: DVec3::from_array(center),
        axis: DVec3::from_array(axis),
        kind,
    }
}

fn entry(source: Source) -> SourceEntry {
    SourceEntry {
        enabled: true,
        source,
    }
}

fn scene(entries: Vec<SourceEntry>) -> SourceScene {
    SourceScene {
        entries,
        ..SourceScene::default()
    }
}

fn unit_domain() -> Domain {
    Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0))
}

fn column(transform: [[f32; 4]; 4], index: usize) -> Vec3 {
    Vec3::new(
        transform[index][0],
        transform[index][1],
        transform[index][2],
    )
}

fn translation(transform: [[f32; 4]; 4]) -> Vec3 {
    column(transform, 3)
}

fn determinant(transform: [[f32; 4]; 4]) -> f32 {
    column(transform, 0).dot(column(transform, 1).cross(column(transform, 2)))
}

fn assert_finite_transform(transform: [[f32; 4]; 4]) -> f32 {
    for (row, values) in transform.iter().enumerate() {
        for (col, value) in values.iter().enumerate() {
            assert!(value.is_finite(), "transform[{row}][{col}] = {value}");
        }
    }
    determinant(transform)
}

fn assert_near(actual: f32, expected: f32, label: &str) {
    assert!(
        (actual - expected).abs() < 1e-5,
        "{label}: expected {expected}, got {actual}"
    );
}

fn assert_vec_near(actual: Vec3, expected: Vec3, label: &str) {
    assert!(
        (actual - expected).length() < 1e-5,
        "{label}: expected {expected:?}, got {actual:?}"
    );
}

fn family_color(kind: &SourceKind) -> [f32; 3] {
    let [red, green, blue] = match kind {
        SourceKind::PointCharge { .. }
        | SourceKind::FiniteLineCharge { .. }
        | SourceKind::ChargedRing { .. }
        | SourceKind::ChargedDisc { .. }
        | SourceKind::SolidSphere { .. } => ELECTRIC_COLOR,
        SourceKind::StraightWire { .. }
        | SourceKind::CurrentLoop { .. }
        | SourceKind::Solenoid { .. } => MAGNETIC_COLOR,
    };
    [
        f32::from(red) / 255.0,
        f32::from(green) / 255.0,
        f32::from(blue) / 255.0,
    ]
}

fn expected_sign(kind: &SourceKind) -> i8 {
    let strength = match *kind {
        SourceKind::PointCharge { charge }
        | SourceKind::FiniteLineCharge { charge, .. }
        | SourceKind::ChargedRing { charge, .. }
        | SourceKind::ChargedDisc { charge, .. }
        | SourceKind::SolidSphere { charge, .. } => charge,
        SourceKind::StraightWire { current }
        | SourceKind::CurrentLoop { current, .. }
        | SourceKind::Solenoid { current, .. } => current,
    };
    if strength < 0.0 { -1 } else { 1 }
}

fn mapping_cases() -> Vec<(SourceKind, MeshPrimitive)> {
    vec![
        (
            SourceKind::PointCharge { charge: -2.0 },
            MeshPrimitive::Sphere,
        ),
        (
            SourceKind::FiniteLineCharge {
                charge: 3.0,
                length: 1.5,
            },
            MeshPrimitive::Cylinder,
        ),
        (
            SourceKind::ChargedRing {
                charge: -1.0,
                radius: 0.75,
            },
            MeshPrimitive::Torus,
        ),
        (
            SourceKind::ChargedDisc {
                charge: 2.0,
                radius: 0.5,
            },
            MeshPrimitive::Disc,
        ),
        (
            SourceKind::SolidSphere {
                charge: -3.0,
                radius: 0.4,
            },
            MeshPrimitive::Sphere,
        ),
        (
            SourceKind::StraightWire { current: -5.0 },
            MeshPrimitive::Cylinder,
        ),
        (
            SourceKind::CurrentLoop {
                current: 2.0,
                radius: 0.6,
            },
            MeshPrimitive::Torus,
        ),
        (
            SourceKind::Solenoid {
                current: -1.0,
                radius: 0.5,
                length: 1.2,
                turns: 8,
            },
            MeshPrimitive::Tube,
        ),
    ]
}

#[test]
fn every_source_kind_maps_to_its_primitive_sign_colour_and_centre() {
    for (kind, primitive) in mapping_cases() {
        let source = source(kind, CENTER, [0.0, 1.0, 0.0]);
        let mapped = MeshScene::from_sources(&scene(vec![entry(source)]), &unit_domain(), None);

        assert_eq!(mapped.objects().len(), 1, "{kind:?}");
        let object = &mapped.objects()[0];
        assert_eq!(object.primitive, primitive, "{kind:?}");
        assert_eq!(object.sign, expected_sign(&kind), "{kind:?}");
        assert_eq!(object.color, family_color(&kind), "{kind:?}");
        assert_eq!(object.opacity, 1.0, "{kind:?}");

        let determinant = assert_finite_transform(object.transform);
        assert!(determinant > 0.0, "{kind:?} determinant {determinant}");
        assert_vec_near(
            translation(object.transform),
            Vec3::new(CENTER[0] as f32, CENTER[1] as f32, CENTER[2] as f32),
            "mapped centre",
        );
    }
}

#[test]
fn non_axis_aligned_basis_is_orthonormal_with_a_positive_determinant() {
    let transform = object_transform(
        Vec3::new(1.0, -2.0, 0.5),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(2.0, 3.0, 4.0),
    );
    let right = column(transform, 0);
    let up = column(transform, 1);
    let normal = column(transform, 2);

    assert_near(right.length(), 2.0, "right length");
    assert_near(up.length(), 3.0, "up length");
    assert_near(normal.length(), 4.0, "normal length");
    assert!(right.dot(up).abs() < 1e-5);
    assert!(right.dot(normal).abs() < 1e-5);
    assert!(up.dot(normal).abs() < 1e-5);
    assert_vec_near(
        up.normalize(),
        Vec3::new(1.0, 2.0, 3.0).normalize(),
        "primitive Y follows the source axis",
    );
    assert!(determinant(transform) > 0.0);
}

#[test]
fn zero_or_non_finite_axis_falls_back_to_z_with_a_finite_transform() {
    let center = Vec3::new(0.25, 0.5, -0.75);
    let scale = Vec3::new(2.0, 3.0, 4.0);
    let z = object_transform(center, Vec3::Z, scale);
    let zero = object_transform(center, Vec3::ZERO, scale);
    let nan = object_transform(center, Vec3::new(f32::NAN, 0.0, 0.0), scale);
    let infinite = object_transform(center, Vec3::new(0.0, f32::INFINITY, 0.0), scale);

    assert_eq!(zero, z);
    assert_eq!(nan, z);
    assert_eq!(infinite, z);
    assert!(assert_finite_transform(z) > 0.0);
}

#[test]
fn transform_stays_finite_for_non_finite_center_and_scale() {
    for transform in [
        object_transform(
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::Y,
            Vec3::new(1.0, 1.0, 1.0),
        ),
        object_transform(
            Vec3::ZERO,
            Vec3::Y,
            Vec3::new(f32::NAN, f32::INFINITY, -1.0),
        ),
    ] {
        assert!(assert_finite_transform(transform) > 0.0);
    }
}

#[test]
fn unit_and_extent_scaling_follow_the_mapping_table() {
    let domain = unit_domain(); // extent 2 => unit 0.1
    let point = MeshScene::from_sources(
        &scene(vec![entry(source(
            SourceKind::PointCharge { charge: 1.0 },
            [0.0; 3],
            [0.0, 0.0, 1.0],
        ))]),
        &domain,
        None,
    );
    let sphere = point.objects()[0].transform;
    for index in 0..3 {
        assert_near(column(sphere, index).length(), 0.1, "point-charge radius");
    }

    let wire = MeshScene::from_sources(
        &scene(vec![entry(source(
            SourceKind::StraightWire { current: 1.0 },
            [0.0; 3],
            [0.0, 0.0, 1.0],
        ))]),
        &domain,
        None,
    );
    let cylinder = wire.objects()[0].transform;
    assert_near(column(cylinder, 1).length(), 4.4, "wire spans 2.2 extents");
    assert_near(
        column(cylinder, 0).length(),
        2.0 * 0.012 * 2.0,
        "wire radius 0.012 extents over a 0.5-radius mesh",
    );

    let disc = MeshScene::from_sources(
        &scene(vec![entry(source(
            SourceKind::ChargedDisc {
                charge: 1.0,
                radius: 0.5,
            },
            [0.0; 3],
            [0.0, 0.0, 1.0],
        ))]),
        &domain,
        None,
    );
    let flat = disc.objects()[0].transform;
    assert_near(column(flat, 0).length(), 0.5, "disc radius");
    assert_near(
        column(flat, 1).length(),
        0.04,
        "disc Y scale is reserved: the zero-thickness fan ignores it",
    );
}

#[test]
fn nan_radius_still_yields_a_finite_transform() {
    let mapped = MeshScene::from_sources(
        &scene(vec![entry(source(
            SourceKind::SolidSphere {
                charge: 1.0,
                radius: f64::NAN,
            },
            CENTER,
            [0.0, 1.0, 0.0],
        ))]),
        &unit_domain(),
        None,
    );
    assert_eq!(mapped.objects().len(), 1);
    assert!(assert_finite_transform(mapped.objects()[0].transform) > 0.0);
}

#[test]
fn non_finite_centres_are_skipped_without_panicking() {
    let mapped = MeshScene::from_sources(
        &scene(vec![
            entry(source(
                SourceKind::PointCharge { charge: 1.0 },
                [f64::NAN, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            )),
            entry(source(
                SourceKind::PointCharge { charge: 1.0 },
                [0.0, f64::INFINITY, 0.0],
                [0.0, 1.0, 0.0],
            )),
            entry(source(
                SourceKind::PointCharge { charge: 1.0 },
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            )),
        ]),
        &unit_domain(),
        None,
    );
    assert_eq!(mapped.objects().len(), 1);
}

#[test]
fn clip_matches_the_plane_frame_normal_and_offset() {
    let frame = PlaneFrame {
        axis: PlaneAxis::Z,
        origin: Vec3::new(1.0, 2.0, 3.0),
        right: Vec3::X,
        up: Vec3::Y,
        normal: Vec3::Z,
        half_u: 1.0,
        half_v: 1.0,
    };
    let mapped = MeshScene::from_sources(&scene(Vec::new()), &unit_domain(), Some(&frame));
    let clip = mapped.clip().expect("clip from frame");
    assert_eq!(clip.normal, [0.0, 0.0, 1.0]);
    assert_near(clip.offset, 3.0, "normal dot origin");

    let no_plane = MeshScene::from_sources(&scene(Vec::new()), &unit_domain(), None);
    assert!(no_plane.clip().is_none());
}

#[test]
fn both_display_filters_by_family() {
    let entries = vec![
        entry(source(
            SourceKind::PointCharge { charge: 1.0 },
            [-0.5, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        )),
        entry(source(
            SourceKind::CurrentLoop {
                current: 1.0,
                radius: 0.5,
            },
            [0.5, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        )),
    ];

    let mut both = scene(entries.clone());
    both.display = SourceDisplay::Both;
    assert_eq!(
        MeshScene::from_sources(&both, &unit_domain(), None)
            .objects()
            .len(),
        2
    );

    let mut electric = scene(entries.clone());
    electric.display = SourceDisplay::Electric;
    let electric_scene = MeshScene::from_sources(&electric, &unit_domain(), None);
    assert_eq!(electric_scene.objects().len(), 1);
    assert_eq!(
        electric_scene.objects()[0].color,
        family_color(&SourceKind::PointCharge { charge: 1.0 })
    );

    let mut magnetic = scene(entries);
    magnetic.display = SourceDisplay::Magnetic;
    let magnetic_scene = MeshScene::from_sources(&magnetic, &unit_domain(), None);
    assert_eq!(magnetic_scene.objects().len(), 1);
    assert_eq!(
        magnetic_scene.objects()[0].color,
        family_color(&SourceKind::CurrentLoop {
            current: 1.0,
            radius: 0.5
        })
    );
}

#[test]
fn disabled_entries_emit_nothing() {
    let mut disabled = entry(source(
        SourceKind::PointCharge { charge: 1.0 },
        [0.0; 3],
        [0.0, 1.0, 0.0],
    ));
    disabled.enabled = false;
    let mapped = MeshScene::from_sources(&scene(vec![disabled]), &unit_domain(), None);
    assert!(mapped.objects().is_empty());
    assert_eq!(mapped.drawn_triangles(), 0);
}

#[test]
fn object_order_is_the_scene_entry_order() {
    let entries: Vec<SourceEntry> = (0..5)
        .map(|index| {
            entry(source(
                SourceKind::PointCharge { charge: 1.0 },
                [f64::from(index), 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ))
        })
        .collect();
    let mapped = MeshScene::from_sources(&scene(entries), &unit_domain(), None);
    for (index, object) in mapped.objects().iter().enumerate() {
        assert_near(
            translation(object.transform).x,
            index as f32,
            "deterministic order",
        );
    }
}

#[test]
fn sixty_four_solenoids_fit_both_budgets() {
    assert_eq!(MAX_OBJECTS, 64);
    let entries: Vec<SourceEntry> = (0..64)
        .map(|index| {
            entry(source(
                SourceKind::Solenoid {
                    current: 1.0,
                    radius: 0.25,
                    length: 1.0,
                    turns: 12,
                },
                [f64::from(index % 4), 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ))
        })
        .collect();
    let mapped = MeshScene::from_sources(&scene(entries), &unit_domain(), None);
    assert_eq!(mapped.objects().len(), MAX_OBJECTS);
    assert_eq!(
        mapped.drawn_triangles(),
        64 * MeshPrimitive::Tube.triangles()
    );
    assert!(mapped.drawn_triangles() <= MAX_DRAWN_TRIANGLES);
}

#[test]
fn truncation_keeps_the_first_sixty_four_entries_and_drops_the_sixty_fifth() {
    assert_eq!(MAX_OBJECTS, 64);
    // Distinct centres make the dropped entry identifiable: entry `n` sits at
    // `x = n`, so the 65th entry (index 64) is the one that must disappear.
    let entries: Vec<SourceEntry> = (0..=MAX_OBJECTS)
        .map(|index| {
            entry(source(
                SourceKind::PointCharge { charge: 1.0 },
                [index as f64, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ))
        })
        .collect();
    let mapped = MeshScene::from_sources(&scene(entries), &unit_domain(), None);
    assert_eq!(mapped.objects().len(), MAX_OBJECTS);
    for (index, object) in mapped.objects().iter().enumerate() {
        assert_near(
            translation(object.transform).x,
            index as f32,
            "entry order preserved under truncation",
        );
    }
    let dropped = (MAX_OBJECTS as f32, 0.0, 0.0);
    assert!(
        mapped.objects().iter().all(|object| {
            let centre = translation(object.transform);
            (centre.x - dropped.0).abs() > 0.5 || centre.y.abs() > 0.5 || centre.z.abs() > 0.5
        }),
        "the 65th entry must be the dropped one, not an arbitrary entry"
    );
}

#[test]
fn triangle_budget_cannot_bind_before_the_object_budget() {
    // The triangle-budget break is unreachable with the current primitive set:
    // the largest body is the 1,152-triangle Tube, so MAX_OBJECTS (64) caps a
    // scene at 73,728 triangles, well under MAX_DRAWN_TRIANGLES (150,000). The
    // object budget always binds first; this pins the headroom that must hold
    // before a triangle-budget truncation path can be exercised.
    let largest = MeshPrimitive::ALL
        .iter()
        .map(|primitive| primitive.triangles())
        .max()
        .expect("at least one primitive");
    assert_eq!(largest, MeshPrimitive::Tube.triangles());
    assert!(MAX_OBJECTS * largest < MAX_DRAWN_TRIANGLES);
}

#[test]
fn nan_source_strength_maps_to_positive_sign() {
    // `source_sign` compares `strength < 0.0`, and every comparison with NaN is
    // false, so a NaN charge or current reads as positive rather than signless.
    for kind in [
        SourceKind::PointCharge { charge: f64::NAN },
        SourceKind::CurrentLoop {
            current: f64::NAN,
            radius: 1.0,
        },
    ] {
        let mapped = MeshScene::from_sources(
            &scene(vec![entry(source(kind, CENTER, [0.0, 1.0, 0.0]))]),
            &unit_domain(),
            None,
        );
        assert_eq!(mapped.objects().len(), 1, "{kind:?}");
        assert_eq!(mapped.objects()[0].sign, 1, "{kind:?}");
    }
}

#[test]
fn generation_is_stable_and_changes_with_mapped_input() {
    let make = |radius: f64| {
        scene(vec![entry(source(
            SourceKind::CurrentLoop {
                current: 1.0,
                radius,
            },
            CENTER,
            [1.0, 2.0, 3.0],
        ))])
    };
    let first = MeshScene::from_sources(&make(0.5), &unit_domain(), None);
    let again = MeshScene::from_sources(&make(0.5), &unit_domain(), None);
    let changed = MeshScene::from_sources(&make(0.75), &unit_domain(), None);

    assert_eq!(first.generation(), again.generation());
    assert_ne!(first.generation(), changed.generation());
    assert_ne!(first.generation(), MeshScene::empty().generation());
}

/// Issue #207: the generation deliberately omits the slice clip, so a cache
/// that keeps a scene on generation equality retains a 2D clip in 3D (and the
/// unclipped 3D scene in 2D). `cache_matches` is the one cache rule that also
/// compares the clip.
#[test]
fn cache_matches_compares_the_slice_clip_as_well_as_the_generation() {
    let sphere = || {
        scene(vec![entry(source(
            SourceKind::SolidSphere {
                charge: 1.0,
                radius: 0.5,
            },
            CENTER,
            [0.0, 1.0, 0.0],
        ))])
    };
    let frame = |axis: PlaneAxis| {
        let (right, up, normal) = match axis {
            PlaneAxis::X => (Vec3::Y, Vec3::Z, Vec3::X),
            PlaneAxis::Y => (Vec3::X, Vec3::Z, Vec3::Y),
            PlaneAxis::Z => (Vec3::X, Vec3::Y, Vec3::Z),
        };
        PlaneFrame {
            axis,
            origin: Vec3::ZERO,
            right,
            up,
            normal,
            half_u: 1.0,
            half_v: 1.0,
        }
    };
    let sources = sphere();
    let two_d = MeshScene::from_sources(&sources, &unit_domain(), Some(&frame(PlaneAxis::Y)));
    let three_d = MeshScene::from_sources(&sources, &unit_domain(), None);
    assert_eq!(
        two_d.generation(),
        three_d.generation(),
        "the 2D and 3D mappings of the same sources share a generation"
    );
    assert!(two_d.clip().is_some());
    assert!(three_d.clip().is_none());
    assert!(!two_d.cache_matches(&three_d));
    assert!(!three_d.cache_matches(&two_d));
    assert!(two_d.cache_matches(&two_d.clone()));
    assert!(three_d.cache_matches(&three_d.clone()));

    // Another plane changes the clip without changing the generation.
    let other = MeshScene::from_sources(&sources, &unit_domain(), Some(&frame(PlaneAxis::X)));
    assert_eq!(two_d.generation(), other.generation());
    assert!(!two_d.cache_matches(&other));

    // An empty enabled set still maps the plane: generation 0, clip Some.
    let empty_2d = MeshScene::from_sources(
        &scene(Vec::new()),
        &unit_domain(),
        Some(&frame(PlaneAxis::Y)),
    );
    let empty_3d = MeshScene::from_sources(&scene(Vec::new()), &unit_domain(), None);
    assert_eq!(empty_2d.generation(), 0);
    assert_eq!(empty_3d.generation(), 0);
    assert!(!empty_2d.cache_matches(&empty_3d));
    assert!(!empty_3d.cache_matches(&empty_2d));
}

#[test]
fn empty_scene_has_no_objects_or_triangles_and_a_distinct_generation() {
    let empty = MeshScene::empty();
    assert!(empty.objects().is_empty());
    assert_eq!(empty.drawn_triangles(), 0);
    assert!(empty.clip().is_none());

    let non_empty = MeshScene::from_sources(
        &scene(vec![entry(source(
            SourceKind::PointCharge { charge: 1.0 },
            [0.0; 3],
            [0.0, 1.0, 0.0],
        ))]),
        &unit_domain(),
        None,
    );
    assert_ne!(empty.generation(), non_empty.generation());
}
