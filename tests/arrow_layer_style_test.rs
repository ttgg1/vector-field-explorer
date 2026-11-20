//! Issue #84 slice A2a: Arrow, Flow and Reference layer styles in the drawables.
//!
//! The neutral [`LayerStyles`] default is the regression contract: every
//! emitted instance must stay byte-identical to the pre-A2a build. A non-neutral
//! Arrows record scales alpha and girth, never the mapped RGB or the head
//! length/sides, and the stored params stay untouched.

use glam::Vec3;
use vector_field_explorer::app::{compose_lines_with_frame, compose_lines_with_frame_style};
use vector_field_explorer::color_mapping::ColorStats;
use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::layer_style::{LayerId, LayerOverride, LayerStyles, PhysicsFamily};
use vector_field_explorer::material::MaterialCase;
use vector_field_explorer::maxwell::MaxwellCase;
use vector_field_explorer::particles::{
    AdvectSettings, FlowLayerStyle, ParticleAppearance, ParticlePalette, Particles, advect,
    dot_instances_at, dot_instances_styled_at, spawn, speed_range, trail_instances_at,
    trail_instances_styled_at,
};
use vector_field_explorer::plane::on_plane;
use vector_field_explorer::renderer::{ArrowInstanceGPU, LineInstanceGPU};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::viewmodel::{STREAMLINE_WIDTH, State};

fn slot(layer: LayerId) -> usize {
    LayerId::ALL
        .iter()
        .position(|candidate| *candidate == layer)
        .expect("every layer is in LayerId::ALL")
}

fn set_layer(state: &mut State, layer: LayerId, opacity: Option<f32>, width: Option<f32>) {
    let mut styles = *state.layer_styles();
    styles.layers[slot(layer)] = LayerOverride {
        opacity,
        width,
        color: None,
    };
    state
        .commit_layer_styles(styles)
        .expect("the test style is valid");
}

fn same_arrows(a: &[ArrowInstanceGPU], b: &[ArrowInstanceGPU]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.origin == b.origin && a.dir_len == b.dir_len && a.color == b.color)
}

fn same_lines(a: &[LineInstanceGPU], b: &[LineInstanceGPU]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.a == b.a && a.b == b.b && a.color == b.color)
}

/// A cloud with a real trail history, built through the advect path.
fn cloud_with_trails(field: &FieldExpr, domain: &Domain) -> Particles {
    let mut cloud = Particles::default();
    spawn(&mut cloud, domain, 6, 3);
    for _ in 0..5 {
        advect(
            &mut cloud,
            field,
            domain,
            AdvectSettings {
                dt: 0.01,
                trail_len: 4,
                ..AdvectSettings::default()
            },
        );
    }
    cloud
}

