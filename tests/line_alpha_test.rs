//! Ticket 48: the line pipeline blends straight alpha so particle trails can
//! fade, dot opacity rides the already-alpha dot pipeline, and trails sit
//! between the backdrop lines and the combined 3D arrow + particle draw.

use std::sync::Arc;

use eframe::wgpu::{BlendFactor, BlendOperation, BlendState};
use glam::Mat4;
use vector_field_explorer::renderer::{
    ARROW_BLEND, DOT_BLEND, DrawPass, LINE_BLEND, LineInstanceGPU, MAX_ARROW_VERTS, SceneGPU,
    sort_lines_back_to_front, sorted_lines_back_to_front,
};

fn line(a: [f32; 3], b: [f32; 3], sentinel: f32) -> LineInstanceGPU {
    LineInstanceGPU {
        a: [a[0], a[1], a[2], 1.0],
        b: [b[0], b[1], b[2], 0.1],
        color: [sentinel, 0.0, 0.0, 1.0],
    }
}

#[test]
fn line_pipeline_blends_straight_alpha() {
    assert_eq!(LINE_BLEND.color.src_factor, BlendFactor::SrcAlpha);
    assert_eq!(LINE_BLEND.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
    assert_eq!(LINE_BLEND.color.operation, BlendOperation::Add);
    assert_eq!(LINE_BLEND.alpha.src_factor, BlendFactor::One);
    assert_eq!(LINE_BLEND.alpha.dst_factor, BlendFactor::OneMinusSrcAlpha);
    assert_ne!(LINE_BLEND, BlendState::REPLACE, "REPLACE would drop trail alpha");
    assert_eq!(LINE_BLEND, ARROW_BLEND, "lines share the arrow's straight-alpha rule");
}

#[test]
fn dot_pipeline_blends_straight_alpha() {
    assert_eq!(DOT_BLEND, BlendState::ALPHA_BLENDING, "particle opacity must blend");
    assert_eq!(DOT_BLEND.color.src_factor, BlendFactor::SrcAlpha);
    assert_eq!(DOT_BLEND.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
}

#[test]
fn scene_emptiness_includes_trails() {
    let mut scene = SceneGPU::default();
    assert!(scene.is_empty(), "a default scene is empty");
    scene.trails = Arc::new(vec![line([0.0; 3], [1.0, 0.0, 0.0], 0.1)]);
    assert!(!scene.is_empty(), "a trail segment makes the scene non-empty");
}

#[test]
fn trails_draw_between_lines_and_the_combined_scene() {
    let scene = SceneGPU {
        mode_3d: true,
        lines: Arc::new(vec![line([0.0; 3], [1.0, 0.0, 0.0], 0.1)]),
        trails: Arc::new(vec![
            line([0.0; 3], [1.0, 0.0, 0.0], 0.2),
            line([1.0, 0.0, 0.0], [2.0, 0.0, 0.0], 0.3),
        ]),
        combined: Arc::new(Vec::new()),
        ..SceneGPU::default()
    };
    assert_eq!(
        scene.draw_passes(),
        vec![
            DrawPass::Lines { instances: 1 },
            DrawPass::Trails { instances: 2 },
        ],
        "trails paint after the backdrop and before the combined stream",
    );

    let with_arrows = SceneGPU {
        combined: Arc::new(vec![vector_field_explorer::renderer::ArrowInstanceGPU {
            origin: [0.0; 4],
            dir_len: [0.0, 1.0, 0.0, 1.0],
            color: [1.0; 4],
        }]),
        ..scene.clone()
    };
    assert_eq!(
        with_arrows.draw_passes(),
        vec![
            DrawPass::Lines { instances: 1 },
            DrawPass::Trails { instances: 2 },
            DrawPass::Scene3D { instances: 1, budget: MAX_ARROW_VERTS },
        ],
        "the combined 3D draw comes last, with trails deliberately behind it",
    );
}

#[test]
fn two_d_never_draws_trails() {
    let scene = SceneGPU {
        mode_3d: false,
        trails: Arc::new(vec![line([0.0; 3], [1.0, 0.0, 0.0], 0.1)]),
        ..SceneGPU::default()
    };
    assert!(
        !scene.draw_passes().iter().any(|pass| matches!(pass, DrawPass::Trails { .. })),
        "particles are 3D-only, so 2D hides their trails",
    );
}

#[test]
fn trails_sort_far_to_near_by_their_midpoint() {
    let mut trails = vec![
        line([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 0.1),
        line([0.0, 0.0, -2.0], [0.0, 0.0, -2.0], 0.2),
        line([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.3),
    ];
    sort_lines_back_to_front(&mut trails, &Mat4::IDENTITY);
    let sentinels: Vec<f32> = trails.iter().map(|line| line.color[0]).collect();
    assert_eq!(sentinels, vec![0.2, 0.3, 0.1], "nearest paints last");

    let sorted = sorted_lines_back_to_front(&trails, &Mat4::IDENTITY);
    assert_eq!(sorted.len(), 3, "the sorted copy keeps every segment");
}

#[test]
fn non_finite_trail_depths_sort_behind_without_panicking() {
    let mut trails = vec![
        line([0.0, 0.0, f32::NAN], [0.0, 0.0, f32::NAN], 0.1),
        line([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.2),
    ];
    sort_lines_back_to_front(&mut trails, &Mat4::IDENTITY);
    let sentinels: Vec<f32> = trails.iter().map(|line| line.color[0]).collect();
    assert_eq!(sentinels, vec![0.1, 0.2], "a bad midpoint joins the far group");
}
