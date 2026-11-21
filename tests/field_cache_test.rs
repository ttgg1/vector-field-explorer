//! Issue #240: bounded field-evaluation cache contract tests.

use glam::{UVec3, Vec3};
use vector_field_explorer::arrow_style::ArrowStyle;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field_cache::{FieldSampleCache, FieldSampleKey, SAMPLE_CAP};
use vector_field_explorer::parameters::{Parameter, ParameterSet};
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::sampling::{ArrowData, ViewOptions, sample_field_at, sample_plane_at};
use vector_field_explorer::source_scene::FieldMode;
use vector_field_explorer::viewmodel::State;

fn base_key() -> FieldSampleKey {
    FieldSampleKey {
        formula: "(x, y)".to_string(),
        parameters: ParameterSet::default(),
        domain: Domain::default(),
        density: 8,
        mode_3d: false,
        plane_frame: None,
        field_mode: FieldMode::Formula,
        view: ViewOptions::default(),
    }
}

fn expected_samples(state: &State) -> Vec<ArrowData> {
    match state.plane_frame() {
        Some(frame) => sample_plane_at(
            &state.field,
            &frame,
            state.grid().x,
            &state.view_options(),
            state.time,
        ),
        None => sample_field_at(
            &state.field,
            &state.domain,
            state.grid(),
            &state.view_options(),
            state.time,
        ),
    }
}

fn assert_sample_bytes_equal(a: &[ArrowData], b: &[ArrowData]) {
    assert_eq!(a.len(), b.len(), "sample count differs");
    for (left, right) in a.iter().zip(b) {
        assert_eq!(
            left.position.to_array().map(f32::to_bits),
            right.position.to_array().map(f32::to_bits),
            "position differs"
        );
        assert_eq!(
            left.dir.to_array().map(f32::to_bits),
            right.dir.to_array().map(f32::to_bits),
            "direction differs"
        );
        assert_eq!(left.mag.to_bits(), right.mag.to_bits(), "magnitude differs");
        assert_eq!(left.color, right.color, "colour differs");
    }
}

fn plane_slice_state() -> State {
    let mut state = State::new(false);
    state.set_formula("(-y, x, z)".to_string());
    state
}

#[test]
fn second_identical_pass_is_served_from_the_cache() {
    let mut state = State::new(false);
    let first = state.sample();
    assert_eq!(state.field_sample_passes_for_tests(), 1);
    assert!(!state.field_sample_cache_is_empty_for_tests());

    let second = state.sample();
    assert_eq!(state.field_sample_passes_for_tests(), 1, "second pass hit");
    assert_sample_bytes_equal(&first, &second);

    // Style-only change: the sampled Arrows are byte-identical and must hit.
    assert!(state.set_arrow_style(ArrowStyle::Flat));
    let styled = state.sample();
    assert_eq!(
        state.field_sample_passes_for_tests(),
        1,
        "style-only rebuild must hit"
    );
    assert_sample_bytes_equal(&first, &styled);
}

#[test]
fn every_key_component_change_forces_a_fresh_key() {
    let base = base_key();
    let cases: Vec<(&str, FieldSampleKey)> = vec![
        (
            "formula",
            FieldSampleKey {
                formula: "(x, -y)".to_string(),
                ..base.clone()
            },
        ),
        (
            "parameter value",
            FieldSampleKey {
                parameters: ParameterSet {
                    version: 1,
                    entries: vec![Parameter {
                        name: "a".to_string(),
                        value: 2.0,
                        step: 1.0,
                        ..Parameter::default()
                    }],
                },
                ..base.clone()
            },
        ),
        (
            "domain",
            FieldSampleKey {
                domain: Domain::new(Vec3::splat(-2.0), Vec3::splat(2.0)),
                ..base.clone()
            },
        ),
        (
            "density",
            FieldSampleKey {
                density: 9,
                ..base.clone()
            },
        ),
        (
            "mode",
            FieldSampleKey {
                mode_3d: true,
                ..base.clone()
            },
        ),
        (
            "plane frame",
            FieldSampleKey {
                plane_frame: Some(PlaneFrame::new(
                    &Domain::default(),
                    PlaneView {
                        enabled: true,
                        axis: PlaneAxis::Z,
                        index: 0,
                    },
                    UVec3::splat(8),
                )),
                ..base.clone()
            },
        ),
        (
            "field mode",
            FieldSampleKey {
                field_mode: FieldMode::Sources,
                ..base.clone()
            },
        ),
        (
            "colour view options",
            FieldSampleKey {
                view: ViewOptions {
                    colormap: Colormap::Turbo,
                    ..ViewOptions::default()
                },
                ..base.clone()
            },
        ),
    ];

    for (name, changed) in cases {
        assert_ne!(base, changed, "{name} must change the key");
    }
}

#[test]
fn changing_a_key_component_through_the_state_forces_a_fresh_pass() {
    let mut state = State::new(false);
    state.sample();
    assert_eq!(state.field_sample_passes_for_tests(), 1);
    assert!(state.set_density(12));
    state.sample();
    assert_eq!(state.field_sample_passes_for_tests(), 2);
}

#[test]
fn animated_time_advance_always_misses() {
    let mut state = State::new(false);
    state.set_formula("(sin(t), cos(t))".to_string());
    state.sample();
    state.sample();
    assert_eq!(
        state.field_sample_passes_for_tests(),
        2,
        "time-dependent fields are not cached"
    );

    state.set_time(0.5);
    state.sample();
    assert_eq!(state.field_sample_passes_for_tests(), 3);
}

