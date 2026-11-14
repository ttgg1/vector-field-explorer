//! Pure native output-frame geometry and final scene shared by both CLI backends.
use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::{CameraKind, CliError, CliOptions, Mode, validate_effective};
use crate::{
    app::compose_lines,
    camera::OrbitCamera,
    fly_camera::FlyCamera,
    particles::{self, AdvectSettings, Particles},
    renderer::{SceneGPU, combined_instances, sort_lines_back_to_front, sort_scene_back_to_front},
    viewmodel::State,
};

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

/// Prepare the renderer-ready final scene for either backend.
pub fn prepare_scene(options: &CliOptions, frame: OutputFrame) -> Result<PreparedScene, CliError> {
    validate_effective(options)?;
    // Validate every interval before allocating or advancing a particle cloud.
    let (steps, endpoint) = simulation_schedule(options)?;
    let mut state = State::new(options.mode == Mode::ThreeD);
    state.set_formula(options.formula.clone());
    state.set_density(options.density);
    state.set_domain(options.domain.min, options.domain.max);
    state.arrows = options.arrows;
    state.streamlines = options.streamlines;
    state.particles = options.particles;
    state.arrow_scale = options.arrow_scale;
    state.arrow_opacity = options.arrow_opacity;
    state.plane_dim_opacity = options.plane_dim_opacity;
    state.arrow_length_mode = options.arrow_length_mode;
    state.magnitude_scale = options.magnitude_scale;
    state.proportional_exponent = options.proportional_exponent;
    state.colormap = options.colormap;
    state.auto_fit = options.color_range.is_none();
    if let Some([min, max]) = options.color_range {
        state.color_min = min;
        state.color_max = max;
    }
    state.arrow_style = options.arrow_style_3d;
    state.arrow_style_2d = options.arrow_style_2d;
    state.arrow_params = options.arrow_params;
    state.seed_mode = options.streamline_seed_mode;
    state.set_plane(options.plane);
    state.particle_count = options.particle_count;
    state.particle_size = options.particle_size;
    state.particle_appearance = options.particle_appearance;
    state.show_grid = options.references.grid_and_domain_box;
    state.show_coordinate_system = options.references.world_axes;
    state.time = options.start_time_seconds as f32;

    let cloud = state.particle_target().map(|target| {
        let mut cloud = Particles::with_seed(options.seed);
        particles::spawn(&mut cloud, &state.domain, target, 3);
        for &(time, dt) in &steps {
            particles::advect(
                &mut cloud,
                &state.field,
                &state.domain,
                AdvectSettings {
                    time,
                    dt,
                    trail_len: state.particle_appearance.trail_len,
                    loop_on_stagnation: state.particle_appearance.loop_on_stagnation,
                    stagnation_seconds: state.particle_appearance.stagnation_seconds,
                },
            );
        }
        cloud
    });
    state.time = endpoint;
    // Snapshot owns flow tracing. Compose references onto those lines exactly once.
    let mut scene = state.snapshot();
    scene.lines = Arc::new(compose_lines(
        scene.lines.as_ref(),
        &[],
        &state.domain,
        state.is_3d(),
        state.show_grid,
        state.show_coordinate_system,
    ));
    if let Some(cloud) = cloud {
        let range = state.color_range(&state.sample());
        let speeds = particles::speed_range(&cloud);
        let spacing = state.spacing();
        scene.dots = Arc::new(particles::dot_instances_at(
            &cloud,
            &state.field,
            &state.domain,
            state.colormap,
            range,
            &state.particle_appearance,
            speeds,
            state.particle_radius(spacing),
            state.time,
        ));
        let mut trails = particles::trail_instances_at(
            &cloud,
            &state.field,
            state.colormap,
            range,
            &state.particle_appearance,
            speeds,
            spacing,
            state.time,
        );
        sort_lines_back_to_front(&mut trails, &frame.view);
        scene.trails = Arc::new(trails);
    }
    if state.mode_3d {
        let mut combined = combined_instances(&scene.arrows, &scene.dots);
        sort_scene_back_to_front(&mut combined, &frame.view);
        scene.combined = Arc::new(combined);
    } else {
        sort_scene_back_to_front(Arc::make_mut(&mut scene.arrows).as_mut_slice(), &frame.view);
    }
    Ok(PreparedScene { scene, frame })
}

/// Indexed f64 schedule with independently rounded f32 RK4 arguments.
/// Keep this private: callers exercise preflight through the complete scene owner.
fn simulation_schedule(options: &CliOptions) -> Result<(Vec<(f32, f32)>, f32), CliError> {
    let invalid = |reason: &str| CliError::Validation(format!("simulation schedule: {reason}"));
    let start = options.start_time_seconds;
    let duration = options.duration_seconds;
    let step = options.fixed_step_seconds;
    let end = start + duration;
    if !end.is_finite() || !(end as f32).is_finite() {
        return Err(invalid("nonfinite endpoint"));
    }
    if duration > 0.0 && end <= start {
        return Err(invalid("f64 boundary does not advance"));
    }
    let count = (duration / step).ceil();
    if !count.is_finite() || count > 100_000.0 {
        return Err(invalid("step count exceeds 100,000"));
    }
    // The bounded count is now safe to convert to an integer.
    let count = count as u32;
    let mut intervals = Vec::with_capacity(count as usize);
    for i in 0..count {
        let next = i
            .checked_add(1)
            .ok_or_else(|| invalid("step count overflow"))?;
        let offset = f64::from(i) * step;
        let next_offset = f64::from(next) * step;
        let a = start + offset;
        let b = (start + next_offset).min(end);
        if !offset.is_finite()
            || !next_offset.is_finite()
            || !a.is_finite()
            || !b.is_finite()
            || b <= a
        {
            return Err(invalid("f64 boundary overflow or stagnation"));
        }
        let time = a as f32;
        let boundary = b as f32;
        if !time.is_finite() || !boundary.is_finite() || boundary <= time {
            return Err(invalid("f32 boundary stagnation or overflow"));
        }
        let dt = (b - a) as f32;
        let midpoint = time + dt * 0.5;
        let rk4_end = time + dt;
        if !dt.is_finite()
            || dt <= 0.0
            || !midpoint.is_finite()
            || !rk4_end.is_finite()
            || !(time < midpoint && midpoint < rk4_end)
        {
            return Err(invalid("RK4 stage times must advance and remain finite"));
        }
        // Deliberately do not compare rk4_end with the separately rounded boundary.
        intervals.push((time, dt));
    }
    Ok((intervals, end as f32))
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
