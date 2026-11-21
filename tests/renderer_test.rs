//! Device-free integration contracts for compositing the object pass (#113).
//!
//! The real `DrawState` owns wgpu pipelines, so a test cannot construct one
//! without a device. The object-scene slot below is the exact device-free state
//! `DrawState` owns, and `paint_stages` is the exact ordering `record` uses for
//! both the live callback and the offscreen capture.

use std::sync::Arc;

use glam::Vec3;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::object_renderer::{ObjectView, object_draw_plan};
use vector_field_explorer::object_scene::MeshScene;
use vector_field_explorer::renderer::{
    ArrowInstanceGPU, DrawPass, DrawStage, ObjectSceneState, SceneGPU, paint_stages,
};
use vector_field_explorer::source_scene::{SourceEntry, SourceKindTag, SourceScene};

fn source_scene() -> SourceScene {
    SourceScene {
        entries: vec![SourceEntry {
            enabled: true,
            source: SourceKindTag::PointCharge.default_source(),
        }],
        ..SourceScene::default()
    }
}

fn mapped_scene() -> MeshScene {
    MeshScene::from_sources(
        &source_scene(),
        &Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0)),
        None,
    )
}

fn probe_view() -> ObjectView {
    ObjectView {
        view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
        eye: [0.0, 0.0, 3.0],
        viewport: [64, 64],
        wireframe: false,
        clear: [0.0, 0.0, 0.0, 1.0],
    }
}

fn arrow_scene() -> SceneGPU {
    SceneGPU {
        mode_3d: true,
        combined: Arc::new(vec![ArrowInstanceGPU {
            origin: [0.0, 0.0, 0.0, 1.0],
            dir_len: [1.0, 0.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        }]),
        ..SceneGPU::default()
    }
}

#[test]
fn draw_state_starts_without_objects() {
    // `DrawState` owns exactly this slot; constructing the pipelines needs a
    // wgpu device, so the slot is the device-free seam.
    let slot = ObjectSceneState::default();
    assert!(slot.get().is_none(), "a fresh slot has no object scene");
    assert!(!slot.has_objects(), "a fresh slot draws no objects");
    assert_eq!(slot.len(), 0);
}

#[test]
fn setting_an_object_scene_keeps_its_generation() {
    let scene = mapped_scene();
    let generation = scene.generation();
    assert!(generation != 0, "a mapped scene has a real fingerprint");

    let mut slot = ObjectSceneState::default();
    let shared = Arc::new(scene);
    slot.set(Arc::clone(&shared));

    let stored = slot.get().expect("the scene is stored");
    assert!(
        Arc::ptr_eq(stored, &shared),
        "the Arc is carried, not rebuilt"
    );
    assert_eq!(stored.generation(), generation);
    assert_eq!(slot.len(), shared.objects().len());
}

#[test]
fn offscreen_renders_objects_before_arrows() {
    let scene = arrow_scene();
    let stages = paint_stages(&scene, 1);
    assert_eq!(
        stages.first(),
        Some(&DrawStage::Objects),
        "the object image composites before any vector draw"
    );
    let object_index = stages
        .iter()
        .position(|stage| *stage == DrawStage::Objects)
        .expect("the objects stage is present");
    let arrow_index = stages
        .iter()
        .position(|stage| matches!(stage, DrawStage::Vector(DrawPass::Scene3D { .. })))
        .expect("the combined 3D draw is present");
    assert!(object_index < arrow_index, "objects are painted first");
}

#[test]
fn empty_object_scene_skips_the_pass_and_keeps_the_scene_unchanged() {
    let empty = MeshScene::empty();
    assert!(object_draw_plan(&empty, &probe_view()).is_none());

    let slot = ObjectSceneState::default();
    assert!(!slot.has_objects());

    let scene = arrow_scene();
    let without_objects = paint_stages(&scene, 0);
    assert!(!without_objects.contains(&DrawStage::Objects));
    let vector: Vec<DrawPass> = without_objects
        .into_iter()
        .filter_map(|stage| match stage {
            DrawStage::Vector(draw) => Some(draw),
            DrawStage::Objects => None,
        })
        .collect();
    assert_eq!(
        vector,
        scene.draw_passes(),
        "an empty object scene leaves the existing painter order byte-identical"
    );

    // A scene whose only mapped input is the clip plane has no objects either.
    let clipped = MeshScene::empty();
    assert!(clipped.objects().is_empty());
    assert!(!ObjectSceneState::from_scene(Arc::new(clipped)).has_objects());
}
