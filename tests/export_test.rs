use std::fs;
use tempfile::NamedTempFile;
use vector_field_explorer::export::{delete_preset, save_preset, save_png};

#[test]
fn test_save_png_creates_file() {
    // create dummy frame: 10x10 white pixels BGRA
    let width = 10u32;
    let height = 10u32;
    let pixel = [255u8, 255, 255, 255];
    let frame: Vec<u8> = pixel.repeat((width * height) as usize);

    // temp file
    let tmp = NamedTempFile::new().expect("tempfile creation failed");
    let path = tmp.path().to_str().unwrap();

    // call save_png
    save_png(&frame, width, height, path).expect("save_png failed");

    // check file exists and not empty
    assert!(fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false));
}

#[test]
fn test_save_preset_creates_missing_dir() {
    // review fix: the export API must tolerate a not-yet-existing directory
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("nested/not-yet-created");
    let cam = vector_field_explorer::Camera::default();
    save_preset(&dir, "a", &cam).expect("save_preset must create the missing dir");
    assert!(dir.join("presets.json").exists());
}

#[test]
fn test_delete_preset_creates_missing_dir() {
    // symmetry with save_preset after the review fix
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("nested/not-yet-created");
    delete_preset(&dir, "a").expect("delete_preset must create the missing dir");
    assert!(dir.join("presets.json").exists());
}

#[test]
fn test_save_png_preserves_rgba_byte_order() {
    // one pure-red pixel in RGBA order; decoded PNG pixel must stay red
    let frame = vec![255u8, 0, 0, 255];
    let base = tempfile::tempdir().unwrap();
    let out = base.path().join("frame.png");
    save_png(&frame, 1, 1, out.to_str().unwrap()).expect("save_png failed");
    let img = image::open(&out).unwrap().to_rgba8();
    assert_eq!(img.get_pixel(0, 0), &image::Rgba([255, 0, 0, 255]));
}

/// The save API must be path-native (fix round 1, finding 3): a native dialog
/// answer is a `PathBuf` that need not be valid UTF-8, and round-tripping it
/// through `to_string_lossy()` would write a different file than the one the
/// user picked. A non-UTF-8 component proves the write follows the exact path.
#[cfg(unix)]
#[test]
fn test_save_png_accepts_a_non_utf8_path() {
    use std::os::unix::ffi::OsStrExt;

    let base = tempfile::tempdir().unwrap();
    let name = std::ffi::OsStr::from_bytes(b"shot-\xff\xfe.png");
    let out = base.path().join(name);
    assert!(out.to_str().is_none(), "the fixture path must not be valid UTF-8");

    let frame = vec![0u8, 0, 255, 255]; // pure blue, RGBA
    save_png(&frame, 1, 1, &out).expect("save_png must accept a &Path");

    assert!(out.exists(), "the PNG must land exactly where the path points");
    let img = image::open(&out).unwrap().to_rgba8();
    assert_eq!(img.get_pixel(0, 0), &image::Rgba([0, 0, 255, 255]));
}