#[test]
fn neutral_layer_styles_render_byte_identical() {
    let mut state = State::new(true);
    assert!(state.set_formula("(-y, x, z)".to_string()));
    assert!(state.set_streamlines(true));
    let sampled = state.sample();
    let arrows_before = state.arrow_instances(&sampled);
    let flow_before = state.streamlines_snapshot(&sampled);
    let range = state.color_range(&sampled);
    let clicks_before = state.click_streamlines_snapshot(range, &[Vec3::new(0.2, 0.1, 0.0)]);
    let reference_before = compose_lines_with_frame(
        &[],
        &[],
        &state.scene_domain(),
        state.is_3d(),
        state.plane_frame(),
        state.show_grid,
        state.show_coordinate_system,
    );
    let params_before = state.effective_arrow_params();

    // Committing the neutral record must be a validated no-op and every
    // drawable must stay exactly where the historical defaults put it.
    state
        .commit_layer_styles(LayerStyles::default())
        .expect("neutral styles validate");

    assert_eq!(params_before, state.arrow_params, "parameters stay stored");
    assert!(same_arrows(
        &arrows_before,
        &state.arrow_instances(&sampled)
    ));
    let flow_after = state.streamlines_snapshot(&sampled);
    assert!(same_lines(&flow_before, &flow_after));
    assert!(same_lines(
        &clicks_before,
        &state.click_streamlines_snapshot(range, &[Vec3::new(0.2, 0.1, 0.0)])
    ));
    let reference_after = compose_lines_with_frame_style(
        &[],
        &[],
        &state.scene_domain(),
        state.is_3d(),
        state.plane_frame(),
        state.show_grid,
        state.show_coordinate_system,
        &state.reference_viewport_style(),
    );
    assert!(same_lines(&reference_before, &reference_after));

    // Pin the historical defaults the neutral record must reproduce.
    for instance in &arrows_before {
        assert_eq!(instance.color[3], 1.0, "neutral arrows stay opaque");
    }
    for line in &flow_before {
        assert_eq!(line.b[3], STREAMLINE_WIDTH, "neutral width is the constant");
        assert_eq!(line.color[3], 1.0, "neutral streamlines stay opaque");
    }

    let field = FieldExpr::parse("(1, 0)").expect("pump field parses");
    let domain = Domain::default();
    let cloud = cloud_with_trails(&field, &domain);
    let stats = ColorStats::from_samples(&[]);
    let palette = ParticlePalette {
        colormap: state.colormap,
        color_range: (0.0, 1.0),
        speed_range: speed_range(&cloud),
        mapping: None,
        stats: &stats,
    };
    let appearance = ParticleAppearance {
        trail_len: 4,
        ..ParticleAppearance::default()
    };
    let radius = state.particle_radius(state.spacing());
    let plain_dots = dot_instances_at(&cloud, &field, &domain, &palette, &appearance, radius, 0.0);
    let neutral_dots = dot_instances_styled_at(
        &cloud,
        &field,
        &domain,
        &palette,
        &appearance,
        radius,
        0.0,
        FlowLayerStyle::default(),
    );
    assert!(same_arrows(&plain_dots, &neutral_dots));
    let plain_trails = trail_instances_at(&cloud, &field, &palette, &appearance, 1.0, 0.0);
    let neutral_trails = trail_instances_styled_at(
        &cloud,
        &field,
        &palette,
        &appearance,
        1.0,
        0.0,
        FlowLayerStyle::default(),
    );
    assert!(same_lines(&plain_trails, &neutral_trails));
    assert!(!plain_trails.is_empty(), "the fixture really has trails");
}

#[test]
fn arrow_opacity_scales_every_instance_alpha() {
    let mut state = State::new(true);
    assert!(state.set_plane_enabled(true));
    assert!(
        state.plane_active(),
        "the default 3D cube has plane dimming"
    );
    assert!(state.set_arrow_opacity(0.8));
    assert!(state.set_plane_dim_opacity(0.5));

    let sampled = state.sample();
    let baseline = state.arrow_instances(&sampled);
    assert!(
        baseline.iter().any(|i| i.color[3] < 0.8),
        "off-plane samples exist"
    );
    assert!(
        baseline.iter().any(|i| i.color[3] == 0.8),
        "on-plane samples exist"
    );

    set_layer(&mut state, LayerId::Arrows, Some(0.5), None);
    let scaled = state.arrow_instances(&sampled);
    let grid = state.grid();
    let (mut on_plane_seen, mut off_plane_seen) = (0, 0);
    for (arrow, (before, after)) in sampled.iter().zip(baseline.iter().zip(&scaled)) {
        assert_eq!(
            [after.color[0], after.color[1], after.color[2]],
            [before.color[0], before.color[1], before.color[2]],
            "layer opacity never changes RGB"
        );
        if on_plane(arrow.position, &state.domain, grid, &state.plane) {
            on_plane_seen += 1;
            assert_eq!(after.color[3], 0.8 * 0.5);
        } else {
            off_plane_seen += 1;
            assert_eq!(after.color[3], 0.8 * 0.5 * 0.5);
        }
    }
    assert!(on_plane_seen > 0 && off_plane_seen > 0);
}

