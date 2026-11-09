//! Ticket 23: the arrow pipeline blends straight alpha.
//!
//! Plane-by-plane mode paints the off-plane arrows at alpha ~0.2, so the arrow
//! pipeline can no longer be `REPLACE`. The GPU pipeline itself needs a device,
//! but the blend state is a plain constant the pipeline is built from; these
//! tests pin it to straight alpha (`SrcAlpha` over `OneMinusSrcAlpha`) — for
//! both the 3D and the flat 2D arrow pipeline (ticket 45).

use eframe::wgpu::{BlendFactor, BlendOperation, BlendState};
use glam::Mat4;
use vector_field_explorer::renderer::{
    ARROW2D_BLEND, ARROW_BLEND, ARROW_KIND, ArrowInstanceGPU, PARTICLE_KIND, SceneGPU,
    combined_instances, sort_back_to_front, sort_scene_back_to_front,
};

#[test]
fn arrow_pipeline_blends_straight_alpha() {
    assert_eq!(ARROW_BLEND.color.src_factor, BlendFactor::SrcAlpha);
    assert_eq!(ARROW_BLEND.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
    assert_eq!(ARROW_BLEND.color.operation, BlendOperation::Add);
    assert_eq!(ARROW_BLEND.alpha.src_factor, BlendFactor::One);
    assert_eq!(ARROW_BLEND.alpha.dst_factor, BlendFactor::OneMinusSrcAlpha);
    assert_ne!(ARROW_BLEND, BlendState::REPLACE, "opaque replacement would ignore the dim alpha");
}

#[test]
fn flat_2d_arrow_selects_its_pipeline_and_keeps_the_alpha_blend() {
    let scene = SceneGPU { mode_3d: false, ..SceneGPU::default() };
    assert!(scene.uses_flat_arrow(), "2D draws the flat arrow pipeline");
    assert_eq!(scene.arrow_vertex_count(), 9, "the flat arrow draws nine vertices");
    assert_eq!(ARROW2D_BLEND, ARROW_BLEND, "the 2D pipeline shares the straight-alpha blend");
    assert_eq!(ARROW2D_BLEND.color.src_factor, BlendFactor::SrcAlpha);
    assert_eq!(ARROW2D_BLEND.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
}

#[test]
fn the_far_to_near_painter_sort_is_unchanged() {
    let instance = |z: f32| ArrowInstanceGPU {
        origin: [0.0, 0.0, z, 1.0],
        dir_len: [0.0, 1.0, 0.0, 1.0],
        color: [1.0, 1.0, 1.0, 1.0],
    };
    let mut instances = vec![instance(1.0), instance(-1.0), instance(0.0)];
    sort_back_to_front(&mut instances, &Mat4::IDENTITY);
    let zs: Vec<f32> = instances.iter().map(|instance| instance.origin[2]).collect();
    assert_eq!(zs, vec![-1.0, 0.0, 1.0], "the painter order still follows view depth");
}

/// One instance at `origin` tagged with the arrow/particle kind; the red
/// channel doubles as an identity sentinel for stable-order assertions.
fn tagged(origin: [f32; 4], sentinel: f32) -> ArrowInstanceGPU {
    ArrowInstanceGPU {
        origin,
        dir_len: [0.0, 1.0, 0.0, 1.0],
        color: [sentinel, 0.0, 0.0, 1.0],
    }
}

#[test]
fn scene_sort_interleaves_arrows_and_particles_by_depth() {
    // The input is deliberately shuffled and alternates kinds by depth, so a
    // per-kind or append-order sort would fail even if each list is ordered.
    let mut instances = vec![
        tagged([0.0, 0.0, 1.0, PARTICLE_KIND], 0.1),
        tagged([0.0, 0.0, -2.0, ARROW_KIND], 0.2),
        tagged([0.0, 0.0, -1.0, PARTICLE_KIND], 0.3),
        tagged([0.0, 0.0, 0.0, ARROW_KIND], 0.4),
    ];
    sort_scene_back_to_front(&mut instances, &Mat4::IDENTITY);
    let order: Vec<(f32, f32)> =
        instances.iter().map(|instance| (instance.origin[2], instance.origin[3])).collect();
    assert_eq!(
        order,
        vec![
            (-2.0, ARROW_KIND),
            (-1.0, PARTICLE_KIND),
            (0.0, ARROW_KIND),
            (1.0, PARTICLE_KIND),
        ],
        "arrows and particles interleave by depth, nearest last",
    );
}

#[test]
fn scene_sort_keeps_input_order_for_equal_depth() {
    let mut instances = vec![
        tagged([0.0, 0.0, 0.0, ARROW_KIND], 0.1),
        tagged([0.0, 0.0, 0.0, PARTICLE_KIND], 0.2),
        tagged([0.0, 0.0, 0.0, ARROW_KIND], 0.3),
        tagged([0.0, 0.0, 0.0, PARTICLE_KIND], 0.4),
    ];
    sort_scene_back_to_front(&mut instances, &Mat4::IDENTITY);
    let sentinels: Vec<f32> = instances.iter().map(|instance| instance.color[0]).collect();
    assert_eq!(
        sentinels,
        vec![0.1, 0.2, 0.3, 0.4],
        "the sort is stable: equal-depth instances keep their pre-sort order",
    );
}

#[test]
fn non_finite_depth_sorts_behind_without_panicking() {
    let depths = [f32::NAN, 0.0, f32::INFINITY, -5.0, f32::NEG_INFINITY];
    let mut instances: Vec<ArrowInstanceGPU> = depths
        .iter()
        .enumerate()
        .map(|(index, depth)| tagged([0.0, 0.0, *depth, ARROW_KIND], index as f32))
        .collect();
    sort_scene_back_to_front(&mut instances, &Mat4::IDENTITY);
    let order: Vec<f32> = instances.iter().map(|instance| instance.color[0]).collect();
    assert_eq!(
        order,
        vec![0.0, 2.0, 4.0, 3.0, 1.0],
        "non-finite depths join the far group (first, painted behind) in stable input order, \
         then the finite -5 and 0",
    );
}

#[test]
fn scene_sort_handles_empty_and_single_kind_inputs() {
    let mut empty: Vec<ArrowInstanceGPU> = Vec::new();
    sort_scene_back_to_front(&mut empty, &Mat4::IDENTITY);
    assert!(empty.is_empty(), "an empty scene sorts without panicking");

    let mut arrows = vec![
        tagged([0.0, 0.0, 1.0, ARROW_KIND], 0.1),
        tagged([0.0, 0.0, -1.0, ARROW_KIND], 0.2),
    ];
    sort_scene_back_to_front(&mut arrows, &Mat4::IDENTITY);
    let order: Vec<f32> = arrows.iter().map(|instance| instance.color[0]).collect();
    assert_eq!(order, vec![0.2, 0.1], "an arrows-only scene still sorts by depth");
}

#[test]
fn combined_instances_tags_arrows_and_particles_and_keeps_their_fields() {
    // An arrow arriving with the wrong tag is corrected to 1.0; a dot arriving
    // with 1.0 is corrected to 0.0. Radius stays in dir_len.w and colour moves
    // through untouched.
    let arrows = vec![ArrowInstanceGPU {
        origin: [1.0, 2.0, 3.0, 9.0],
        dir_len: [0.0, 1.0, 0.0, 4.5],
        color: [0.25, 0.5, 0.75, 0.4],
    }];
    let dots = vec![ArrowInstanceGPU {
        origin: [4.0, 5.0, 6.0, 1.0],
        dir_len: [0.0, 0.0, 1.0, 0.12],
        color: [0.1, 0.2, 0.3, 1.0],
    }];
    let combined = combined_instances(&arrows, &dots);
    assert_eq!(combined.len(), 2, "concat keeps every instance");
    assert_eq!(combined[0].origin[3], ARROW_KIND, "arrows carry the arrow tag");
    assert_eq!(combined[0].origin[..3], [1.0, 2.0, 3.0], "arrow position is untouched");
    assert_eq!(combined[0].dir_len, arrows[0].dir_len, "arrow direction and length survive");
    assert_eq!(combined[0].color, arrows[0].color, "arrow colour survives");
    assert_eq!(combined[1].origin[3], PARTICLE_KIND, "dots carry the particle tag");
    assert_eq!(combined[1].origin[..3], [4.0, 5.0, 6.0], "dot centre is untouched");
    assert_eq!(combined[1].dir_len[3], 0.12, "the dot radius stays in dir_len.w");
    assert_eq!(combined[1].color, dots[0].color, "dot colour survives");
}

#[test]
fn particle_free_combined_keeps_the_arrow_list_and_sorted_order() {
    // Pixel identity when particles are off: the combined stream is built from
    // the same arrows in the same input order and run through the same
    // comparator as the long-standing arrow sort.
    let arrows = vec![
        tagged([0.0, 0.0, 2.0, ARROW_KIND], 0.1),
        tagged([0.0, 0.0, -3.0, ARROW_KIND], 0.2),
        tagged([0.0, 0.0, -1.0, ARROW_KIND], 0.3),
        tagged([0.0, 0.0, 0.0, ARROW_KIND], 0.4),
    ];
    let mut combined = combined_instances(&arrows, &[]);
    assert_eq!(combined.len(), arrows.len(), "particles off: one combined instance per arrow");

    sort_scene_back_to_front(&mut combined, &Mat4::IDENTITY);
    let mut reference = arrows.clone();
    sort_back_to_front(&mut reference, &Mat4::IDENTITY);
    let combined_order: Vec<f32> =
        combined.iter().map(|instance| instance.color[0]).collect();
    let reference_order: Vec<f32> =
        reference.iter().map(|instance| instance.color[0]).collect();
    assert_eq!(
        combined_order, reference_order,
        "the combined stream orders arrows exactly like the legacy sort",
    );
}
