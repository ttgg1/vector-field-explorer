use std::fs;
use tempfile::NamedTempFile;
use vector_field_explorer::export::save_png;

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
