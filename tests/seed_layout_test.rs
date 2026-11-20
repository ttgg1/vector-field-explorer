//! Issue #87 slice S1: the pure explicit seed-layout model.
//!
//! Covers the collection contract, domain validation, total sanitising, the
//! plane-anchor round trip and the `State` commit seam. No generators, picking,
//! UI or named layouts in this slice.

use glam::{UVec3, Vec3};

use vector_field_explorer::domain::Domain;
use vector_field_explorer::plane::{PlaneAxis, PlaneFrame, PlaneView};
use vector_field_explorer::seed_layout::{
    MAX_GENERATED_PER_APPLICATION, MAX_SEEDS, SeedAnchor, SeedError, SeedLayout, SeedPoint,
    circle_seeds, clamp_seed_local, line_seeds, region_seeds,
};
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
fn line_circle_and_region_generators_are_deterministic_and_inclusive() {
    let domain = domain();
    let from = Vec3::new(-1.5, 0.0, 0.0);
    let to = Vec3::new(1.5, 0.5, 0.0);
    let line = line_seeds(&domain, from, to, 5).expect("a valid line generates seeds");
    assert_eq!(line.len(), 5, "the generator honours its count");
    assert_eq!(line[0], from, "the line includes its start endpoint");
    assert_eq!(line[4], to, "the line includes its end endpoint");
    assert_eq!(
        line,
        line_seeds(&domain, from, to, 5).expect("the same line generates the same seeds"),
        "line generation is deterministic"
    );

    let circle = circle_seeds(&domain, Vec3::ZERO, PlaneAxis::Z, 1.0, 4)
        .expect("a valid circle generates seeds");
    assert_eq!(circle.len(), 4, "the generator honours its count");
    let expected = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
    ];
    for (actual, expected) in circle.iter().zip(expected) {
        assert!(
            (*actual - expected).length() <= 1e-5,
            "the circle starts at angle 0 and steps evenly: {actual:?} vs {expected:?}"
        );
    }
    assert_eq!(
        circle,
        circle_seeds(&domain, Vec3::ZERO, PlaneAxis::Z, 1.0, 4)
            .expect("the same circle generates the same seeds"),
        "circle generation is deterministic"
    );

    let min = Vec3::new(-1.0, -0.5, 0.0);
    let max = Vec3::new(1.0, 0.5, 0.0);
    let region = region_seeds(&domain, min, max, 9).expect("a valid region generates seeds");
    assert_eq!(region.len(), 9, "the region lattice is capped at the count");
    for seed in &region {
        assert!(
            seed.cmpge(min).all() && seed.cmple(max).all(),
            "a region seed stays inside the requested box: {seed:?}"
        );
    }
    assert_eq!(
        region,
        region_seeds(&domain, min, max, 9).expect("the same region generates the same seeds"),
        "region generation is deterministic"
    );
    assert_eq!(
        region,
        region_seeds(&domain, min, max, 9).expect("repeating the region regenerates"),
        "region generation is stable across calls"
    );
}

