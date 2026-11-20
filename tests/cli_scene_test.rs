use glam::{Mat4, Vec3};
use vector_field_explorer::app::compose_lines_with_frame;
use vector_field_explorer::color_mapping::{ColorSample, ColorStats};
use vector_field_explorer::{
    camera::OrbitCamera,
    cli::{
        CliError, CliFlags, CliOptions, Mode, OutputFrame, PreparedScene, output_frame,
        parse_config, prepare_scene, resolve_config,
    },
    fly_camera::FlyCamera,
    renderer::SceneGPU,
};
use vector_field_explorer::{
    particles::{self, AdvectSettings, ParticlePalette, Particles},
    renderer,
    viewmodel::State,
};

fn options(json: &str) -> CliOptions {
    resolve_config(
        Some(parse_config(json).unwrap()),
        CliFlags {
            output: Some("plot.png".into()),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn prepared_scene_samples_requested_endpoint_without_particles() {
    for (start, duration) in [(2.0, 0.05), (2.0, 0.0), (0.0, 0.05)] {
        let mut o = options(r#"{"density":4,"formula":"(t, x+t)"}"#);
        o.start_time_seconds = start;
        o.duration_seconds = duration;
        let prepared = prepare_scene(&o, output_frame(&o).unwrap()).unwrap();
        let mut state = vector_field_explorer::viewmodel::State::new(false);
        state.set_formula(o.formula.clone());
        state.set_density(o.density);
        state.time = (o.start_time_seconds + o.duration_seconds) as f32;
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(prepared.scene.arrows.as_slice()),
            bytemuck::cast_slice::<_, u8>(state.snapshot().arrows.as_slice())
        );
    }
}

fn same_gpu<T: bytemuck::Pod>(actual: &[T], expected: &[T]) {
    assert_eq!(
        bytemuck::cast_slice::<T, u8>(actual),
        bytemuck::cast_slice::<T, u8>(expected)
    );
}

fn reference_cloud(o: &CliOptions, count: usize, intervals: usize) -> Particles {
    let mut cloud = Particles::default();
    particles::spawn(&mut cloud, &o.domain, count, 3);
    let field = vector_field_explorer::field::FieldExpr::parse(&o.formula).unwrap();
    let start = o.start_time_seconds;
    let end = start + o.duration_seconds;
    let step = o.fixed_step_seconds;
    for i in 0..intervals {
        let a = start + i as f64 * step;
        let b = (start + (i + 1) as f64 * step).min(end);
        assert!(b > a);
        particles::advect(
            &mut cloud,
            &field,
            &o.domain,
            AdvectSettings {
                time: a as f32,
                dt: (b - a) as f32,
                trail_len: o.particle_appearance.trail_len,
                loop_on_stagnation: o.particle_appearance.loop_on_stagnation,
                stagnation_seconds: o.particle_appearance.stagnation_seconds,
            },
        );
    }
    cloud
}

#[test]
fn particles_follow_indexed_start_of_step_rk4_and_fractional_remainder() {
    for (start, duration, step, intervals) in [
        (2.0, 0.0, 0.125, 0),
        (2.0, 0.25, 0.125, 2),
        (2.0, 0.3, 0.125, 3),
        (0.0, 0.05, 1.0 / 60.0, 3),
    ] {
        let mut o = options(
            r#"{"mode":"3d","density":4,"particles":true,"particle_count":7,"formula":"(t, x+t, 0)"}"#,
        );
        o.start_time_seconds = start;
        o.duration_seconds = duration;
        o.fixed_step_seconds = step;
        let expected = reference_cloud(&o, 7, intervals);
        let p = prepare_scene(&o, output_frame(&o).unwrap()).unwrap();
        assert_eq!(p.scene.dots.len(), 7);
        for (dot, point) in p.scene.dots.iter().zip(expected.pos) {
            assert_eq!(&dot.origin[..3], point.as_ref());
        }
        if start == 0.0 {
            let a = (2.0 * step) as f32;
            let dt = (duration - 2.0 * step) as f32;
            assert_ne!(a + dt, duration as f32);
        }
    }
}

#[test]
fn decimal_multiple_stops_at_the_indexed_endpoint() {
    for (duration, intervals) in [(0.07, 7), (0.14, 14), (0.28, 28), (0.56, 56)] {
        let mut o = options(
            r#"{"mode":"3d","density":4,"particles":true,"particle_count":7,"formula":"(t, x+t, 0)","fixed_step_seconds":0.01}"#,
        );
        o.duration_seconds = duration;
        assert_eq!(intervals as f64 * o.fixed_step_seconds, duration);
        let expected = reference_cloud(&o, 7, intervals);
        let prepared = prepare_scene(&o, output_frame(&o).unwrap()).unwrap();
        assert_eq!(prepared.scene.dots.len(), 7);
        for (dot, point) in prepared.scene.dots.iter().zip(expected.pos) {
            assert_eq!(&dot.origin[..3], point.as_ref());
        }
        o.particles = false;
        assert!(prepare_scene(&o, output_frame(&o).unwrap()).is_ok());
    }
}

#[test]
fn schedule_rejects_nonprogress_and_excess_work_even_without_particles() {
    let base = options("{}");
    let frame = output_frame(&base).unwrap();
    for (start, duration, step, message) in [
        (3000.0, 0.00001, 0.01, "boundary"),
        (2048.0, 0.0002, 0.01, "stage"),
        (1.0, f64::from_bits(1), 0.01, "boundary"),
        (0.0, 3600.0, 0.0001, "100,000"),
        (
            0.0,
            f64::from_bits(0.07_f64.to_bits() + 1),
            0.01,
            "boundary",
        ),
        (
            0.0,
            f64::from_bits(10.0_f64.to_bits() + 1),
            0.0001,
            "100,000",
        ),
        (3600.0, 1.0, 0.01, "3600"),
        (0.0, f64::NAN, 0.01, "finite"),
        (0.0, 1.0, 0.0, "fixed_step"),
        (0.0, 1.0, f64::from_bits(1), "fixed_step"),
        (0.0, f64::from_bits(1), 0.01, "boundary"),
        (0.0, f64::INFINITY, 0.01, "finite"),
        (0.0, 1.0, f64::INFINITY, "finite"),
        (f64::MAX, f64::MAX, 0.01, "3600"),
    ] {
        let mut o = base.clone();
        o.start_time_seconds = start;
        o.duration_seconds = duration;
        o.fixed_step_seconds = step;
        match prepare_scene(&o, frame) {
            Err(CliError::Validation(text)) => {
                assert!(text.contains(message), "{text} expected {message}")
            }
            _ => panic!("accepted invalid schedule {start}/{duration}/{step}"),
        }
    }
}

#[test]
fn schedule_accepts_exactly_the_indexed_step_cap() {
    let o = options(r#"{"density":4,"duration_seconds":10,"fixed_step_seconds":0.0001}"#);
    assert!(prepare_scene(&o, output_frame(&o).unwrap()).is_ok());
}

#[test]
fn final_particle_colors_and_appearance_use_endpoint_and_shared_range() {
    for color in ["magnitude", "speed", "fixed"] {
        let o = options(&format!(
            r#"{{"mode":"3d","density":4,"formula":"(t,x+t,0)","particles":true,"particle_count":7,"start_time_seconds":2,"duration_seconds":0.25,"fixed_step_seconds":0.125,"particle_size":2,"colormap":"turbo","particle_appearance":{{"color_mode":"{color}","opacity":0.4,"fixed_color":[100,200,50],"trail_len":3,"trail_fade":0.3,"trail_width":1.2,"loop_on_stagnation":false,"stagnation_seconds":2}}}}"#
        ));
        let cloud = reference_cloud(&o, 7, 2);
        let mut state = State::new(true);
        state.set_formula(o.formula.clone());
        state.set_density(4);
        state.time = 2.25;
        let sampled = state.sample();
        let stats = ColorStats::from_samples(
            &sampled
                .iter()
                .map(|arrow| ColorSample {
                    magnitude: arrow.mag,
                    dir: arrow.dir,
                })
                .collect::<Vec<_>>(),
        );
        let range = state.color_range(&sampled);
        let speeds = particles::speed_range(&cloud);
        let palette = ParticlePalette {
            colormap: o.colormap,
            color_range: range,
            speed_range: speeds,
            mapping: None,
            stats: &stats,
        };
        let dots = particles::dot_instances_at(
            &cloud,
            &state.field,
            &o.domain,
            &palette,
            &o.particle_appearance,
            state.particle_radius(state.spacing()) * 2.0,
            state.time,
        );
        let trails = particles::trail_instances_at(
            &cloud,
            &state.field,
            &palette,
            &o.particle_appearance,
            state.spacing(),
            state.time,
        );
        let frame = output_frame(&o).unwrap();
        let p = prepare_scene(&o, frame).unwrap();
        same_gpu(p.scene.dots.as_slice(), &dots);
        same_gpu(
            p.scene.trails.as_slice(),
            &renderer::sorted_lines_back_to_front(&trails, &frame.view),
        );
    }
}

#[test]
fn scene_honors_explicit_arrow_geometry_color_plane_and_click_seed_options() {
    let o = options(
        r#"{"mode":"3d","density":4,"domain":{"min":[-3,-2,-1],"max":[2,3,4]},"formula":"(x+t,y,z)","start_time_seconds":1,"duration_seconds":0.25,"fixed_step_seconds":0.125,"arrow_scale":1.7,"arrow_opacity":0.6,"plane_dim_opacity":0.3,"arrow_length_mode":"magnitude","magnitude_scale":2,"proportional_exponent":2,"colormap":"mono","color_range":[0,10],"arrow_style_3d":"cylinder","arrow_style_2d":"streamline","arrow_params":{"head_length":0.4,"head_width":0.2,"shaft_radius":0.08,"sides":8},"streamlines":true,"streamline_seed_mode":"click","plane":{"enabled":true,"axis":"x","index":1},"references":{"grid_and_domain_box":false,"world_axes":false}}"#,
    );
    let mut state = State::new(true);
    state.set_formula(o.formula.clone());
    state.set_density(4);
    state.set_domain(o.domain.min, o.domain.max);
    state.time = 1.25;
    state.arrow_scale = 1.7;
    state.arrow_opacity = 0.6;
    state.plane_dim_opacity = 0.3;
    state.arrow_length_mode = vector_field_explorer::viewmodel::ArrowLengthMode::Magnitude;
    state.magnitude_scale = 2.0;
    state.proportional_exponent = 2.0;
    state.colormap = vector_field_explorer::colormap::Colormap::Mono;
    state.auto_fit = false;
    state.color_min = 0.0;
    state.color_max = 10.0;
    state.set_plane(o.plane);
    let expected = state.snapshot();
    let p = prepare_scene(&o, output_frame(&o).unwrap()).unwrap();
    same_gpu(p.scene.arrows.as_slice(), expected.arrows.as_slice());
    assert!(p.scene.lines.is_empty());
    assert_eq!(p.scene.arrow_style, o.arrow_style_3d);
    assert_eq!(p.scene.arrow_style_2d, o.arrow_style_2d);
    assert_eq!(p.scene.arrow_params, o.arrow_params);
}

#[test]
fn snapshot_flows_and_selected_references_appear_exactly_once() {
    for mode in ["2d", "3d"] {
        for (grid, axes) in [(false, false), (true, false), (false, true), (true, true)] {
            let o = options(&format!(
                r#"{{"mode":"{mode}","density":4,"formula":"(t,x+t)","streamlines":true,"start_time_seconds":2,"duration_seconds":0.05,"references":{{"grid_and_domain_box":{grid},"world_axes":{axes}}}}}"#
            ));
            let mut state = State::new(mode == "3d");
            state.set_formula(o.formula.clone());
            state.set_density(4);
            state.streamlines = true;
            state.time = (o.start_time_seconds + o.duration_seconds) as f32;
            let snapshot = state.snapshot();
            assert!(!snapshot.lines.is_empty());
            let expected = vector_field_explorer::app::compose_lines(
                snapshot.lines.as_ref(),
                &[],
                &state.domain,
                state.is_3d(),
                grid,
                axes,
            );
            let p = prepare_scene(&o, output_frame(&o).unwrap()).unwrap();
            same_gpu(p.scene.lines.as_slice(), &expected);
            same_gpu(p.scene.arrows.as_slice(), snapshot.arrows.as_slice());
        }
    }
}

#[test]
fn three_component_field_in_2d_uses_slice_local_references() {
    let o = options(
        r#"{"mode":"2d","density":4,"formula":"(x,y,z)","streamlines":true,"references":{"grid_and_domain_box":true,"world_axes":true}}"#,
    );
    let mut state = State::new(false);
    state.set_formula(o.formula.clone());
    state.set_density(o.density);
    state.set_plane(o.plane);
    state.streamlines = o.streamlines;
    let snapshot = state.snapshot();
    let expected = compose_lines_with_frame(
        snapshot.lines.as_ref(),
        &[],
        &state.scene_domain(),
        state.is_3d(),
        state.plane_frame(),
        o.references.grid_and_domain_box,
        o.references.world_axes,
    );
    let p = prepare_scene(&o, output_frame(&o).unwrap()).unwrap();
    same_gpu(p.scene.lines.as_slice(), &expected);
    assert!(
        p.scene
            .lines
            .iter()
            .all(|line| line.a[2].abs() < 1e-5 && line.b[2].abs() < 1e-5),
        "a 2D slice must use its flat local frame for references, not the world cube"
    );
}

#[test]
fn two_d_x_slice_frames_on_its_local_extents() {
    let o = options(
        r#"{"mode":"2d","width":1280,"height":800,"formula":"(x,y,z)","density":5,"domain":{"min":[-10,-2,-5],"max":[10,2,5]},"plane":{"axis":"x","index":2}}"#,
    );
    let mut state = State::new(false);
    state.set_formula(o.formula.clone());
    state.set_density(o.density);
    state.set_domain(o.domain.min, o.domain.max);
    state.set_plane(o.plane);
    let local = state.scene_domain().domain_size();
    let aspect = o.width as f32 / o.height as f32;
    let expected_height = (local.x / aspect).max(local.y) * 1.35;
    let expected_half = expected_height * 0.5 / o.camera.ortho_zoom;

    let frame = output_frame(&o).unwrap();
    assert!(
        (frame.projection.y_axis.y - 1.0 / expected_half).abs() < 1e-4,
        "the ortho height must frame the x slice's local z/y extents, got {}",
        frame.projection.y_axis.y,
    );

    // The old world-extent calculation used world x = 20, which for this
    // asymmetric box would double the visible height.
    let world = o.domain.max - o.domain.min;
    let world_height = (world.x / aspect).max(world.y) * 1.35;
    let world_half = world_height * 0.5 / o.camera.ortho_zoom;
    assert!(
        (frame.projection.y_axis.y - 1.0 / world_half).abs() > 1e-3,
        "the frame must not be sized by the world x box"
    );
}

#[test]
fn explicit_particle_seed_preserves_default_stream_and_accepts_zero() {
    let domain = vector_field_explorer::domain::Domain::default();
    let mut gui = Particles::default();
    let mut default_seed = Particles::with_seed(0x123456789ABCDEF0);
    let mut zero = Particles::with_seed(0);
    let mut other = Particles::with_seed(42);
    for cloud in [&mut gui, &mut default_seed, &mut zero, &mut other] {
        particles::spawn(cloud, &domain, 12, 3);
    }
    assert_eq!(gui.pos, default_seed.pos);
    assert_ne!(zero.pos, gui.pos);
    assert_ne!(other.pos, gui.pos);
    for seed in [0, 42] {
        let o = options(&format!(
            r#"{{"mode":"3d","density":4,"particles":true,"particle_count":12,"seed":{seed}}}"#
        ));
        let p = prepare_scene(&o, output_frame(&o).unwrap()).unwrap();
        let expected = if seed == 0 { &zero } else { &other };
        for (dot, point) in p.scene.dots.iter().zip(&expected.pos) {
            assert_eq!(&dot.origin[..3], point.as_ref());
        }
        assert_eq!(p.scene.dots.len(), 12);
    }
}

#[test]
fn eligibility_combined_tags_and_stable_camera_order() {
    for (mode, formula, arrows, expected_dots) in [
        ("2d", "(x,y)", true, 0),
        ("3d", "(x,y,z)", false, 9),
        ("3d", "(x,y)", true, 0),
        ("3d", "(x,y,z)", true, 9),
    ] {
        let o = options(&format!(
            r#"{{"mode":"{mode}","formula":"{formula}","density":4,"particles":true,"particle_count":9,"arrows":{arrows}}}"#
        ));
        let mut frame = output_frame(&o).unwrap();
        // Equal-depth view exercises stability including arrow/dot ties.
        frame.view = Mat4::from_cols(
            Vec3::X.extend(0.0),
            Vec3::Y.extend(0.0),
            glam::Vec4::ZERO,
            glam::Vec4::W,
        );
        let p = prepare_scene(&o, frame).unwrap();
        assert_eq!(p.scene.dots.len(), expected_dots);
        if mode == "2d" {
            assert!(p.scene.combined.is_empty());
            assert!(p.scene.trails.is_empty());
        } else {
            let expected = renderer::combined_instances(&p.scene.arrows, &p.scene.dots);
            assert!(!expected.is_empty());
            same_gpu(p.scene.combined.as_slice(), &expected);
            assert!(
                p.scene.combined[..p.scene.arrows.len()]
                    .iter()
                    .all(|a| a.origin[3] == renderer::ARROW_KIND)
            );
            assert!(
                p.scene.combined[p.scene.arrows.len()..]
                    .iter()
                    .all(|a| a.origin[3] == renderer::PARTICLE_KIND)
            );
            let actual_view = output_frame(&o).unwrap();
            let sorted = prepare_scene(&o, actual_view).unwrap();
            let mut expected = expected;
            renderer::sort_scene_back_to_front(&mut expected, &actual_view.view);
            same_gpu(sorted.scene.combined.as_slice(), &expected);
        }
    }
}

#[test]
fn trails_keep_chronology_at_equal_depth_and_sort_far_to_near() {
    let o = options(
        r#"{"mode":"3d","density":4,"formula":"(0.2,0.1,0.3)","particles":true,"particle_count":7,"duration_seconds":0.375,"fixed_step_seconds":0.125,"particle_appearance":{"trail_len":4}}"#,
    );
    let cloud = reference_cloud(&o, 7, 3);
    let mut state = State::new(true);
    state.set_formula(o.formula.clone());
    state.set_density(4);
    let sampled = state.sample();
    let stats = ColorStats::from_samples(
        &sampled
            .iter()
            .map(|arrow| ColorSample {
                magnitude: arrow.mag,
                dir: arrow.dir,
            })
            .collect::<Vec<_>>(),
    );
    let palette = ParticlePalette {
        colormap: o.colormap,
        color_range: state.color_range(&sampled),
        speed_range: particles::speed_range(&cloud),
        mapping: None,
        stats: &stats,
    };
    let field = &state.field;
    let expected = particles::trail_instances_at(
        &cloud,
        field,
        &palette,
        &o.particle_appearance,
        state.spacing(),
        o.duration_seconds as f32,
    );
    assert_eq!(expected.len(), 21);
    let mut frame = output_frame(&o).unwrap();
    frame.view = Mat4::ZERO;
    let p = prepare_scene(&o, frame).unwrap();
    same_gpu(p.scene.trails.as_slice(), &expected);
    let frame = output_frame(&o).unwrap();
    let p = prepare_scene(&o, frame).unwrap();
    same_gpu(
        p.scene.trails.as_slice(),
        &renderer::sorted_lines_back_to_front(&expected, &frame.view),
    );
}

#[test]
fn full_particle_budget_accepts_requested_32_and_stores_25_points() {
    let o = options(
        r#"{"mode":"3d","density":4,"formula":"(0.01,0.01,0.01)","particles":true,"particle_count":4000,"duration_seconds":0.25,"fixed_step_seconds":0.01,"particle_appearance":{"trail_len":32}}"#,
    );
    assert_eq!(particles::effective_trail_len(4000, 32), 25);
    let p = prepare_scene(&o, output_frame(&o).unwrap()).unwrap();
    assert_eq!(p.scene.dots.len(), 4000);
    assert_eq!(p.scene.trails.len(), 4000 * 24);
}

fn close_vec(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).length() < 1e-4,
        "{actual:?} != {expected:?}"
    );
}

fn close_mat(actual: Mat4, expected: Mat4) {
    assert!(
        actual.abs_diff_eq(expected, 1e-5),
        "{actual:?} != {expected:?}"
    );
}

fn valid(frame: &OutputFrame) {
    for m in [frame.view, frame.projection, frame.view_proj] {
        assert!(m.is_finite());
        assert!(m.determinant().is_finite());
        assert_ne!(m.determinant(), 0.0);
        assert!(m.inverse().is_finite());
    }
    close_mat(frame.view_proj, frame.projection * frame.view);
}

#[test]
fn front_frame_fits_off_origin_mid_z_corners_at_every_aspect() {
    for (width, height) in [(1280, 800), (800, 1280), (1, 4096), (4096, 1), (1, 1)] {
        let mut o = options(r#"{"domain":{"min":[4,-9,40],"max":[10,-5,42]}}"#);
        o.width = width;
        o.height = height;
        let f = output_frame(&o).unwrap();
        valid(&f);
        assert_eq!((f.width, f.height), (width, height));
        assert_eq!(f.aspect, width as f32 / height as f32);
        close_vec(f.target, Vec3::new(7.0, -7.0, 41.0));
        close_vec(f.eye, f.target + Vec3::Z * OrbitCamera::default().distance);
        let mut furthest = 0.0_f32;
        for x in [4.0, 10.0] {
            for y in [-9.0, -5.0] {
                let p = f.view_proj.project_point3(Vec3::new(x, y, 41.0));
                assert!(p.x.abs() <= 1.0 / 1.35 + 1e-5);
                assert!(p.y.abs() <= 1.0 / 1.35 + 1e-5);
                assert!((0.0..=1.0).contains(&p.z));
                furthest = furthest.max(p.x.abs()).max(p.y.abs());
            }
        }
        assert!((furthest - 1.0 / 1.35).abs() < 1e-5);
    }
}

#[test]
fn orbit_uses_recentered_app_pose_not_domain_fit() {
    let o = options(r#"{"mode":"3d","domain":{"min":[-90,-80,40],"max":[70,60,42]}}"#);
    let f = output_frame(&o).unwrap();
    let orbit = OrbitCamera {
        target: o.domain.center(),
        ..Default::default()
    };
    valid(&f);
    close_vec(f.target, orbit.target);
    close_vec(f.eye, orbit.position());
    close_mat(f.view, orbit.view());
    close_mat(f.projection, orbit.proj_perspective(f.aspect));
    assert!(((f.eye - f.target).length() - 3.2).abs() < 1e-4);
}

#[test]
fn orbit_components_merge_individually_and_override_defaults() {
    let config = parse_config(
        r#"{"mode":"3d","camera":{"target":[2,3,4],"yaw":0.3,"distance":7,"fov_y_degrees":73}}"#,
    )
    .unwrap();
    let o = resolve_config(
        Some(config),
        CliFlags {
            output: Some("plot.png".into()),
            camera_yaw: Some(-0.6),
            ..Default::default()
        },
    )
    .unwrap();
    let f = output_frame(&o).unwrap();
    let orbit = OrbitCamera {
        target: Vec3::new(2.0, 3.0, 4.0),
        yaw: -0.6,
        distance: 7.0,
        fov_y: 73.0_f32.to_radians(),
        ..Default::default()
    };
    close_vec(f.eye, orbit.position());
    close_mat(f.view, orbit.view());
    close_mat(f.projection, orbit.proj_perspective(f.aspect));
}

#[test]
fn fly_derives_pose_then_overrides_only_explicit_components() {
    for camera in [
        r#"{"kind":"fly"}"#,
        r#"{"kind":"fly","position":[3,4,5]}"#,
        r#"{"kind":"fly","yaw":0.2}"#,
        r#"{"kind":"fly","pitch":-0.4}"#,
    ] {
        let o = options(&format!(
            r#"{{"mode":"3d","domain":{{"min":[4,-9,40],"max":[10,-5,42]}},"camera":{camera}}}"#
        ));
        let orbit = OrbitCamera {
            target: o.domain.center(),
            ..Default::default()
        };
        let mut fly = FlyCamera::from_orbit(&orbit, FlyCamera::default().sensitivity);
        if o.camera.specified.position {
            fly.position = Vec3::from_array(o.camera.position);
        }
        if o.camera.specified.yaw {
            fly.yaw = o.camera.yaw;
        }
        if o.camera.specified.pitch {
            fly.pitch = o.camera.pitch;
        }
        let f = output_frame(&o).unwrap();
        valid(&f);
        close_vec(f.eye, fly.position);
        close_vec(f.target, fly.position + fly.look_dir());
        close_mat(f.view, fly.view());
        close_mat(f.projection, fly.proj_perspective(f.aspect));
    }
}

#[test]
fn front_view_uses_target_and_zoom_but_not_implicit_orbit_pose() {
    let mut o = options(r#"{"camera":{"target":[3,4,41],"ortho_zoom":2}}"#);
    let f = output_frame(&o).unwrap();
    let orbit = OrbitCamera {
        target: Vec3::new(3.0, 4.0, 41.0),
        ortho_zoom: 2.0,
        ..Default::default()
    };
    close_mat(f.view, orbit.front_view());
    close_mat(
        f.projection,
        orbit.proj_ortho(f.aspect, (2.0_f32 / f.aspect).max(2.0) * 1.35),
    );
    o.camera.yaw = -0.4;
    o.camera.pitch = 0.5;
    o.camera.distance = 19.0;
    o.camera.fov_y_degrees = 90.0;
    let same = output_frame(&o).unwrap();
    close_mat(same.view_proj, f.view_proj);
    close_vec(same.eye, f.eye);

    o.mode = Mode::ThreeD;
    o.camera.specified.ortho_zoom = false;
    let a = output_frame(&o).unwrap();
    o.camera.ortho_zoom = 30.0;
    close_mat(output_frame(&o).unwrap().view_proj, a.view_proj);
}

#[test]
fn frame_rejects_explicit_inapplicable_camera_members() {
    for (mode, kind, field) in [
        (Mode::TwoD, "orbit", "position"),
        (Mode::TwoD, "orbit", "yaw"),
        (Mode::TwoD, "orbit", "pitch"),
        (Mode::TwoD, "orbit", "distance"),
        (Mode::TwoD, "orbit", "fov_y_degrees"),
        (Mode::ThreeD, "orbit", "position"),
        (Mode::ThreeD, "orbit", "ortho_zoom"),
        (Mode::ThreeD, "fly", "target"),
        (Mode::ThreeD, "fly", "distance"),
        (Mode::ThreeD, "fly", "fov_y_degrees"),
        (Mode::ThreeD, "fly", "ortho_zoom"),
    ] {
        let mut o = options(&format!(r#"{{"mode":"3d","camera":{{"kind":"{kind}"}}}}"#));
        o.mode = mode;
        o.plane.index = 0;
        match field {
            "position" => o.camera.specified.position = true,
            "target" => o.camera.specified.target = true,
            "yaw" => o.camera.specified.yaw = true,
            "pitch" => o.camera.specified.pitch = true,
            "distance" => o.camera.specified.distance = true,
            "fov_y_degrees" => o.camera.specified.fov_y_degrees = true,
            "ortho_zoom" => o.camera.specified.ortho_zoom = true,
            _ => unreachable!(),
        }
        assert!(
            matches!(output_frame(&o), Err(CliError::Validation(message)) if message.contains("inapplicable")),
            "{mode:?}/{kind}/{field}"
        );
    }
    let mut o = options(r#"{"mode":"3d","camera":{"kind":"fly"}}"#);
    o.mode = Mode::TwoD;
    o.plane.index = 0;
    assert!(matches!(output_frame(&o), Err(CliError::Validation(_))));
}

#[test]
fn frame_rejects_degenerate_final_transforms_even_with_intrinsically_valid_inputs() {
    // Orbit pitch zero is within the schema but puts the eye on the up axis.
    for json in [
        r#"{"mode":"3d","camera":{"pitch":0}}"#,
        r#"{"mode":"3d","camera":{"target":[1e30,1e30,1e30]}}"#,
        r#"{"mode":"3d","camera":{"kind":"fly","position":[1e30,1e30,1e30]}}"#,
        r#"{"mode":"3d","camera":{"fov_y_degrees":1e-40}}"#,
    ] {
        let o = options(json);
        assert!(
            matches!(output_frame(&o), Err(CliError::Validation(message)) if message.contains("transform")),
            "{json}"
        );
    }
}

#[test]
fn background_decodes_srgb_without_premultiplying_alpha_and_carrier_keeps_frame() {
    let default = output_frame(&options("{}")).unwrap();
    assert_eq!(default.background_srgb, [27, 27, 27, 255]);
    assert!((default.background_linear[0] - 0.010960094006488246).abs() < 1e-12);
    assert_eq!(default.background_linear[3], 1.0);
    let f = output_frame(&options(r#"{"background":[10,128,255,64]}"#)).unwrap();
    assert_eq!(f.background_srgb, [10, 128, 255, 64]);
    assert!((f.background_linear[0] - 10.0 / 255.0 / 12.92).abs() < 1e-12);
    assert!((f.background_linear[1] - 0.21586050011389926).abs() < 1e-12);
    assert_eq!(f.background_linear[2], 1.0);
    assert_eq!(f.background_linear[3], 64.0 / 255.0);
    for (linear, byte) in f.background_linear[..3].iter().zip(f.background_srgb) {
        let encoded = if *linear <= 0.0031308 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        assert_eq!((encoded * 255.0).round() as u8, byte);
    }
    let prepared = PreparedScene {
        scene: SceneGPU::default(),
        frame: f,
    };
    assert_eq!(prepared.frame.background_srgb, [10, 128, 255, 64]);
    assert!(prepared.scene.is_empty());
}
