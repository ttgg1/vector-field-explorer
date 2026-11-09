//! Ticket 36: arrow opacity and the plane-mode off-plane opacity.
//!
//! Two independent knobs: `arrow_opacity` is the user's global de-emphasis,
//! `plane_dim_opacity` the contrast of plane-by-plane mode. The effective alpha
//! is `arrow_opacity` normally and `arrow_opacity * plane_dim_opacity` only for
//! off-plane arrows while plane mode is active. RGB must never move, and at
//! full opacity the alpha is exactly `1.0`, so the always-alpha pipeline stays
//! pixel-identical (the brief's ruling: no second/opaque pipeline).

use vector_field_explorer::renderer::ArrowInstanceGPU;
use vector_field_explorer::sampling::ArrowData;
use vector_field_explorer::viewmodel::{
    MAX_ARROW_OPACITY, MAX_PLANE_DIM_OPACITY, MIN_ARROW_OPACITY, MIN_PLANE_DIM_OPACITY, State,
};
use vector_field_explorer::plane::{DIM_ALPHA, on_plane};

fn instances(state: &State) -> (Vec<ArrowData>, Vec<ArrowInstanceGPU>) {
    let sampled = state.sample();
    let instances = state.arrow_instances(&sampled);
    assert!(!instances.is_empty(), "the default field samples at least one arrow");
    assert_eq!(sampled.len(), instances.len(), "one instance per sample");
    (sampled, instances)
}

fn assert_all_alphas(state: &State, expected: f32) {
    let (_, instances) = instances(state);
    for (index, instance) in instances.iter().enumerate() {
        assert_eq!(
            instance.color[3], expected,
            "instance {index} alpha should be exactly {expected}, got {}",
            instance.color[3],
        );
    }
}

fn three_d_with_plane() -> State {
    let mut state = State::new(true);
    assert!(state.set_plane_enabled(true), "plane mode on");
    assert!(state.plane_active(), "the default 3D field has a sampled cube");
    state
}

// --- Global arrow opacity (plane off) --------------------------------------

#[test]
fn off_plane_alpha_equals_the_arrow_opacity() {
    let mut state = State::new(true);
    assert!(!state.plane_active(), "plane mode starts off");

    for opacity in [1.0, 0.5, MIN_ARROW_OPACITY] {
        state.set_arrow_opacity(opacity);
        assert_eq!(state.arrow_opacity, opacity, "the setter stores {opacity}");
        assert_all_alphas(&state, opacity);
    }
}

#[test]
fn two_d_honours_the_arrow_opacity() {
    let mut state = State::new(false);
    assert!(!state.is_3d(), "this is the 2D projection");
    assert!(state.set_arrow_opacity(0.35));
    assert_all_alphas(&state, 0.35);
}

#[test]
fn opacity_never_changes_rgb() {
    let mut state = State::new(true);
    let (_, baseline) = instances(&state);
    let baseline_rgb: Vec<[f32; 3]> = baseline
        .iter()
        .map(|instance| [instance.color[0], instance.color[1], instance.color[2]])
        .collect();

    assert!(state.set_arrow_opacity(0.25));
    let (_, dimmed) = instances(&state);
    for (index, instance) in dimmed.iter().enumerate() {
        assert_eq!(
            [instance.color[0], instance.color[1], instance.color[2]],
            baseline_rgb[index],
            "instance {index} RGB is untouched by the opacity knob",
        );
    }
}

#[test]
fn full_opacity_is_exactly_one_for_pixel_identity() {
    let mut state = State::new(true);
    assert_eq!(state.arrow_opacity, 1.0, "the default is fully opaque");
    assert_all_alphas(&state, 1.0);

    // Turning the knob away and back must still land on exactly 1.0.
    assert!(state.set_arrow_opacity(0.4));
    assert!(state.set_arrow_opacity(1.0));
    assert_all_alphas(&state, 1.0);
}

// --- Plane-mode off-plane opacity ------------------------------------------

#[test]
fn plane_on_uses_the_global_opacity_on_plane_and_the_product_off_plane() {
    let mut state = three_d_with_plane();
    assert!(state.set_arrow_opacity(0.5));

    for dim in [0.2, 0.0, 1.0] {
        state.set_plane_dim_opacity(dim);
        assert_eq!(state.plane_dim_opacity, dim, "the setter stores {dim}");
        let (sampled, instances) = instances(&state);
        let grid = state.grid();
        let mut on_plane_seen = 0;
        let mut off_plane_seen = 0;
        for (arrow, instance) in sampled.iter().zip(instances.iter()) {
            let expected = if on_plane(arrow.position, &state.domain, grid, &state.plane) {
                on_plane_seen += 1;
                0.5
            } else {
                off_plane_seen += 1;
                0.5 * dim
            };
            assert_eq!(instance.color[3], expected, "dim {dim}: alpha = global x plane dim");
        }
        assert!(on_plane_seen > 0, "the selected z slice has samples");
        assert!(off_plane_seen > 0, "the cube has samples off the selected slice");
    }
}

#[test]
fn plane_off_ignores_the_off_plane_opacity() {
    let mut state = State::new(true);
    assert!(!state.plane_active());
    assert!(state.set_arrow_opacity(0.6));
    assert!(state.set_plane_dim_opacity(0.0));
    assert_all_alphas(&state, 0.6);
}

#[test]
fn two_d_ignores_the_off_plane_opacity() {
    let mut state = State::new(false);
    assert!(state.set_arrow_opacity(0.6));
    assert!(state.set_plane_dim_opacity(0.0));
    // Enabling the plane mirror in 2D must not start dimming: sampling is flat.
    state.set_plane_enabled(true);
    assert!(!state.plane_active(), "2D has no plane dimming");
    assert_all_alphas(&state, 0.6);
}

