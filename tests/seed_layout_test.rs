//! Issue #87 slice S1: the pure explicit seed-layout model.
//!
//! Covers the collection contract, domain validation, total sanitising, the
//! plane-anchor round trip and the `State` commit seam. No generators, picking,
//! UI or named layouts in this slice.

use glam::{UVec3, Vec3};

use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::seed_layout::{MAX_SEEDS, SeedAnchor, SeedLayout, SeedPoint};
use vector_field_explorer::viewmodel::State;

fn domain() -> Domain {
    Domain::new(Vec3::new(-2.0, -1.0, -3.0), Vec3::new(2.0, 1.0, 3.0))
}

fn seed(x: f32, y: f32, z: f32) -> SeedPoint {
    SeedPoint {
        position: Vec3::new(x, y, z),
        anchor: None,
    }
}

#[test]
fn empty_layout_is_valid_and_sanitizes_to_itself() {
    let layout = SeedLayout::default();
    assert!(layout.is_empty());
    assert_eq!(layout.len(), 0);
    layout
        .validate(&domain())
        .expect("an empty layout is valid");
    assert_eq!(layout.clone().sanitized(&domain()), layout);
}

#[test]
fn layout_at_the_cap_validates_and_one_more_is_rejected() {
    let center = domain().center();
    let at_cap = SeedLayout {
        seeds: vec![
            SeedPoint {
                position: center,
                anchor: None
            };
            MAX_SEEDS
        ],
    };
    at_cap
        .validate(&domain())
        .expect("a layout of exactly MAX_SEEDS seeds is valid");

    let mut over = at_cap.clone();
    over.seeds.push(SeedPoint {
        position: center,
        anchor: None,
    });
    let error = over
        .validate(&domain())
        .expect_err("more than MAX_SEEDS seeds is rejected");
    assert!(
        error.contains(&MAX_SEEDS.to_string()),
        "the cap error must name the cap: {error}"
    );
}

#[test]
fn sanitized_drops_non_finite_and_out_of_domain_seeds_and_truncates() {
    let mut seeds = vec![
        seed(0.0, 0.0, 0.0),
        seed(f32::NAN, 0.0, 0.0),
        seed(9.0, 0.0, 0.0),
        SeedPoint {
            position: Vec3::new(0.5, 0.5, 0.5),
            anchor: Some(SeedAnchor {
                axis: PlaneAxis::Z,
                index: 0,
                u: f32::INFINITY,
                v: 0.0,
            }),
        },
        seed(-1.0, 0.5, 2.0),
    ];
    // Pad with valid seeds so the total also exercises the cap.
    seeds.extend((0..MAX_SEEDS + 20).map(|_| seed(0.0, 0.0, 0.0)));
    let layout = SeedLayout { seeds };
    assert!(
        layout.validate(&domain()).is_err(),
        "the raw layout is invalid"
    );

    let sanitized = layout.sanitized(&domain());
    assert_eq!(
        sanitized.len(),
        MAX_SEEDS,
        "sanitising truncates to the cap"
    );
    assert_eq!(
        sanitized.seeds[0],
        seed(0.0, 0.0, 0.0),
        "valid seeds keep insertion order"
    );
    assert_eq!(sanitized.seeds[1], seed(-1.0, 0.5, 2.0));
    sanitized
        .validate(&domain())
        .expect("a sanitised layout is valid");
}

#[test]
fn anchor_round_trips_through_a_rotated_plane() {
    let domain = domain();
    let grid = UVec3::new(8, 8, 8);
    let plane = PlaneView {
        enabled: true,
        axis: PlaneAxis::X,
        index: 3,
    };
    let frame = PlaneFrame::new(&domain, plane, grid);
    let local = Vec3::new(0.75, -0.4, 0.0);
    let position = frame.to_world(local);
    let point = SeedPoint {
        position,
        anchor: Some(SeedAnchor {
            axis: plane.axis,
            index: plane.index,
            u: local.x,
            v: local.y,
        }),
    };
    let anchor = point.anchor.expect("the test seeds an anchor");
    let recovered = frame.to_world(Vec3::new(anchor.u, anchor.v, 0.0));
    assert!(
        (recovered - position).length() <= 1e-5,
        "the anchor must recover the planted world position: {recovered:?} vs {position:?}"
    );
    let back = frame.to_local(position);
    assert!((back.x - anchor.u).abs() <= 1e-5 && (back.y - anchor.v).abs() <= 1e-5);
}

