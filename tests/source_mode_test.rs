//! Task 2 field modes, source sampling and source-mode gating coverage.

use glam::{DVec3, UVec3};
use vector_field_explorer::color_ramp::Rgb;
use vector_field_explorer::layer_style::PhysicsFamily;
use vector_field_explorer::source_scene::{
    FamilyColors, FamilyDisposition, FieldMode, SourceDisplay, SourceKindTag, SourceScene,
    classify_family_result,
};
use vector_field_explorer::sources::{
    Diagnostics, Estimate, FamilyResult, FieldFamily, StopReason,
};
use vector_field_explorer::ui::appearance_panel::{AppearanceEdit, apply_edit};
use vector_field_explorer::ui::sources_panel::both_legend;
use vector_field_explorer::viewmodel::{MAX_DENSITY_2D, MAX_DENSITY_3D, State};

fn one_point() -> SourceScene {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    scene
}

fn estimate(value: DVec3, uncertainty: f64, target: f64) -> Estimate {
    Estimate {
        value,
        uncertainty,
        target,
        absolute_contributions: uncertainty,
        diagnostics: Diagnostics {
            kernel_id: "test",
            family: FieldFamily::Electric,
            softening: 0.1,
            epsilon: 1e-7,
            family_scale: 1.0,
            source_count: 1,
            work_units: 0,
            cells_high_water: 0,
            model_regularized: true,
            roundoff_limited: false,
            underflow: false,
        },
    }
}

#[test]
fn mode_switches_preserve_formula_and_sources() {
    let mut state = State::new(false);
    state.set_formula("(y, -x, z)".into());
    let formula = state.formula.clone();
    state.set_source_scene(one_point());
    assert_eq!(state.sources.entries.len(), 1);
    assert!(state.set_field_mode(FieldMode::Sources));
    assert_eq!(state.field_mode, FieldMode::Sources);
    assert_eq!(state.formula, formula);
    assert_eq!(state.sources.entries.len(), 1);
    let three_d = state.mode_3d;
    assert!(state.set_field_mode(FieldMode::Formula));
    assert_eq!(state.mode_3d, three_d);
    assert_eq!(state.formula, formula);
    assert_eq!(state.sources.entries.len(), 1);
}

#[test]
fn source_mode_is_three_dimensional_and_2d_uses_the_plane_frame() {
    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.set_mode(false);
    assert_eq!(state.active_dimension(), 3);
    assert!(state.is_plane_slice());
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().expect("source mode sampled");
    assert!(!sampling.arrows.is_empty());
    assert!(sampling.arrows.iter().all(|a| a.position.z.abs() < 1e-6));
}

#[test]
fn accuracy_not_met_is_hidden_by_default_and_opt_in_draws_finite_only() {
    let estimate = estimate(DVec3::X, 1.0, 0.1);
    let not_met = FamilyResult::AccuracyNotMet(estimate.clone(), StopReason::WorkLimit);
    assert_eq!(
        classify_family_result(&not_met, false),
        FamilyDisposition::HiddenAccuracy
    );
    assert_eq!(
        classify_family_result(&not_met, true),
        FamilyDisposition::Draw
    );
    assert_eq!(
        classify_family_result(&FamilyResult::Unavailable(StopReason::NumericRange), true),
        FamilyDisposition::Unavailable
    );

    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.sources.allow_inaccurate = false;
    state.mark_dirty();
    state.snapshot_if_dirty();
    let default_sampling = state.source_sampling().unwrap();
    assert_eq!(default_sampling.electric.accuracy_not_met_drawn, 0);
}

#[test]
fn both_display_uses_two_family_colours_without_summing() {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    scene
        .add(SourceKindTag::CurrentLoop.default_source())
        .unwrap();
    scene.set_display(SourceDisplay::Both);
    let mut state = State::new(true);
    state.set_source_scene(scene);
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().unwrap();
    let electric: Vec<_> = sampling
        .arrows
        .iter()
        .filter(|a| a.color == [255, 176, 64])
        .collect();
    let magnetic: Vec<_> = sampling
        .arrows
        .iter()
        .filter(|a| a.color == [64, 196, 255])
        .collect();
    assert!(!electric.is_empty(), "E family is drawn");
    assert!(!magnetic.is_empty(), "B family is drawn");
}

fn both_scene() -> SourceScene {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    scene
        .add(SourceKindTag::CurrentLoop.default_source())
        .unwrap();
    scene.set_display(SourceDisplay::Both);
    scene
}