#[test]
fn full_opacity_plane_on_with_dim_one_is_exactly_one() {
    let mut state = three_d_with_plane();
    state.set_arrow_opacity(1.0);
    state.set_plane_dim_opacity(1.0);
    assert_eq!(state.arrow_opacity, 1.0);
    assert_eq!(state.plane_dim_opacity, 1.0);
    assert_all_alphas(&state, 1.0);
}

#[test]
fn off_plane_opacity_never_changes_rgb() {
    let mut state = three_d_with_plane();
    let (_, baseline) = instances(&state);
    let baseline_rgb: Vec<[f32; 3]> = baseline
        .iter()
        .map(|instance| [instance.color[0], instance.color[1], instance.color[2]])
        .collect();

    assert!(state.set_plane_dim_opacity(0.0));
    let (_, zero_dim) = instances(&state);
    for (index, instance) in zero_dim.iter().enumerate() {
        assert_eq!(
            [instance.color[0], instance.color[1], instance.color[2]],
            baseline_rgb[index],
            "instance {index} keeps its dimmed/colormap RGB; only alpha moves",
        );
    }
}

// --- Defaults and setter hygiene -------------------------------------------

#[test]
fn the_state_defaults_are_full_arrow_opacity_and_the_dim_alpha() {
    let flat = State::new(false);
    assert_eq!(flat.arrow_opacity, 1.0, "arrows start fully opaque");
    assert_eq!(flat.plane_dim_opacity, DIM_ALPHA, "off-plane starts at the dim alpha");
    assert_eq!(flat.plane_dim_opacity, 0.2, "DIM_ALPHA is the historical 0.2");
    assert_eq!(MIN_ARROW_OPACITY, 0.05, "the arrow-opacity floor is 5%");
    assert_eq!(MAX_ARROW_OPACITY, 1.0);
    assert_eq!(MIN_PLANE_DIM_OPACITY, 0.0);
    assert_eq!(MAX_PLANE_DIM_OPACITY, 1.0);
}

#[test]
fn arrow_opacity_setter_clamps_and_rejects_non_finite_values() {
    let mut state = State::new(false);
    assert!(state.snapshot_if_dirty().is_some(), "consume the fresh scene");

    assert!(state.set_arrow_opacity(0.0), "below the floor still applies clamped");
    assert_eq!(state.arrow_opacity, MIN_ARROW_OPACITY);
    assert!(!state.set_arrow_opacity(MIN_ARROW_OPACITY), "the same value is not a change");

    assert!(state.set_arrow_opacity(5.0), "above the ceiling still applies clamped");
    assert_eq!(state.arrow_opacity, MAX_ARROW_OPACITY);

    assert!(state.set_arrow_opacity(0.4));
    assert!(state.set_arrow_opacity(f32::NAN), "NaN falls back instead of poisoning");
    assert_eq!(state.arrow_opacity, 1.0);
    assert!(state.set_arrow_opacity(0.4));
    assert!(state.set_arrow_opacity(f32::INFINITY));
    assert_eq!(state.arrow_opacity, 1.0);
    assert!(state.set_arrow_opacity(0.4));
    assert!(state.set_arrow_opacity(f32::NEG_INFINITY));
    assert_eq!(state.arrow_opacity, 1.0);
}

#[test]
fn plane_dim_opacity_setter_clamps_and_rejects_non_finite_values() {
    let mut state = State::new(false);
    assert!(state.snapshot_if_dirty().is_some(), "consume the fresh scene");

    assert!(state.set_plane_dim_opacity(-2.0), "below zero clamps");
    assert_eq!(state.plane_dim_opacity, 0.0);
    assert!(!state.set_plane_dim_opacity(0.0), "the same value is not a change");

    assert!(state.set_plane_dim_opacity(5.0), "above one clamps");
    assert_eq!(state.plane_dim_opacity, 1.0);

    assert!(state.set_plane_dim_opacity(0.5));
    assert!(state.set_plane_dim_opacity(f32::NAN));
    assert_eq!(state.plane_dim_opacity, DIM_ALPHA, "NaN falls back to the dim alpha");
    assert!(state.set_plane_dim_opacity(0.5));
    assert!(state.set_plane_dim_opacity(f32::INFINITY));
    assert_eq!(state.plane_dim_opacity, DIM_ALPHA, "Infinity falls back too");
}

#[test]
fn the_opacity_setters_only_dirty_on_a_real_change() {
    let mut state = State::new(false);
    assert!(state.snapshot_if_dirty().is_some(), "consume the fresh scene");

    assert!(!state.set_arrow_opacity(1.0), "the default value is a no-op");
    assert!(!state.set_plane_dim_opacity(DIM_ALPHA), "the default dim is a no-op");
    assert!(!state.dirty, "no-op setters leave the scene clean");

    assert!(state.set_arrow_opacity(0.5));
    assert!(state.dirty, "a real change dirties");
    assert!(state.snapshot_if_dirty().is_some(), "and rebuilds once");

    assert!(state.set_plane_dim_opacity(0.4));
    assert!(state.dirty, "a real dim change dirties");
    assert!(state.snapshot_if_dirty().is_some());

    assert!(!state.set_arrow_opacity(0.5), "unchanged opacity is a no-op");
    assert!(!state.set_plane_dim_opacity(0.4), "unchanged dim is a no-op");
    assert!(state.snapshot_if_dirty().is_none(), "still clean");
}
