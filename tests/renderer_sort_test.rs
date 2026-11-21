//! Issue #245: painter-order sorting precomputes one view-depth key per
//! element (decorate–sort–undecorate) instead of transforming inside the
//! comparator. These tests pin the unchanged ordering contract exactly:
//! byte-identical output, far-to-near, NaN-in-the-far-group, and stability.

use bytemuck::cast_slice;
use glam::{Mat4, Vec3};
use vector_field_explorer::renderer::{
    ArrowInstanceGPU, LineInstanceGPU, line_view_depths, scene_view_depths,
    sort_lines_back_to_front, sort_scene_back_to_front, sorted_arrows_back_to_front,
    sorted_lines_back_to_front,
};

fn line(a: [f32; 3], b: [f32; 3], sentinel: f32) -> LineInstanceGPU {
    LineInstanceGPU {
        a: [a[0], a[1], a[2], 1.0],
        b: [b[0], b[1], b[2], 0.1],
        color: [sentinel, 0.0, 0.0, 1.0],
    }
}

/// A line whose midpoint sits exactly at depth `z` under [`Mat4::IDENTITY`].
fn zline(z: f32, sentinel: f32) -> LineInstanceGPU {
    line([0.0, 0.0, z], [0.0, 0.0, z], sentinel)
}

fn arrow(origin: [f32; 3], sentinel: f32) -> ArrowInstanceGPU {
    ArrowInstanceGPU {
        origin: [origin[0], origin[1], origin[2], 1.0],
        dir_len: [0.0, 1.0, 0.0, 1.0],
        color: [sentinel, 0.0, 0.0, 1.0],
    }
}

/// The pre-#245 line comparator, kept verbatim as the observable reference.
fn legacy_line_depth(line: &LineInstanceGPU, view: &Mat4) -> f32 {
    let midpoint = Vec3::new(
        (line.a[0] + line.b[0]) * 0.5,
        (line.a[1] + line.b[1]) * 0.5,
        (line.a[2] + line.b[2]) * 0.5,
    );
    let z = (*view * midpoint.extend(1.0)).z;
    if z.is_finite() { z } else { f32::NEG_INFINITY }
}