#[test]
fn arrow_width_scales_girth_not_length() {
    let mut state = State::new(true);
    let stored = state.arrow_params;
    set_layer(&mut state, LayerId::Arrows, None, Some(0.5));

    let effective = state.effective_arrow_params();
    assert_eq!(effective.head_width, stored.head_width * 0.5);
    assert_eq!(effective.shaft_radius, stored.shaft_radius * 0.5);
    assert_eq!(effective.head_length, stored.head_length);
    assert_eq!(effective.sides, stored.sides);
    assert_eq!(state.arrow_params, stored, "the stored record never moves");

    let mut wide = State::new(true);
    set_layer(&mut wide, LayerId::Arrows, None, Some(2.0));
    assert_eq!(
        wide.effective_arrow_params().head_width,
        stored.head_width * 2.0
    );
    assert_eq!(
        wide.effective_arrow_params().shaft_radius,
        stored.shaft_radius * 2.0
    );
}

// ---- Issue #84 slice B2: Teaching-scene layer styling ----

fn family_slot(family: PhysicsFamily) -> usize {
    PhysicsFamily::ALL
        .iter()
        .position(|candidate| *candidate == family)
        .expect("every family is in PhysicsFamily::ALL")
}

fn set_layer_full(
    state: &mut State,
    layer: LayerId,
    opacity: Option<f32>,
    width: Option<f32>,
    color: Option<Rgb>,
) {
    let mut styles = *state.layer_styles();
    styles.layers[slot(layer)] = LayerOverride {
        opacity,
        width,
        color,
    };
    state
        .commit_layer_styles(styles)
        .expect("the test style is valid");
}

fn set_family_full(
    state: &mut State,
    family: PhysicsFamily,
    opacity: Option<f32>,
    width: Option<f32>,
    color: Option<Rgb>,
) {
    let mut styles = *state.layer_styles();
    styles.families[family_slot(family)] = LayerOverride {
        opacity,
        width,
        color,
    };
    state
        .commit_layer_styles(styles)
        .expect("the test style is valid");
}

fn material_state(case: MaterialCase) -> State {
    let mut state = State::new(true);
    assert!(state.set_field_mode(FieldMode::Materials));
    let mut scene = state.material.clone();
    scene.case = case;
    state
        .set_material_scene(scene)
        .expect("the teaching case validates");
    state.set_plane_enabled(false);
    state.refresh_material_sampling();
    state
}

fn maxwell_state(case: MaxwellCase) -> State {
    let mut state = State::new(true);
    assert!(state.set_field_mode(FieldMode::Maxwell));
    state.set_mode(true);
    let mut scene = state.maxwell.clone();
    scene.case = case;
    let _ = state.set_maxwell_scene(scene);
    state.refresh_maxwell_sampling();
    state
}

fn assert_scene_bytes_equal(
    a: &vector_field_explorer::renderer::SceneGPU,
    b: &vector_field_explorer::renderer::SceneGPU,
) {
    assert!(same_arrows(&a.arrows, &b.arrows), "arrow instances differ");
    assert!(same_lines(&a.lines, &b.lines), "line instances differ");
    assert_eq!(a.arrow_params, b.arrow_params, "arrow params differ");
}

#[test]
fn materials_arrows_follow_the_teaching_scene_layer() {
    let mut state = material_state(MaterialCase::DielectricSphere);
    let neutral = state.snapshot();
    assert!(
        !neutral.arrows.is_empty(),
        "the fixture really draws arrows"
    );

    set_layer_full(
        &mut state,
        LayerId::TeachingScene,
        Some(0.5),
        Some(0.5),
        None,
    );
    set_family_full(
        &mut state,
        PhysicsFamily::Electric,
        None,
        None,
        Some(Rgb([10, 20, 30])),
    );
    let styled = state.snapshot();

    assert_eq!(styled.arrows.len(), neutral.arrows.len());
    for (before, after) in neutral.arrows.iter().zip(styled.arrows.iter()) {
        assert_eq!(
            [after.color[0], after.color[1], after.color[2]],
            [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0],
            "the family colour replaces the fixed family RGB"
        );
        assert_eq!(after.color[3], before.color[3] * 0.5, "opacity multiplies");
        assert_eq!(after.dir_len, before.dir_len, "width never changes length");
        assert_eq!(after.origin, before.origin);
    }
    assert_eq!(
        styled.arrow_params.head_width,
        neutral.arrow_params.head_width * 0.5
    );
    assert_eq!(
        styled.arrow_params.shaft_radius,
        neutral.arrow_params.shaft_radius * 0.5
    );
    assert_eq!(
        styled.arrow_params.head_length, neutral.arrow_params.head_length,
        "width never changes the head length"
    );
}

