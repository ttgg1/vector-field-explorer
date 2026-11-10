use vector_field_explorer::screenshot::{default_svg_filename, scene_to_svg};
use vector_field_explorer::renderer::SceneGPU;
use vector_field_explorer::export::ensure_svg_extension;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn default_svg_filename_has_svg_extension() {
    let name = default_svg_filename(SystemTime::UNIX_EPOCH, "2D");
    assert!(name.ends_with(".svg"), "{name}");
    assert!(name.contains("2d"), "{name}");
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
    assert!(svg.starts_with("<svg"), "{}", &svg[..svg.len().min(60)]);
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("width=\"64\""));
}