#[test]
fn generators_reject_non_finite_degenerate_and_out_of_domain_input() {
    let domain = domain();
    let nan = Vec3::new(f32::NAN, 0.0, 0.0);

    assert!(matches!(
        line_seeds(&domain, nan, Vec3::X, 2),
        Err(SeedError::NonFinite(_))
    ));
    assert!(matches!(
        line_seeds(&domain, Vec3::ZERO, Vec3::ZERO, 2),
        Err(SeedError::Degenerate(_))
    ));
    assert!(matches!(
        line_seeds(&domain, -Vec3::X, Vec3::X, 0),
        Err(SeedError::ZeroCount)
    ));
    assert!(matches!(
        line_seeds(&domain, -Vec3::X, Vec3::X, 1),
        Err(SeedError::CountTooSmall { count: 1, min: 2 })
    ));
    assert!(matches!(
        line_seeds(&domain, -Vec3::X, Vec3::X, 65),
        Err(SeedError::CountTooLarge { count: 65, .. })
    ));
    assert!(matches!(
        line_seeds(&domain, Vec3::new(-10.0, 0.0, 0.0), Vec3::X, 2),
        Err(SeedError::OutsideDomain(_))
    ));

    assert!(matches!(
        circle_seeds(&domain, nan, PlaneAxis::Z, 1.0, 4),
        Err(SeedError::NonFinite(_))
    ));
    assert!(matches!(
        circle_seeds(&domain, Vec3::ZERO, PlaneAxis::Z, 0.0, 4),
        Err(SeedError::Degenerate(_))
    ));
    assert!(matches!(
        circle_seeds(&domain, Vec3::ZERO, PlaneAxis::Z, 1.0, 2),
        Err(SeedError::CountTooSmall { count: 2, min: 3 })
    ));
    assert!(matches!(
        circle_seeds(&domain, Vec3::new(10.0, 0.0, 0.0), PlaneAxis::Z, 1.0, 4),
        Err(SeedError::OutsideDomain(_))
    ));

    assert!(matches!(
        region_seeds(&domain, nan, Vec3::ZERO, 1),
        Err(SeedError::NonFinite(_))
    ));
    assert!(matches!(
        region_seeds(&domain, Vec3::ZERO, Vec3::ZERO, 1),
        Err(SeedError::Degenerate(_))
    ));
    assert!(matches!(
        region_seeds(&domain, Vec3::X, -Vec3::X, 4),
        Err(SeedError::Degenerate(_))
    ));
    assert!(matches!(
        region_seeds(
            &domain,
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(11.0, 1.0, 1.0),
            4
        ),
        Err(SeedError::OutsideDomain(_))
    ));
    assert!(matches!(
        region_seeds(&domain, Vec3::ZERO, Vec3::ONE, 0),
        Err(SeedError::ZeroCount)
    ));
}

#[test]
fn over_cap_applications_are_rejected_in_full() {
    let mut state = State::new(false);
    let domain = state.scene_domain();
    let center = domain.center();
    let mut layout = SeedLayout::default();
    for _ in 0..(MAX_SEEDS - 6) {
        layout
            .push(
                SeedPoint {
                    position: center,
                    anchor: None,
                },
                &domain,
            )
            .expect("building a nearly full layout stays valid");
    }
    state
        .commit_seed_layout(layout.clone())
        .expect("the nearly full layout commits");
    state.dirty = false;

    let generated = vec![center; 7];
    let error = state
        .apply_generated_seeds(&generated)
        .expect_err("an application above the layout cap is rejected");
    assert!(
        matches!(error, SeedError::LayoutFull { .. }),
        "the rejection names the layout cap: {error}"
    );
    assert_eq!(
        state.seed_layout, layout,
        "a rejected application changes nothing"
    );
    assert!(!state.dirty, "a rejected application records no change");

    let too_many = vec![center; MAX_GENERATED_PER_APPLICATION + 1];
    let error = state
        .apply_generated_seeds(&too_many)
        .expect_err("an application above the per-application cap is rejected");
    assert!(
        matches!(error, SeedError::CountTooLarge { .. }),
        "the rejection names the per-application cap: {error}"
    );
    assert_eq!(
        state.seed_layout, layout,
        "the over-cap application changes nothing"
    );

    let fits = vec![center; 6];
    state
        .apply_generated_seeds(&fits)
        .expect("an application that exactly fills the layout is accepted");
    assert_eq!(state.seed_layout.len(), MAX_SEEDS);
}