#[test]
fn maxwell_outline_follows_the_teaching_scene_layer() {
    let mut state = maxwell_state(MaxwellCase::HollowSphere);
    let neutral = state.snapshot();
    assert!(!neutral.lines.is_empty(), "the fixture really outlines");

    set_layer_full(
        &mut state,
        LayerId::TeachingScene,
        Some(0.5),
        Some(2.0),
        None,
    );
    set_family_full(
        &mut state,
        PhysicsFamily::Electric,
        None,
        None,
        Some(Rgb([10, 20, 30])),
    );
    let styled = state.snapshot();

    assert_eq!(styled.lines.len(), neutral.lines.len());
    for (before, after) in neutral.lines.iter().zip(styled.lines.iter()) {
        assert_eq!(after.a, before.a);
        assert_eq!(after.b[0], before.b[0]);
        assert_eq!(after.b[1], before.b[1]);
        assert_eq!(after.b[2], before.b[2]);
        assert_eq!(after.b[3], before.b[3] * 2.0, "width multiplier applies");
        assert_eq!(
            [after.color[0], after.color[1], after.color[2]],
            [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0],
            "the family colour replaces the outline RGB"
        );
        assert_eq!(after.color[3], before.color[3] * 0.5, "opacity multiplies");
    }
}

#[test]
fn materials_outline_follows_the_teaching_scene_layer() {
    let mut state = material_state(MaterialCase::PermeableSphere);
    let neutral = state.snapshot();
    assert!(!neutral.lines.is_empty(), "the fixture really outlines");

    set_layer_full(
        &mut state,
        LayerId::TeachingScene,
        Some(0.25),
        Some(3.0),
        None,
    );
    set_family_full(
        &mut state,
        PhysicsFamily::Magnetic,
        None,
        None,
        Some(Rgb([200, 100, 50])),
    );
    let styled = state.snapshot();

    assert_eq!(styled.lines.len(), neutral.lines.len());
    for (before, after) in neutral.lines.iter().zip(styled.lines.iter()) {
        assert_eq!(after.b[3], before.b[3] * 3.0);
        assert_eq!(after.color[3], before.color[3] * 0.25);
        assert_eq!(
            [after.color[0], after.color[1], after.color[2]],
            [200.0 / 255.0, 100.0 / 255.0, 50.0 / 255.0]
        );
    }
}

#[test]
fn neutral_teaching_styles_are_byte_identical() {
    for state in [
        material_state(MaterialCase::DielectricSphere),
        material_state(MaterialCase::PermeableSphere),
        maxwell_state(MaxwellCase::ParallelPlates),
        maxwell_state(MaxwellCase::Solenoid),
    ] {
        let mut neutral = state.clone();
        neutral
            .commit_layer_styles(LayerStyles::default())
            .expect("neutral styles validate");
        assert_scene_bytes_equal(&neutral.snapshot(), &state.snapshot_blueprint());
        assert_scene_bytes_equal(&neutral.snapshot(), &state.snapshot());
    }
}

#[test]
fn teaching_modes_ignore_the_arrows_layer() {
    let mut material = material_state(MaterialCase::DielectricSphere);
    let neutral_material = material.snapshot();
    set_layer_full(
        &mut material,
        LayerId::Arrows,
        Some(0.1),
        Some(3.0),
        Some(Rgb([1, 2, 3])),
    );
    assert_scene_bytes_equal(&neutral_material, &material.snapshot());

    let mut maxwell = maxwell_state(MaxwellCase::ParallelPlates);
    let neutral_maxwell = maxwell.snapshot();
    set_layer_full(
        &mut maxwell,
        LayerId::Arrows,
        Some(0.1),
        Some(3.0),
        Some(Rgb([1, 2, 3])),
    );
    assert_scene_bytes_equal(&neutral_maxwell, &maxwell.snapshot());

    // The teaching override still applies, so the equality above is not
    // vacuous.
    let mut changed = material_state(MaterialCase::DielectricSphere);
    set_layer_full(
        &mut changed,
        LayerId::TeachingScene,
        Some(0.5),
        Some(2.0),
        None,
    );
    assert!(!same_arrows(
        &neutral_material.arrows,
        &changed.snapshot().arrows
    ));
    assert_ne!(
        neutral_material.arrow_params,
        changed.snapshot().arrow_params
    );
}

