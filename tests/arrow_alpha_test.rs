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
    ARROW2D_BLEND, ARROW_BLEND, ArrowInstanceGPU, SceneGPU, sort_back_to_front,
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
