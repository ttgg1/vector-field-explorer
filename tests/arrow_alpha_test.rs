//! Ticket 23: the arrow pipeline blends straight alpha.
//!
//! Plane-by-plane mode paints the off-plane arrows at alpha ~0.2, so the arrow
//! pipeline can no longer be `REPLACE`. The GPU pipeline itself needs a device,
//! but the blend state is a plain constant the pipeline is built from; these
//! tests pin it to straight alpha (`SrcAlpha` over `OneMinusSrcAlpha`).

use eframe::wgpu::{BlendFactor, BlendOperation, BlendState};
use vector_field_explorer::renderer::ARROW_BLEND;

#[test]
fn arrow_pipeline_blends_straight_alpha() {
    assert_eq!(ARROW_BLEND.color.src_factor, BlendFactor::SrcAlpha);
    assert_eq!(ARROW_BLEND.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
    assert_eq!(ARROW_BLEND.color.operation, BlendOperation::Add);
    assert_eq!(ARROW_BLEND.alpha.src_factor, BlendFactor::One);
    assert_eq!(ARROW_BLEND.alpha.dst_factor, BlendFactor::OneMinusSrcAlpha);
    assert_ne!(ARROW_BLEND, BlendState::REPLACE, "opaque replacement would ignore the dim alpha");
}