#[test]
fn push_rejects_in_full_and_keeps_insertion_order() {
    let domain = domain();
    let mut layout = SeedLayout::default();
    layout
        .push(seed(0.0, 0.0, 0.0), &domain)
        .expect("a valid seed is accepted");
    layout
        .push(seed(1.0, -0.5, 2.0), &domain)
        .expect("a valid seed is accepted");
    assert_eq!(
        layout.seeds,
        vec![seed(0.0, 0.0, 0.0), seed(1.0, -0.5, 2.0)],
        "push preserves insertion order"
    );

    let before = layout.clone();
    let error = layout
        .push(seed(9.0, 0.0, 0.0), &domain)
        .expect_err("an out-of-domain seed is rejected");
    assert!(!error.is_empty());
    assert_eq!(layout, before, "a rejected push changes nothing");

    let error = layout
        .push(
            SeedPoint {
                position: Vec3::ZERO,
                anchor: Some(SeedAnchor {
                    axis: PlaneAxis::Y,
                    index: 7,
                    u: f32::NAN,
                    v: 0.0,
                }),
            },
            &domain,
        )
        .expect_err("a non-finite anchor is rejected");
    assert!(!error.is_empty());
    assert_eq!(layout, before, "a rejected push changes nothing");

    let center = domain.center();
    let mut full = SeedLayout {
        seeds: vec![
            SeedPoint {
                position: center,
                anchor: None
            };
            MAX_SEEDS
        ],
    };
    let error = full
        .push(seed(0.0, 0.0, 0.0), &domain)
        .expect_err("a full layout rejects another seed");
    assert!(
        error.contains(&MAX_SEEDS.to_string()),
        "the cap error must name the cap: {error}"
    );
    assert_eq!(
        full.len(),
        MAX_SEEDS,
        "the rejected push does not grow the layout"
    );
}

#[test]
fn commit_seed_layout_validates_no_ops_and_dirties_once() {
    let mut state = State::new(true);
    let center = state.scene_domain().center();
    let layout = SeedLayout {
        seeds: vec![SeedPoint {
            position: center,
            anchor: Some(SeedAnchor {
                axis: PlaneAxis::Z,
                index: 2,
                u: 0.25,
                v: -0.5,
            }),
        }],
    };

    state.dirty = false;
    state
        .commit_seed_layout(layout.clone())
        .expect("a valid layout commits");
    assert_eq!(state.seed_layout, layout);
    assert!(state.dirty, "a changed commit dirties the scene");

    state.dirty = false;
    state
        .commit_seed_layout(layout.clone())
        .expect("an equal layout is a no-op");
    assert_eq!(state.seed_layout, layout);
    assert!(!state.dirty, "an unchanged commit is not dirtying");

    let bad = SeedLayout {
        seeds: vec![seed(1.0e9, 0.0, 0.0)],
    };
    state.dirty = false;
    state
        .commit_seed_layout(bad)
        .expect_err("an out-of-domain layout is rejected");
    assert_eq!(
        state.seed_layout, layout,
        "a rejected commit changes nothing"
    );
    assert!(!state.dirty, "a rejected commit is not dirtying");

    state.dirty = false;
    state.reset_seed_layout();
    assert!(state.seed_layout.is_empty());
    assert!(state.dirty, "clearing a non-empty layout dirties the scene");

    state.dirty = false;
    state.reset_seed_layout();
    assert!(!state.dirty, "clearing an empty layout is a no-op");
}
