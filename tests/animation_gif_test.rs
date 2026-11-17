use std::path::Path;

use gif::{ColorOutput, DecodeOptions, Repeat};
use tempfile::tempdir;
use vector_field_explorer::animation::{
    ANIMATION_MAX_BYTES, AnimationError, AnimationFormat, AnimationSpec, gif_delay_centiseconds,
    gif_sink,
};

const BANNER: [u8; 4] = [20, 220, 40, 255];

fn spec(loop_gif: bool) -> AnimationSpec {
    AnimationSpec {
        start_time: 0.0,
        duration: 0.3,
        fps: 10,
        width: 4,
        height: 4,
        loop_gif,
        format: AnimationFormat::Gif,
    }
}

fn frame(fill: [u8; 4]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(4 * 4 * 4);
    for index in 0..4 * 4 {
        let colour = if index < 4 { BANNER } else { fill };
        pixels.extend_from_slice(&colour);
    }
    pixels
}

fn temporary_siblings(directory: &Path) -> Vec<String> {
    directory
        .read_dir()
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect()
}

#[test]
fn gif_encoder_writes_every_frame_with_local_palettes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("animation.gif");
    let spec = spec(true);
    let fills = [[255, 0, 0, 255], [0, 128, 255, 255], [255, 255, 0, 255]];

    let mut sink = gif_sink(&spec, &path).expect("GIF sink opens");
    for fill in fills {
        sink.push_frame(&frame(fill), spec.width, spec.height)
            .expect("frame encodes");
    }
    assert!(sink.bytes_written() > 0);
    sink.finish().expect("GIF finishes");

    assert!(path.exists());
    assert!(temporary_siblings(dir.path()).is_empty());

    let file = std::fs::File::open(&path).unwrap();
    let mut decoder = DecodeOptions::new();
    decoder.set_color_output(ColorOutput::RGBA);
    let mut decoder = decoder.read_info(file).unwrap();
    assert_eq!((decoder.width(), decoder.height()), (4, 4));
    assert_eq!(decoder.repeat(), Repeat::Infinite);

    let mut decoded = Vec::new();
    while let Some(next) = decoder.read_next_frame().unwrap() {
        decoded.push(next.clone());
    }
    assert_eq!(decoded.len(), 3);
    let expected_delay = gif_delay_centiseconds(spec.fps);
    assert_eq!(expected_delay, 10);
    for (index, next) in decoded.iter().enumerate() {
        assert_eq!(next.delay, expected_delay);
        assert_eq!((next.width, next.height), (4, 4));
        assert!(
            next.palette.is_some(),
            "frame {index} must carry a local palette"
        );
        let pixels = next.buffer.as_ref();
        assert_eq!(pixels.len(), 4 * 4 * 4);
        assert_eq!(&pixels[0..4], &BANNER, "frame {index} lost the banner");
        assert!(
            pixels[16..20] != BANNER,
            "frame {index} must not collapse to a single palette"
        );
    }
}

#[test]
fn single_play_gif_omits_the_repeat_extension() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("single.gif");
    let spec = spec(false);

    let mut sink = gif_sink(&spec, &path).unwrap();
    sink.push_frame(&frame([255, 0, 0, 255]), spec.width, spec.height)
        .unwrap();
    sink.finish().unwrap();

    let bytes = std::fs::read(&path).unwrap();
    assert!(
        !bytes
            .windows(b"NETSCAPE2.0".len())
            .any(|window| window == b"NETSCAPE2.0"),
        "single-play GIF must not write the Netscape repeat extension"
    );

    let file = std::fs::File::open(&path).unwrap();
    let decoder = DecodeOptions::new().read_info(file).unwrap();
    assert_eq!(decoder.repeat(), Repeat::Finite(0));
}

#[test]
fn gif_frames_must_be_tightly_packed_rgba() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bad.gif");
    let spec = spec(true);

    let mut sink = gif_sink(&spec, &path).unwrap();
    let error = sink
        .push_frame(&[0u8; 3], spec.width, spec.height)
        .expect_err("an incomplete frame must fail");
    assert!(matches!(error, AnimationError::InvalidSpec(_)));
    drop(sink);

    assert!(!path.exists());
    assert!(temporary_siblings(dir.path()).is_empty());
}

#[test]
fn gif_byte_limit_is_a_constants_sanity_check() {
    // The cap is exercised mid-stream only by a writer-level limit; this keeps
    // the public bound pinned without allocating 512 MiB in the suite.
    assert_eq!(ANIMATION_MAX_BYTES, 512 * 1024 * 1024);
}