#[test]
fn explicit_seeds_replace_the_automatic_grid_and_clearing_restores_it() {
    let mut state = State::new(false);
    state.set_density(6);
    state.set_streamlines(true);
    let automatic = state.streamlines_snapshot(&[]);
    assert!(!automatic.is_empty(), "the automatic grid paints lines");

    let center = state.scene_domain().center();
    state
        .apply_generated_seeds(&[
            center + Vec3::new(-0.4, 0.0, 0.0),
            center + Vec3::new(0.4, 0.0, 0.0),
        ])
        .expect("two explicit seeds apply");
    let explicit = state.streamlines_snapshot(&[]);
    assert!(!explicit.is_empty(), "explicit seeds paint lines");
    assert_ne!(
        format!("{explicit:?}"),
        format!("{automatic:?}"),
        "explicit seeds replace the automatic jittered grid"
    );
    assert!(
        !state
            .click_streamlines_snapshot((0.0, 1.0), &[center])
            .is_empty(),
        "runtime click bursts still compose with an explicit layout"
    );

    state.set_seed_mode(vector_field_explorer::viewmodel::StreamlineSeedMode::Click);
    assert_eq!(
        format!("{:?}", state.streamlines_snapshot(&[])),
        format!("{explicit:?}"),
        "Click mode composes with an explicit layout instead of hiding it"
    );

    state.reset_seed_layout();
    assert!(
        state.streamlines_snapshot(&[]).is_empty(),
        "the empty layout under Click still paints no automatic grid"
    );

    state.set_seed_mode(vector_field_explorer::viewmodel::StreamlineSeedMode::Auto);
    assert_eq!(
        format!("{:?}", state.streamlines_snapshot(&[])),
        format!("{automatic:?}"),
        "clearing restores the automatic grid exactly"
    );
}

#[test]
fn seed_editing_preserves_the_plane_anchor_across_a_plane_switch() {
    let mut state = State::new(false);
    state.set_formula("x, y, z".into());
    // An odd count puts the middle z slice exactly at the scene plane.
    state.set_density(9);
    state.set_plane_axis(PlaneAxis::Z);
    state.set_plane_index(4);
    let frame = state
        .plane_frame()
        .expect("a 3D field in 2D is a plane slice");
    let position = frame.to_world(Vec3::new(0.3, -0.2, 0.0));
    let point = state.seed_point_on_current_plane(position);
    assert_eq!(
        point.anchor,
        Some(SeedAnchor {
            axis: PlaneAxis::Z,
            index: 4,
            u: frame.to_local(position).x,
            v: frame.to_local(position).y,
        })
    );
    state
        .apply_generated_seeds(&[position])
        .expect("the anchored seed applies");
    let stored = state.seed_layout().seeds.clone();

    state.set_plane_axis(PlaneAxis::X);
    state.set_plane_index(2);
    assert_eq!(
        state.seed_layout().seeds,
        stored,
        "switching the editing plane never moves stored seeds"
    );

    let anchor = stored[0].anchor.expect("the seed kept its anchor");
    let old_frame = PlaneFrame::new(
        &state.domain,
        PlaneView {
            enabled: true,
            axis: anchor.axis,
            index: anchor.index,
        },
        state.plane_grid(),
    );
    let recovered = old_frame.to_world(Vec3::new(anchor.u, anchor.v, 0.0));
    assert!(
        (recovered - position).length() <= 1e-5,
        "the stored anchor still reproduces the planted position: {recovered:?} vs {position:?}"
    );
}

#[test]
fn clamp_seed_local_keeps_a_dragged_seed_inside_the_plane() {
    let domain = domain();
    let frame = PlaneFrame::new(
        &domain,
        PlaneView {
            enabled: true,
            axis: PlaneAxis::Z,
            index: 0,
        },
        UVec3::new(8, 8, 8),
    );
    let clamped = clamp_seed_local(&frame, Vec3::new(100.0, -100.0, 4.0));
    assert!(
        clamped.x.abs() <= frame.half_u + 1e-5,
        "the drag clamps u into the plane: {clamped:?}"
    );
    assert!(
        clamped.y.abs() <= frame.half_v + 1e-5,
        "the drag clamps v into the plane: {clamped:?}"
    );
    assert_eq!(clamped.z, 0.0, "a plane drag stays on the plane");
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