#[test]
fn teaching_svg_blueprint_stays_neutral() {
    for neutral in [
        material_state(MaterialCase::DielectricSphere),
        maxwell_state(MaxwellCase::ParallelPlates),
    ] {
        let mut styled = neutral.clone();
        set_layer_full(
            &mut styled,
            LayerId::TeachingScene,
            Some(0.2),
            Some(2.5),
            Some(Rgb([9, 8, 7])),
        );
        let blueprint = styled.snapshot_blueprint();
        assert_scene_bytes_equal(&neutral.snapshot(), &blueprint);
        // The styled live scene really differs, so the blueprint equality is
        // not vacuous.
        assert!(!same_arrows(
            &neutral.snapshot().arrows,
            &styled.snapshot().arrows
        ));
        assert!(!same_lines(
            &neutral.snapshot().lines,
            &styled.snapshot().lines
        ));
    }
}

#[test]
fn non_finite_teaching_styles_are_defensively_repaired() {
    let mut state = material_state(MaterialCase::DielectricSphere);
    let baseline = state.snapshot();
    assert!(!baseline.arrows.is_empty() && !baseline.lines.is_empty());

    // A direct field write bypasses `commit_layer_styles` validation.
    state.layer_styles.layers[slot(LayerId::TeachingScene)].opacity = Some(f32::NAN);
    state.layer_styles.layers[slot(LayerId::TeachingScene)].width = Some(-2.0);

    let repaired = state.snapshot();
    for (before, after) in baseline.arrows.iter().zip(repaired.arrows.iter()) {
        assert_eq!(
            after.color[3], before.color[3],
            "NaN opacity repairs to neutral"
        );
        assert!(after.color.iter().all(|channel| channel.is_finite()));
    }
    for (before, after) in baseline.lines.iter().zip(repaired.lines.iter()) {
        assert_eq!(
            after.color[3], before.color[3],
            "NaN opacity repairs to neutral alpha"
        );
        assert!(after.color.iter().all(|channel| channel.is_finite()));
        assert!(
            after.b[3].is_finite() && after.b[3] > 0.0,
            "a negative width is clamped to a usable positive value"
        );
        assert_eq!(
            after.b[3],
            before.b[3] * vector_field_explorer::layer_style::MIN_WIDTH_MULTIPLIER,
            "a finite out-of-range width clamps to the A1 bound"
        );
    }
    assert_eq!(
        repaired.arrow_params.head_width,
        baseline.arrow_params.head_width * vector_field_explorer::layer_style::MIN_WIDTH_MULTIPLIER
    );

    // A non-finite width repairs to the neutral multiplier.
    state.layer_styles.layers[slot(LayerId::TeachingScene)].width = Some(f32::INFINITY);
    let infinite = state.snapshot();
    assert_eq!(infinite.arrow_params, baseline.arrow_params);
    for (before, after) in baseline.lines.iter().zip(infinite.lines.iter()) {
        assert_eq!(after.b[3], before.b[3]);
    }
}

#[test]
fn neutral_effective_params_equal_stored() {
    let state = State::new(true);
    assert_eq!(state.effective_arrow_params(), state.arrow_params);
    assert_eq!(state.effective_arrow_params().head_length, 0.3);
    assert_eq!(state.effective_arrow_params().head_width, 0.12);
    assert_eq!(state.effective_arrow_params().shaft_radius, 0.055);
    assert_eq!(state.effective_arrow_params().sides, 12);
}
