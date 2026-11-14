//! Pure native output-frame geometry shared by both CLI backends.
use glam::{Mat4, Vec3};

use super::{CameraKind, CliError, CliOptions, Mode, validate_effective};
use crate::{camera::OrbitCamera, fly_camera::FlyCamera, renderer::SceneGPU};

/// Backend-independent camera, dimensions and straight-alpha background.
#[derive(Debug, Clone, Copy)]
pub struct OutputFrame {
    pub width: u32,
    pub height: u32,
    pub aspect: f32,
    pub view: Mat4,
    pub projection: Mat4,
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub target: Vec3,
    pub background_srgb: [u8; 4],
    pub background_linear: [f64; 4],
}

/// Renderer-ready scene paired with the single frame used by either exporter.
pub struct PreparedScene {
    pub scene: SceneGPU,
    pub frame: OutputFrame,
}

/// Resolve deterministic geometry before preparing a scene or selecting a backend.
pub fn output_frame(options: &CliOptions) -> Result<OutputFrame, CliError> {
    validate_effective(options)?;
    let aspect = options.width as f32 / options.height as f32;
    let camera = &options.camera;
    let mut orbit = OrbitCamera {
        target: options.domain.center(),
        ..OrbitCamera::default()
    };
    let (view, projection, eye, target) = match (options.mode, camera.kind) {
        (Mode::TwoD, CameraKind::Orbit) => {
            orbit.target = Vec3::from_array(camera.target);
            orbit.ortho_zoom = camera.ortho_zoom;
            let extent = options.domain.max - options.domain.min;
            let plane_height = (extent.x / aspect).max(extent.y) * 1.35;
            (
                orbit.front_view(),
                orbit.proj_ortho(aspect, plane_height),
                orbit.eye_position(true),
                orbit.target,
            )
        }
        (Mode::ThreeD, CameraKind::Orbit) => {
            orbit.target = Vec3::from_array(camera.target);
            orbit.yaw = camera.yaw;
            orbit.pitch = camera.pitch;
            orbit.distance = camera.distance;
            orbit.fov_y = camera.fov_y_degrees.to_radians();
            (
                orbit.view(),
                orbit.proj_perspective(aspect),
                orbit.position(),
                orbit.target,
            )
        }
        (Mode::ThreeD, CameraKind::Fly) => {
            let mut fly = FlyCamera::from_orbit(&orbit, FlyCamera::default().sensitivity);
            if camera.specified.position {
                fly.position = Vec3::from_array(camera.position);
            }
            if camera.specified.yaw {
                fly.yaw = camera.yaw;
            }
            if camera.specified.pitch {
                fly.pitch = camera.pitch;
            }
            (
                fly.view(),
                fly.proj_perspective(aspect),
                fly.position,
                fly.position + fly.look_dir(),
            )
        }
        (Mode::TwoD, CameraKind::Fly) => {
            return Err(CliError::Validation(
                "fly camera is inapplicable to 2D".into(),
            ));
        }
    };
    let view_proj = projection * view;
    for (name, matrix) in [
        ("view", view),
        ("projection", projection),
        ("view-projection", view_proj),
    ] {
        let determinant = matrix.determinant();
        if !matrix.is_finite()
            || !determinant.is_finite()
            || determinant == 0.0
            || !matrix.inverse().is_finite()
        {
            return Err(CliError::Validation(format!(
                "{name} transform must be finite and nonsingular"
            )));
        }
    }
    let [r, g, b, a] = options.background;
    Ok(OutputFrame {
        width: options.width,
        height: options.height,
        aspect,
        view,
        projection,
        view_proj,
        eye,
        target,
        background_srgb: options.background,
        background_linear: [
            srgb_to_linear(r),
            srgb_to_linear(g),
            srgb_to_linear(b),
            f64::from(a) / 255.0,
        ],
    })
}

fn srgb_to_linear(byte: u8) -> f64 {
    let channel = f64::from(byte) / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}