fn family_rgb(color: Rgb) -> [f32; 3] {
    color.0.map(|channel| f32::from(channel) / 255.0)
}

#[test]
fn both_arrows_follow_appearance_family_colours_and_reset_to_defaults() {
    let mut state = State::new(true);
    state.set_source_scene(both_scene());
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    assert_eq!(
        state.source_family_colors(),
        FamilyColors::default(),
        "an unset Appearance record keeps the shipped family colours"
    );

    let electric = Rgb([12, 34, 56]);
    let magnetic = Rgb([200, 150, 100]);
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Electric, Some(electric)),
    )
    .unwrap();
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Magnetic, Some(magnetic)),
    )
    .unwrap();

    // Same-frame read: the helper and the legend read the committed record
    // before any explicit resample.
    assert_eq!(
        state.source_family_colors(),
        FamilyColors {
            electric: electric.0,
            magnetic: magnetic.0,
        }
    );
    assert_eq!(
        both_legend(state.source_family_colors()),
        [("E", electric.0), ("B", magnetic.0)]
    );

    // The edit dirties the scene, so the very next rebuild recolours Both.
    state
        .snapshot_if_dirty()
        .expect("an Appearance edit dirties the source scene");
    let sampling = state.source_sampling().expect("source mode sampled");
    let mut saw_electric = false;
    let mut saw_magnetic = false;
    for arrow in &sampling.arrows {
        match arrow.color {
            color if color == electric.0 => saw_electric = true,
            color if color == magnetic.0 => saw_magnetic = true,
            other => panic!("Both kept a stale arrow colour {other:?}"),
        }
    }
    assert!(saw_electric, "electric arrows carry the override");
    assert!(saw_magnetic, "magnetic arrows carry the override");

    let instances = state.arrow_instances(&sampling.arrows);
    assert!(
        instances
            .iter()
            .any(|instance| instance.color[..3] == family_rgb(electric)),
        "the GPU instances use the electric override"
    );
    assert!(
        instances
            .iter()
            .any(|instance| instance.color[..3] == family_rgb(magnetic)),
        "the GPU instances use the magnetic override"
    );

    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Electric, None),
    )
    .unwrap();
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Magnetic, None),
    )
    .unwrap();
    state
        .snapshot_if_dirty()
        .expect("clearing an override dirties the source scene");
    let arrows = &state.source_sampling().expect("source mode sampled").arrows;
    assert!(
        arrows
            .iter()
            .any(|arrow| arrow.color == FamilyColors::default().electric),
        "resetting restores the shipped electric colour"
    );
    assert!(
        arrows
            .iter()
            .any(|arrow| arrow.color == FamilyColors::default().magnetic),
        "resetting restores the shipped magnetic colour"
    );
    assert!(
        arrows
            .iter()
            .all(|arrow| arrow.color != electric.0 && arrow.color != magnetic.0),
        "the cleared overrides no longer paint any arrow"
    );
}

#[test]
fn mixed_family_override_keeps_the_other_family_on_the_shipped_fallback() {
    let mut state = State::new(true);
    state.set_source_scene(both_scene());
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();

    let electric = Rgb([12, 34, 56]);
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Electric, Some(electric)),
    )
    .unwrap();

    // Only one family is overridden; the other must fall back family by
    // family to the shipped constant rather than inheriting the override.
    let defaults = FamilyColors::default();
    let resolved = FamilyColors {
        electric: electric.0,
        magnetic: defaults.magnetic,
    };
    assert_eq!(state.source_family_colors(), resolved);
    assert_eq!(
        both_legend(state.source_family_colors()),
        [("E", electric.0), ("B", defaults.magnetic)]
    );

    state
        .snapshot_if_dirty()
        .expect("an Appearance edit dirties the source scene");
    let sampling = state.source_sampling().expect("source mode sampled");
    let mut saw_electric = false;
    let mut saw_magnetic = false;
    for arrow in &sampling.arrows {
        match arrow.color {
            color if color == electric.0 => saw_electric = true,
            color if color == defaults.magnetic => saw_magnetic = true,
            other => panic!("Both kept an unaccounted arrow colour {other:?}"),
        }
    }
    assert!(saw_electric, "electric arrows carry the override");
    assert!(
        saw_magnetic,
        "magnetic arrows keep the shipped fallback while electric is overridden"
    );

    let instances = state.arrow_instances(&sampling.arrows);
    assert!(
        instances
            .iter()
            .any(|instance| instance.color[..3] == family_rgb(electric)),
        "the GPU instances use the electric override"
    );
    assert!(
        instances
            .iter()
            .any(|instance| instance.color[..3] == family_rgb(Rgb(defaults.magnetic))),
        "the GPU instances keep the shipped magnetic fallback"
    );
}