fn legacy_sort_lines(lines: &mut [LineInstanceGPU], view: &Mat4) {
    lines.sort_by(|a, b| {
        legacy_line_depth(a, view)
            .partial_cmp(&legacy_line_depth(b, view))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// The pre-#245 arrow comparator, kept verbatim as the observable reference.
fn legacy_arrow_depth(instance: &ArrowInstanceGPU, view: &Mat4) -> f32 {
    let position = Vec3::new(instance.origin[0], instance.origin[1], instance.origin[2]);
    let z = (*view * position.extend(1.0)).z;
    if z.is_finite() { z } else { f32::NEG_INFINITY }
}

fn legacy_sort_arrows(instances: &mut [ArrowInstanceGPU], view: &Mat4) {
    instances.sort_by(|a, b| {
        legacy_arrow_depth(a, view)
            .partial_cmp(&legacy_arrow_depth(b, view))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// A view that mixes all three world axes into view-space z, so a depth
/// computed from the wrong coordinate or midpoint would show up.
fn oblique_view() -> Mat4 {
    Mat4::from_rotation_x(0.4)
        * Mat4::from_rotation_y(0.3)
        * Mat4::from_translation(Vec3::new(0.7, -0.2, 1.3))
}

/// Mixed depths, equal-depth ties (same midpoint, different payload) and a
/// non-finite midpoint.
fn mixed_lines() -> Vec<LineInstanceGPU> {
    vec![
        line([0.0, 0.0, -2.0], [0.0, 0.0, -2.0], 1.0),
        line([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 2.0),
        line([0.0, 0.0, -2.0], [0.0, 0.0, -2.0], 3.0),
        line([f32::NAN, 0.0, 0.0], [f32::NAN, 0.0, 0.0], 4.0),
        line([1.0, 0.0, 1.0], [-1.0, 0.0, 1.0], 5.0),
        line([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 6.0),
        line([0.5, -1.0, 0.0], [0.5, 1.0, 0.0], 7.0),
    ]
}

fn mixed_arrows() -> Vec<ArrowInstanceGPU> {
    vec![
        arrow([0.0, 0.0, -2.0], 1.0),
        arrow([0.0, 0.0, 1.0], 2.0),
        arrow([0.0, 0.0, -2.0], 3.0),
        arrow([f32::NAN, 0.0, 0.0], 4.0),
        arrow([1.0, 0.0, 1.0], 5.0),
        arrow([0.0, 0.0, 1.0], 6.0),
    ]
}

#[test]
fn lines_sort_byte_identically_to_the_legacy_comparator() {
    let view = oblique_view();
    let mut expected = mixed_lines();
    legacy_sort_lines(&mut expected, &view);

    let mut actual = mixed_lines();
    sort_lines_back_to_front(&mut actual, &view);

    assert_eq!(
        cast_slice::<LineInstanceGPU, u8>(&actual),
        cast_slice::<LineInstanceGPU, u8>(&expected),
        "in-place trail ordering must stay byte-identical"
    );

    let copied = sorted_lines_back_to_front(&mixed_lines(), &view);
    assert_eq!(
        cast_slice::<LineInstanceGPU, u8>(&copied),
        cast_slice::<LineInstanceGPU, u8>(&expected),
        "the sorted copy must match the in-place result"
    );
}

#[test]
fn arrows_sort_byte_identically_to_the_legacy_comparator() {
    let view = oblique_view();
    let mut expected = mixed_arrows();
    legacy_sort_arrows(&mut expected, &view);

    let mut actual = mixed_arrows();
    sort_scene_back_to_front(&mut actual, &view);
    assert_eq!(
        cast_slice::<ArrowInstanceGPU, u8>(&actual),
        cast_slice::<ArrowInstanceGPU, u8>(&expected),
        "the shared decorate–sort–undecorate path must not change arrow order"
    );

    let copied = sorted_arrows_back_to_front(&mixed_arrows(), &view);
    assert_eq!(
        cast_slice::<ArrowInstanceGPU, u8>(&copied),
        cast_slice::<ArrowInstanceGPU, u8>(&expected),
        "the sorted arrow copy must match the in-place result"
    );
}

#[test]
fn trails_sort_far_to_near_by_their_midpoint() {
    let mut trails = vec![
        line([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 0.1),
        line([0.0, 0.0, -2.0], [0.0, 0.0, -2.0], 0.2),
        // Midpoint is z = 0 even though the endpoints straddle it.
        line([0.0, 0.0, -2.0], [0.0, 0.0, 2.0], 0.3),
    ];
    sort_lines_back_to_front(&mut trails, &Mat4::IDENTITY);
    let sentinels: Vec<f32> = trails.iter().map(|line| line.color[0]).collect();
    assert_eq!(sentinels, vec![0.2, 0.3, 0.1], "nearest paints last");
}

#[test]
fn equal_depths_keep_chronological_order() {
    let mut trails = vec![
        line([0.0, 0.0, 0.5], [0.0, 0.0, 0.5], 10.0),
        line([0.0, 0.0, 0.5], [0.0, 0.0, 0.5], 20.0),
        line([0.0, 0.0, 0.5], [0.0, 0.0, 0.5], 30.0),
    ];
    sort_lines_back_to_front(&mut trails, &Mat4::IDENTITY);
    let sentinels: Vec<f32> = trails.iter().map(|line| line.color[0]).collect();
    assert_eq!(
        sentinels,
        vec![10.0, 20.0, 30.0],
        "a stable sort must preserve equal-depth input order"
    );
}

#[test]
fn non_finite_trail_depths_sort_behind_without_panicking() {
    let mut trails = vec![
        line([0.0, 0.0, f32::NAN], [0.0, 0.0, f32::NAN], 1.0),
        line([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 2.0),
    ];
    sort_lines_back_to_front(&mut trails, &Mat4::IDENTITY);
    let sentinels: Vec<f32> = trails.iter().map(|line| line.color[0]).collect();
    assert_eq!(
        sentinels,
        vec![1.0, 2.0],
        "a bad midpoint joins the far group"
    );
}

/// A view whose z translation is negative zero, so a midpoint at `-0.0`
/// keeps a `-0.0` key while a midpoint at `+0.0` yields `+0.0` (IEEE addition
/// of `+0.0 + -0.0` rounds to `+0.0`). That lets the test distinguish the two
/// zero keys without a GPU.
fn signed_zero_view() -> Mat4 {
    // Every z-row product is negative zero for a `-0.0` midpoint (and the
    // `+0.0` midpoint still sums to `+0.0`), so the transform preserves the
    // signed-zero distinction instead of normalising it.
    let mut view = Mat4::IDENTITY;
    view.x_axis.z = -0.0;
    view.y_axis.z = -0.0;
    view.w_axis.z = -0.0;
    view
}

#[test]
fn signed_zero_depths_are_ties_and_keep_input_order() {
    let view = signed_zero_view();
    let lines = vec![
        zline(0.0, 1.0),
        zline(-0.0, 2.0),
        zline(0.0, 3.0),
        zline(-0.0, 4.0),
    ];

    let keys = line_view_depths(&lines, &view);
    assert_eq!(
        keys.iter().map(|key| key.to_bits()).collect::<Vec<_>>(),
        vec![
            0.0_f32.to_bits(),
            (-0.0_f32).to_bits(),
            0.0_f32.to_bits(),
            (-0.0_f32).to_bits(),
        ],
        "the helper must preserve signed zero; the sort treats both as ties"
    );

    let mut sorted = lines.clone();
    sort_lines_back_to_front(&mut sorted, &view);
    let sentinels: Vec<f32> = sorted.iter().map(|line| line.color[0]).collect();
    assert_eq!(
        sentinels,
        vec![1.0, 2.0, 3.0, 4.0],
        "-0.0 and +0.0 are equal keys, so stable input order must survive"
    );
}

#[test]
fn equal_depths_keep_chronological_order_across_a_large_mixed_list() {
    // Ties sit between distinct depths, so an unstable partition has room to
    // move equal elements across each other.
    let depths = [-2.0_f32, 0.0, -1.0, 0.0, 0.0, -3.0, 0.0, -0.0, 0.0, 0.0]
        .into_iter()
        .cycle()
        .take(60)
        .collect::<Vec<_>>();
    let lines: Vec<LineInstanceGPU> = depths
        .iter()
        .enumerate()
        .map(|(index, &z)| zline(z, index as f32))
        .collect();

    let mut expected = lines.clone();
    // Stable reference sort, independent of the production permutation.
    expected.sort_by(|a, b| {
        legacy_line_depth(a, &Mat4::IDENTITY)
            .partial_cmp(&legacy_line_depth(b, &Mat4::IDENTITY))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut actual = lines;
    sort_lines_back_to_front(&mut actual, &Mat4::IDENTITY);

    let sentinels = |items: &[LineInstanceGPU]| -> Vec<f32> {
        items.iter().map(|line| line.color[0]).collect()
    };
    assert_eq!(
        sentinels(&actual),
        sentinels(&expected),
        "equal depths must keep input order even in a large mixed list"
    );
}

#[test]
fn all_non_finite_depths_keep_input_order_without_panicking() {
    let lines: Vec<LineInstanceGPU> = (0..12)
        .map(|index| line([f32::NAN, 0.0, 0.0], [f32::NAN, 0.0, 0.0], index as f32))
        .collect();
    assert!(
        line_view_depths(&lines, &Mat4::IDENTITY)
            .iter()
            .all(|key| *key == f32::NEG_INFINITY),
        "every non-finite midpoint joins the far group"
    );

    let mut sorted = lines.clone();
    sort_lines_back_to_front(&mut sorted, &Mat4::IDENTITY);
    let sentinels: Vec<f32> = sorted.iter().map(|line| line.color[0]).collect();
    assert_eq!(
        sentinels,
        (0..12).map(|index| index as f32).collect::<Vec<_>>(),
        "an all-far-group list keeps its input order"
    );

    let arrows: Vec<ArrowInstanceGPU> = (0..12)
        .map(|index| arrow([f32::NAN, 0.0, 0.0], index as f32))
        .collect();
    let mut sorted_arrows = arrows.clone();
    sort_scene_back_to_front(&mut sorted_arrows, &Mat4::IDENTITY);
    assert_eq!(
        cast_slice::<ArrowInstanceGPU, u8>(&sorted_arrows),
        cast_slice::<ArrowInstanceGPU, u8>(&arrows),
        "all-non-finite arrow depths keep their input order"
    );
}

#[test]
fn empty_and_single_element_inputs_return_without_mutation() {
    let view = oblique_view();

    let mut empty_lines: Vec<LineInstanceGPU> = Vec::new();
    sort_lines_back_to_front(&mut empty_lines, &view);
    assert!(empty_lines.is_empty());
    let mut empty_arrows: Vec<ArrowInstanceGPU> = Vec::new();
    sort_scene_back_to_front(&mut empty_arrows, &view);
    assert!(empty_arrows.is_empty());
    assert!(line_view_depths(&empty_lines, &view).is_empty());
    assert!(scene_view_depths(&empty_arrows, &view).is_empty());

    let single_line = vec![zline(-0.0, 7.0)];
    let line_before = cast_slice::<LineInstanceGPU, u8>(&single_line).to_vec();
    let mut sorted_line = single_line;
    sort_lines_back_to_front(&mut sorted_line, &view);
    assert_eq!(
        cast_slice::<LineInstanceGPU, u8>(&sorted_line),
        line_before.as_slice(),
        "a single line is already ordered; its bytes must not change"
    );

    let single_arrow = vec![arrow([0.0, 0.0, 0.0], 9.0)];
    let arrow_before = cast_slice::<ArrowInstanceGPU, u8>(&single_arrow).to_vec();
    let mut sorted_arrow = single_arrow;
    sort_scene_back_to_front(&mut sorted_arrow, &view);
    assert_eq!(
        cast_slice::<ArrowInstanceGPU, u8>(&sorted_arrow),
        arrow_before.as_slice(),
        "a single arrow is already ordered; its bytes must not change"
    );
}

#[test]
fn line_view_depths_evaluates_one_transform_per_element() {
    let view = oblique_view();
    let lines = mixed_lines();
    let keys = line_view_depths(&lines, &view);

    assert_eq!(
        keys.len(),
        lines.len(),
        "one depth key per element, not one per comparison"
    );
    for (key, line) in keys.iter().zip(&lines) {
        assert_eq!(
            key.to_bits(),
            legacy_line_depth(line, &view).to_bits(),
            "each key must be the same single midpoint transform the comparator used"
        );
    }
    assert_eq!(
        keys[3],
        f32::NEG_INFINITY,
        "a non-finite midpoint maps to the far group"
    );
}

#[test]
fn scene_view_depths_evaluates_one_transform_per_element() {
    let view = oblique_view();
    let instances = mixed_arrows();
    let keys = scene_view_depths(&instances, &view);

    assert_eq!(
        keys.len(),
        instances.len(),
        "one depth key per element, not one per comparison"
    );
    for (key, instance) in keys.iter().zip(&instances) {
        assert_eq!(
            key.to_bits(),
            legacy_arrow_depth(instance, &view).to_bits(),
            "each key must be the same single origin transform the comparator used"
        );
    }
    assert_eq!(
        keys[3],
        f32::NEG_INFINITY,
        "a non-finite origin maps to the far group"
    );
}
