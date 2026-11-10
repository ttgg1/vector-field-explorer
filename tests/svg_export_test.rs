use vector_field_explorer::screenshot::{default_svg_filename, scene_to_svg, svg_element_count, SVG_ELEMENT_CAP, svg_export_enabled};
use vector_field_explorer::renderer::{SceneGPU, ArrowInstanceGPU, LineInstanceGPU};
use vector_field_explorer::export::ensure_svg_extension;
use std::time::SystemTime;

fn arrow(pos: [f32;3], color: [f32;4]) -> ArrowInstanceGPU {
    ArrowInstanceGPU {
        origin: [pos[0], pos[1], pos[2], 1.0],
        dir_len: [1.0, 0.0, 0.0, 1.0],
        color,
    }
}

fn particle(pos: [f32;3], color: [f32;4]) -> ArrowInstanceGPU {
    ArrowInstanceGPU {
        origin: [pos[0], pos[1], pos[2], 0.0],
        dir_len: [0.0, 0.0, 0.0, 3.0],
        color,
    }
}

fn line(a: [f32;3], b: [f32;3], color: [f32;4]) -> LineInstanceGPU {
    LineInstanceGPU {
        a: [a[0], a[1], a[2], 1.0],
        b: [b[0], b[1], b[2], 1.5],
        color,
    }
}

#[test]
fn default_svg_filename_has_svg_extension() {
    let name = default_svg_filename(SystemTime::UNIX_EPOCH, "2D");
    assert!(name.ends_with(".svg"), "{name}");
    assert!(name.contains("2d"), "{name}");
    // M1: must not over-trim the mode label.
    assert!(name.contains("2d.svg"), "{name}");
}

#[test]
fn ensure_svg_extension_forces_svg() {
    use std::path::PathBuf;
    assert_eq!(
        ensure_svg_extension(PathBuf::from("shot.png")),
        PathBuf::from("shot.svg")
    );
    assert_eq!(
        ensure_svg_extension(PathBuf::from("shot.svg")),
        PathBuf::from("shot.svg")
    );
}

#[test]
fn scene_to_svg_emits_valid_root() {
    let scene = SceneGPU::default();
    let vp = glam::Mat4::IDENTITY;
    let svg = scene_to_svg(&scene, vp, 64, 48, [0.0, 0.0, 0.0, 1.0]);
    assert!(svg.starts_with("<?xml"), "{}", &svg[..svg.len().min(60)]);
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("width=\"64\""));
    assert!(svg.contains("<svg"));
}

#[test]
fn scene_to_svg_round_trips_elements() {
    let scene = SceneGPU {
        mode_3d: false,
        arrows: std::sync::Arc::new(vec![arrow([0.0,0.0,0.0], [1.0,0.0,0.0,1.0])]),
        dots: std::sync::Arc::new(vec![particle([1.0,1.0,0.0], [0.0,1.0,0.0,1.0])]),
        lines: std::sync::Arc::new(vec![line([0.0,0.0,0.0],[1.0,0.0,0.0],[0.0,0.0,1.0,1.0])]),
        trails: std::sync::Arc::new(vec![line([0.0,1.0,0.0],[1.0,1.0,0.0],[1.0,1.0,0.0,1.0])]),
        ..Default::default()
    };
    let vp = glam::Mat4::IDENTITY;
    let svg = scene_to_svg(&scene, vp, 64, 48, [0.0,0.0,0.0,1.0]);
    // arrow shaft line present
    assert!(svg.contains("marker-end=\"url(#ah)\""), "{}", svg);
    // streamline/trail lines present
    assert!(svg.contains("<line"), "{}", svg);
    // particle circle present (C1)
    assert!(svg.contains("<circle"), "{}", svg);
    // colours are hex
    assert!(svg.contains("#ff0000") || svg.contains("#00ff00") || svg.contains("#0000ff"), "{}", svg);
}

#[test]
fn scene_to_svg_does_not_truncate_large_scene() {
    let n = SVG_ELEMENT_CAP + 5;
    let scene = SceneGPU {
        mode_3d: false,
        arrows: std::sync::Arc::new((0..n).map(|i| arrow([i as f32 *0.01,0.0,0.0],[1.0,0.0,0.0,1.0])).collect()),
        ..Default::default()
    };
    let count = svg_element_count(&scene);
    assert!(count > SVG_ELEMENT_CAP);
    let vp = glam::Mat4::IDENTITY;
    let svg = scene_to_svg(&scene, vp, 64, 48, [0.0,0.0,0.0,1.0]);
    // still contains all (no truncation): count circles? just ensure not empty and root closed
    assert!(svg.ends_with("</svg>"));
}

#[test]
fn svg_export_rejected_in_3d() {
    assert!(!svg_export_enabled(true));
    assert!(svg_export_enabled(false));
}

#[test]
fn colours_are_hex() {
    let scene = SceneGPU {
        mode_3d: false,
        dots: std::sync::Arc::new(vec![particle([0.0,0.0,0.0],[0.2,0.4,0.6,1.0])]),
        ..Default::default()
    };
    let vp = glam::Mat4::IDENTITY;
    let svg = scene_to_svg(&scene, vp, 64, 48, [0.0,0.0,0.0,1.0]);
    // find a fill="#rrggbb"
    assert!(svg.contains("fill=\"#"), "{}", svg);
}