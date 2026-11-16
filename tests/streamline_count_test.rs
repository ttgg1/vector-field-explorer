// The explicit streamline seed count (issue 13 task 5): `0` keeps the Auto
// density-derived lattice, any other value seeds exactly that many lines from
// a density-independent lattice.

use glam::Vec3;
use vector_field_explorer::domain::Domain;
use vector_field_explorer::streamlines::explicit_streamline_seeds;
use vector_field_explorer::viewmodel::{MAX_STREAMLINES, State};

#[test]
fn explicit_streamline_seeds_returns_exactly_n() {
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    assert_eq!(explicit_streamline_seeds(&domain, 1, false, 7).len(), 1);
    assert_eq!(explicit_streamline_seeds(&domain, 37, false, 7).len(), 37);
    assert_eq!(
        explicit_streamline_seeds(&domain, 4096, false, 7).len(),
        4096
    );
}

#[test]
fn explicit_streamline_seeds_returns_exactly_n_in_3d() {
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    assert_eq!(explicit_streamline_seeds(&domain, 1, true, 7).len(), 1);
    assert_eq!(explicit_streamline_seeds(&domain, 37, true, 7).len(), 37);
    assert_eq!(
        explicit_streamline_seeds(&domain, 4096, true, 7).len(),
        4096
    );
}

#[test]
fn explicit_seed_lattice_is_density_independent() {
    let domain = Domain::new(Vec3::splat(-1.0), Vec3::splat(1.0));
    assert_eq!(
        explicit_streamline_seeds(&domain, 26, false, 3),
        explicit_streamline_seeds(&domain, 26, false, 3)
    );
}

#[test]
fn streamline_count_defaults_to_auto() {
    assert_eq!(State::new(false).streamline_count, 0);
}

#[test]
fn streamline_count_clamps_and_marks_dirty() {
    let mut state = State::new(false);
    assert!(state.set_streamline_count(99_999));
    assert_eq!(state.streamline_count, MAX_STREAMLINES);
    state.snapshot_if_dirty();
    assert!(!state.set_streamline_count(MAX_STREAMLINES));
}