#[test]
fn single_family_display_keeps_its_colormap_when_a_family_colour_is_overridden() {
    let mut scene = both_scene();
    scene.set_display(SourceDisplay::Electric);
    let mut state = State::new(true);
    state.set_source_scene(scene);
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();

    let electric = Rgb([1, 2, 3]);
    apply_edit(
        &mut state,
        AppearanceEdit::FamilyColor(PhysicsFamily::Electric, Some(electric)),
    )
    .unwrap();
    state
        .snapshot_if_dirty()
        .expect("an Appearance edit dirties the source scene");
    let arrows = &state.source_sampling().expect("source mode sampled").arrows;
    assert!(!arrows.is_empty(), "the electric display drew arrows");
    assert!(
        arrows.iter().all(|arrow| arrow.color != electric.0),
        "a single-family display colours through its colormap, not the family override"
    );
    assert!(
        arrows.iter().any(|arrow| arrow.color != arrows[0].color),
        "the colormap still varies with magnitude"
    );
}

#[test]
fn source_mode_has_no_formula_transients_and_does_not_consume_the_clock() {
    let mut state = State::new(true);
    state.set_formula("(-y, x, 0)".into());
    state.set_streamlines(true);
    state.set_particles(true);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    assert!(state.streamlines_snapshot(&[]).is_empty());
    assert!(state.click_streamlines_snapshot((0.0, 1.0), &[]).is_empty());
    assert!(state.particle_target().is_none());
    assert!(!state.has_time_dependent_consumer());
}

#[test]
fn an_empty_family_is_zero_not_an_error() {
    let mut scene = SourceScene::default();
    scene
        .add(SourceKindTag::PointCharge.default_source())
        .unwrap();
    let mut state = State::new(true);
    state.set_source_scene(scene);
    state.set_field_mode(FieldMode::Sources);
    state.sources.set_display(SourceDisplay::Magnetic);
    state.snapshot_if_dirty();
    let sampling = state.source_sampling().unwrap();
    assert!(sampling.magnetic.empty);
    assert_eq!(sampling.magnetic.drawn, 0);
    assert!(sampling.validation_error.is_none());
}

#[test]
fn formula_edits_do_not_replace_source_samples() {
    let mut state = State::new(false);
    state.set_source_scene(one_point());
    state.set_field_mode(FieldMode::Sources);
    state.snapshot_if_dirty();
    let before = state.source_sampling().unwrap().arrows.clone();
    state.set_formula("(1, 2, 3)".into());
    state.snapshot_if_dirty();
    let after = state.source_sampling().unwrap().arrows.clone();
    assert_eq!(before, after);
}

#[test]
fn source_mode_never_samples_above_the_3d_density_cap() {
    // A legal 2D source view keeps the 200² cap, but toggling the user-controlled
    // 3D view must re-clamp the density to 40³ rather than sampling 200³.
    let mut state = State::new(false);
    state.set_source_scene(one_point());
    assert!(state.set_density(MAX_DENSITY_2D));
    assert_eq!(state.density, MAX_DENSITY_2D);
    assert!(state.set_field_mode(FieldMode::Sources));
    assert_eq!(
        state.density, MAX_DENSITY_2D,
        "2D source sampling still affords the 2D cap"
    );
    assert_eq!(state.grid(), UVec3::new(MAX_DENSITY_2D, MAX_DENSITY_2D, 1));
    assert!(state.set_mode(true));
    assert_eq!(state.density, MAX_DENSITY_3D);
    assert_eq!(state.grid(), UVec3::splat(MAX_DENSITY_3D));

    // Entering Sources from an already-3D two-component formula view must also
    // re-clamp, because Sources makes that view cubic.
    let mut already_3d = State::new(false);
    assert!(already_3d.set_mode(true));
    assert!(already_3d.set_density(MAX_DENSITY_2D));
    assert_eq!(already_3d.density, MAX_DENSITY_2D);
    already_3d.set_source_scene(one_point());
    assert!(already_3d.set_field_mode(FieldMode::Sources));
    assert_eq!(already_3d.density, MAX_DENSITY_3D);
    assert_eq!(already_3d.grid(), UVec3::splat(MAX_DENSITY_3D));
}