#[test]
fn oversized_passes_are_not_cached_and_the_cap_is_honoured() {
    let sample = ArrowData {
        position: Vec3::ZERO,
        dir: Vec3::X,
        mag: 1.0,
        color: [0, 0, 0],
    };
    let key = base_key();
    let mut cache = FieldSampleCache::default();
    assert!(cache.is_empty());

    let at_cap = vec![sample; SAMPLE_CAP];
    assert!(cache.insert(key.clone(), at_cap));
    assert_eq!(cache.sample_count(), Some(SAMPLE_CAP));

    let over_cap = vec![sample; SAMPLE_CAP + 1];
    assert!(!cache.insert(key.clone(), over_cap));
    assert!(cache.is_empty(), "oversized insert must not be retained");
    assert_eq!(cache.get(&key), None);
}

#[test]
fn changed_view_options_miss_even_without_clearing() {
    let key = base_key();
    let sample = ArrowData {
        position: Vec3::ZERO,
        dir: Vec3::X,
        mag: 1.0,
        color: [0, 0, 0],
    };
    let mut cache = FieldSampleCache::default();
    assert!(cache.insert(key.clone(), vec![sample]));
    assert!(cache.get(&key).is_some());

    let changed_view = FieldSampleKey {
        view: ViewOptions {
            colormap: Colormap::Turbo,
            ..ViewOptions::default()
        },
        ..key.clone()
    };
    assert_eq!(
        cache.get(&changed_view),
        None,
        "colour inputs are part of the key"
    );
}

#[test]
fn colour_option_change_changes_sampled_colour_bytes_without_setter_clearing() {
    let mut state = State::new(false);
    let before = state.sample();
    assert_eq!(state.field_sample_passes_for_tests(), 1);
    assert!(!before.is_empty());

    // Bypass `set_colormap`: only key completeness protects this path.
    state.colormap = Colormap::Turbo;
    let after = state.sample();
    assert_eq!(
        state.field_sample_passes_for_tests(),
        2,
        "a changed ViewOptions must miss even when no setter cleared the cache"
    );
    assert!(
        before
            .iter()
            .zip(&after)
            .any(|(left, right)| left.color != right.color),
        "changing the colormap must change sampled arrow colour bytes"
    );
}

#[test]
fn time_advance_on_static_field_keeps_the_cache_hit() {
    let mut state = State::new(false);
    state.sample();
    assert_eq!(state.field_sample_passes_for_tests(), 1);

    assert!(state.set_time(0.5), "time changed");
    state.sample();
    assert_eq!(
        state.field_sample_passes_for_tests(),
        1,
        "a time-invariant field keeps hitting across a clock advance"
    );
    assert!(!state.field_sample_cache_is_empty_for_tests());
}

#[test]
fn cached_and_uncached_samples_are_byte_identical() {
    for state in [State::new(false), State::new(true), plane_slice_state()] {
        let expected = expected_samples(&state);
        let first = state.sample();
        let hit = state.sample();
        assert_sample_bytes_equal(&expected, &first);
        assert_sample_bytes_equal(&expected, &hit);
        assert_sample_bytes_equal(&first, &hit);
    }
}

#[test]
fn invalidating_setters_clear_the_cache_table_driven() {
    type Make = fn() -> State;
    type Apply = fn(&mut State) -> bool;

    let cases: Vec<(&str, Make, Apply)> = vec![
        (
            "formula",
            || State::new(false),
            |state: &mut State| state.set_formula("(x, -y)".to_string()),
        ),
        (
            "parameters",
            || State::new(false),
            |state: &mut State| {
                state
                    .commit_parameters(ParameterSet {
                        version: 1,
                        entries: vec![Parameter {
                            name: "a".to_string(),
                            value: 2.0,
                            step: 1.0,
                            ..Parameter::default()
                        }],
                    })
                    .expect("valid parameter set");
                true
            },
        ),
        (
            "domain",
            || State::new(false),
            |state: &mut State| state.set_domain(Vec3::splat(-2.0), Vec3::splat(2.0)),
        ),
        (
            "density",
            || State::new(false),
            |state: &mut State| state.set_density(8),
        ),
        (
            "mode",
            || State::new(false),
            |state: &mut State| state.set_mode(true),
        ),
        ("plane", plane_slice_state, |state: &mut State| {
            state.set_plane_axis(PlaneAxis::X)
        }),
        (
            "colour",
            || State::new(false),
            |state: &mut State| state.set_colormap(Colormap::Turbo),
        ),
    ];

    for (name, make, apply) in cases {
        let mut state = make();
        state.sample();
        assert_eq!(state.field_sample_passes_for_tests(), 1, "{name}: primed");
        assert!(
            !state.field_sample_cache_is_empty_for_tests(),
            "{name}: cache starts populated"
        );

        assert!(apply(&mut state), "{name}: setter reports a change");
        assert!(
            state.field_sample_cache_is_empty_for_tests(),
            "{name}: invalidating setter clears the cache"
        );

        let fresh = state.sample();
        assert_eq!(
            state.field_sample_passes_for_tests(),
            2,
            "{name}: next pass is fresh"
        );
        assert_sample_bytes_equal(&fresh, &expected_samples(&state));
    }
}

#[test]
fn stale_key_entry_is_never_returned_even_without_a_clear() {
    let key = base_key();
    let sample = ArrowData {
        position: Vec3::ZERO,
        dir: Vec3::X,
        mag: 1.0,
        color: [0, 0, 0],
    };
    let mut cache = FieldSampleCache::default();
    assert!(cache.insert(key.clone(), vec![sample]));
    let changed = FieldSampleKey {
        density: key.density + 1,
        ..key.clone()
    };
    assert_eq!(cache.get(&changed), None);
    cache.clear();
    assert_eq!(cache.get(&key), None);
}
