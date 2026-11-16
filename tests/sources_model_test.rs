//! Task 1 model validation, preparation and exact-zero coverage.

use glam::DVec3;
use vector_field_explorer::sources::{
    AccuracyRequest, EvalOptions, Evaluator, FamilyResult, InputError, InvalidOption, PrepareError,
    Source, SourceKind, SourceParameter, SourceSet, StopReason, WorkLimitField, WorkLimits,
};

fn limits() -> WorkLimits {
    WorkLimits::default()
}

fn options() -> EvalOptions {
    EvalOptions::default()
}

fn source(center: DVec3, axis: DVec3, kind: SourceKind) -> Source {
    Source { center, axis, kind }
}

fn point(charge: f64) -> Source {
    source(DVec3::ZERO, DVec3::ZERO, SourceKind::PointCharge { charge })
}

fn wire(current: f64) -> Source {
    source(DVec3::ZERO, DVec3::Z, SourceKind::StraightWire { current })
}

fn assert_invalid(source: Source, parameter: SourceParameter) {
    let error = SourceSet::new(std::slice::from_ref(&source), &limits(), &|| false)
        .expect_err("source must be rejected");
    assert_eq!(
        error,
        InputError::InvalidSource {
            index: 0,
            parameter
        }
    );
}

fn all_valid_sources() -> Vec<Source> {
    vec![
        source(
            DVec3::ZERO,
            DVec3::ZERO,
            SourceKind::PointCharge { charge: -2.0 },
        ),
        source(
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::Z,
            SourceKind::FiniteLineCharge {
                charge: 1.0,
                length: 2.0,
            },
        ),
        source(
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::X,
            SourceKind::ChargedRing {
                charge: 1.0,
                radius: 0.5,
            },
        ),
        source(
            DVec3::new(0.0, 0.0, 1.0),
            DVec3::Y,
            SourceKind::ChargedDisc {
                charge: -1.0,
                radius: 0.5,
            },
        ),
        source(
            DVec3::new(2.0, 2.0, 2.0),
            DVec3::ZERO,
            SourceKind::SolidSphere {
                charge: 1.0,
                radius: 0.5,
            },
        ),
        source(
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 0.0, 2.0),
            SourceKind::StraightWire { current: -3.0 },
        ),
        source(
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::Z,
            SourceKind::CurrentLoop {
                current: 1.0,
                radius: 0.25,
            },
        ),
        source(
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::Z,
            SourceKind::Solenoid {
                current: 1.0,
                radius: 0.25,
                length: 2.0,
                turns: 3,
            },
        ),
    ]
}

#[test]
fn validation_is_strict() {
    let valid = all_valid_sources();
    let set = SourceSet::new(&valid, &limits(), &|| false).expect("all eight valid kinds");
    assert_eq!(set.len(), 8);

    for signed in [point(-1.0), point(0.0), wire(-2.0), wire(0.0)] {
        SourceSet::new(&[signed], &limits(), &|| false).expect("signed/zero strength is valid");
    }

    for base in &valid {
        for center in [
            DVec3::new(f64::NAN, 0.0, 0.0),
            DVec3::new(0.0, f64::INFINITY, 0.0),
        ] {
            assert_invalid(Source { center, ..*base }, SourceParameter::Center);
        }
        for axis in [
            DVec3::new(f64::NAN, 0.0, 0.0),
            DVec3::new(0.0, 0.0, f64::INFINITY),
        ] {
            assert_invalid(Source { axis, ..*base }, SourceParameter::Axis);
        }
    }

    for bad in [f64::NAN, f64::INFINITY] {
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::ZERO,
                SourceKind::PointCharge { charge: bad },
            ),
            SourceParameter::Charge,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::FiniteLineCharge {
                    charge: bad,
                    length: 2.0,
                },
            ),
            SourceParameter::Charge,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::FiniteLineCharge {
                    charge: 1.0,
                    length: bad,
                },
            ),
            SourceParameter::Length,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::ChargedRing {
                    charge: bad,
                    radius: 1.0,
                },
            ),
            SourceParameter::Charge,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::ChargedRing {
                    charge: 1.0,
                    radius: bad,
                },
            ),
            SourceParameter::Radius,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::ChargedDisc {
                    charge: bad,
                    radius: 1.0,
                },
            ),
            SourceParameter::Charge,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::ChargedDisc {
                    charge: 1.0,
                    radius: bad,
                },
            ),
            SourceParameter::Radius,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::ZERO,
                SourceKind::SolidSphere {
                    charge: bad,
                    radius: 1.0,
                },
            ),
            SourceParameter::Charge,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::ZERO,
                SourceKind::SolidSphere {
                    charge: 1.0,
                    radius: bad,
                },
            ),
            SourceParameter::Radius,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::StraightWire { current: bad },
            ),
            SourceParameter::Current,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::CurrentLoop {
                    current: bad,
                    radius: 1.0,
                },
            ),
            SourceParameter::Current,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::CurrentLoop {
                    current: 1.0,
                    radius: bad,
                },
            ),
            SourceParameter::Radius,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::Solenoid {
                    current: bad,
                    radius: 1.0,
                    length: 2.0,
                    turns: 2,
                },
            ),
            SourceParameter::Current,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::Solenoid {
                    current: 1.0,
                    radius: 1.0,
                    length: bad,
                    turns: 2,
                },
            ),
            SourceParameter::Length,
        );
    }

    for bad in [0.0, -1.0] {
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::FiniteLineCharge {
                    charge: 1.0,
                    length: bad,
                },
            ),
            SourceParameter::Length,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::ChargedRing {
                    charge: 1.0,
                    radius: bad,
                },
            ),
            SourceParameter::Radius,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::ChargedDisc {
                    charge: 1.0,
                    radius: bad,
                },
            ),
            SourceParameter::Radius,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::ZERO,
                SourceKind::SolidSphere {
                    charge: 1.0,
                    radius: bad,
                },
            ),
            SourceParameter::Radius,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::CurrentLoop {
                    current: 1.0,
                    radius: bad,
                },
            ),
            SourceParameter::Radius,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::Solenoid {
                    current: 1.0,
                    radius: 1.0,
                    length: bad,
                    turns: 2,
                },
            ),
            SourceParameter::Length,
        );
        assert_invalid(
            source(
                DVec3::ZERO,
                DVec3::Z,
                SourceKind::Solenoid {
                    current: 1.0,
                    radius: bad,
                    length: 2.0,
                    turns: 2,
                },
            ),
            SourceParameter::Radius,
        );
    }

    for kind in [
        SourceKind::FiniteLineCharge {
            charge: 1.0,
            length: 2.0,
        },
        SourceKind::ChargedRing {
            charge: 1.0,
            radius: 1.0,
        },
        SourceKind::ChargedDisc {
            charge: 1.0,
            radius: 1.0,
        },
        SourceKind::StraightWire { current: 1.0 },
        SourceKind::CurrentLoop {
            current: 1.0,
            radius: 1.0,
        },
        SourceKind::Solenoid {
            current: 1.0,
            radius: 1.0,
            length: 2.0,
            turns: 2,
        },
    ] {
        assert_invalid(
            source(DVec3::ZERO, DVec3::ZERO, kind),
            SourceParameter::Axis,
        );
    }

    // Point and sphere orientation is immaterial; a finite zero axis is valid.
    SourceSet::new(
        &[
            source(
                DVec3::ZERO,
                DVec3::ZERO,
                SourceKind::PointCharge { charge: 1.0 },
            ),
            source(
                DVec3::ZERO,
                DVec3::ZERO,
                SourceKind::SolidSphere {
                    charge: 1.0,
                    radius: 1.0,
                },
            ),
        ],
        &limits(),
        &|| false,
    )
    .expect("zero axis valid for point/sphere");

    assert_invalid(
        source(
            DVec3::ZERO,
            DVec3::Z,
            SourceKind::Solenoid {
                current: 1.0,
                radius: 1.0,
                length: 2.0,
                turns: 0,
            },
        ),
        SourceParameter::Turns,
    );

    let sixty_four = vec![point(1.0); 64];
    SourceSet::new(&sixty_four, &limits(), &|| false).expect("64 sources accepted");
    let sixty_five = vec![point(1.0); 65];
    assert_eq!(
        SourceSet::new(&sixty_five, &limits(), &|| false).expect_err("65 rejected"),
        InputError::SourceLimit {
            requested: 65,
            max: 64
        }
    );

    let zero_limits = [
        (
            WorkLimitField::MaxSources,
            WorkLimits {
                max_sources: 0,
                ..limits()
            },
        ),
        (
            WorkLimitField::WorkPerFamily,
            WorkLimits {
                work_per_family: 0,
                ..limits()
            },
        ),
        (
            WorkLimitField::CellsPerFamily,
            WorkLimits {
                cells_per_family: 0,
                ..limits()
            },
        ),
        (
            WorkLimitField::MaxDepth,
            WorkLimits {
                max_depth: 0,
                ..limits()
            },
        ),
        (
            WorkLimitField::MaxQueries,
            WorkLimits {
                max_queries: 0,
                ..limits()
            },
        ),
    ];
    let above_limits = [
        (
            WorkLimitField::MaxSources,
            WorkLimits {
                max_sources: 65,
                ..limits()
            },
        ),
        (
            WorkLimitField::WorkPerFamily,
            WorkLimits {
                work_per_family: 2_000_001,
                ..limits()
            },
        ),
        (
            WorkLimitField::CellsPerFamily,
            WorkLimits {
                cells_per_family: 8_193,
                ..limits()
            },
        ),
        (
            WorkLimitField::MaxDepth,
            WorkLimits {
                max_depth: 49,
                ..limits()
            },
        ),
        (
            WorkLimitField::MaxQueries,
            WorkLimits {
                max_queries: 65_537,
                ..limits()
            },
        ),
    ];
    for (field, bad_limits) in zero_limits.into_iter().chain(above_limits) {
        assert_eq!(
            SourceSet::new(&[], &bad_limits, &|| false).expect_err("bad limits rejected"),
            InputError::InvalidLimits(field)
        );
    }

    let set = SourceSet::new(&[], &limits(), &|| false).expect("empty set");
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut bad_options = options();
        bad_options.softening = bad;
        assert_eq!(
            Evaluator::new(&set, bad_options, &|| false).expect_err("bad softening"),
            PrepareError::InvalidOptions(InvalidOption::Softening)
        );

        for (field, accuracy) in [
            (
                InvalidOption::Epsilon,
                AccuracyRequest {
                    epsilon: bad,
                    ..options().accuracy
                },
            ),
            (
                InvalidOption::ElectricScale,
                AccuracyRequest {
                    e_scale: bad,
                    ..options().accuracy
                },
            ),
            (
                InvalidOption::MagneticScale,
                AccuracyRequest {
                    b_scale: bad,
                    ..options().accuracy
                },
            ),
        ] {
            let mut bad_options = options();
            bad_options.accuracy = accuracy;
            assert_eq!(
                Evaluator::new(&set, bad_options, &|| false).expect_err("bad accuracy"),
                PrepareError::InvalidOptions(field)
            );
        }
    }

    // The evaluator revalidates its own limits as options.
    let mut bad_options = options();
    bad_options.limits.max_queries = 0;
    assert_eq!(
        Evaluator::new(&set, bad_options, &|| false).expect_err("bad option limits"),
        PrepareError::InvalidOptions(InvalidOption::Limits(WorkLimitField::MaxQueries))
    );
}

#[test]
fn prepare_is_cancellable_before_allocation() {
    assert_eq!(
        SourceSet::new(&[point(1.0)], &limits(), &|| true).expect_err("source preparation"),
        InputError::Cancelled
    );

    let set = SourceSet::new(&[point(1.0)], &limits(), &|| false).expect("prepared set");
    assert_eq!(
        Evaluator::new(&set, options(), &|| true).expect_err("evaluator preparation"),
        PrepareError::Cancelled
    );

    let mut evaluator = Evaluator::new(&set, options(), &|| false).expect("evaluator");
    let sample = evaluator.evaluate(DVec3::new(1.0, 0.0, 0.0), &|| true);
    assert!(matches!(
        sample.electric,
        FamilyResult::Unavailable(StopReason::Cancelled)
    ));
    assert!(matches!(
        sample.magnetic,
        FamilyResult::Unavailable(StopReason::Cancelled)
    ));
}

fn expect_exact_zero(result: FamilyResult) {
    let FamilyResult::TargetMet(estimate) = result else {
        panic!("expected TargetMet exact zero, got {result:?}");
    };
    assert_eq!(estimate.value, DVec3::ZERO);
    assert!(estimate.uncertainty.is_finite());
    assert_eq!(estimate.uncertainty, 0.0);
    assert!(estimate.target > 0.0);
    assert_eq!(estimate.absolute_contributions, 0.0);
    assert!(!estimate.diagnostics.underflow);
}

#[test]
fn empty_and_zero_are_exact() {
    let mut empty = {
        let set = SourceSet::new(&[], &limits(), &|| false).expect("empty set");
        Evaluator::new(&set, options(), &|| false).expect("evaluator")
    };
    let sample = empty.evaluate(DVec3::new(3.0, -2.0, 7.0), &|| false);
    expect_exact_zero(sample.electric);
    expect_exact_zero(sample.magnetic);

    let zero_sources = vec![
        point(0.0),
        source(
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::Z,
            SourceKind::FiniteLineCharge {
                charge: 0.0,
                length: 2.0,
            },
        ),
        source(
            DVec3::ZERO,
            DVec3::Z,
            SourceKind::ChargedRing {
                charge: 0.0,
                radius: 1.0,
            },
        ),
        source(
            DVec3::ZERO,
            DVec3::Z,
            SourceKind::ChargedDisc {
                charge: 0.0,
                radius: 1.0,
            },
        ),
        source(
            DVec3::ZERO,
            DVec3::ZERO,
            SourceKind::SolidSphere {
                charge: 0.0,
                radius: 1.0,
            },
        ),
        wire(0.0),
        source(
            DVec3::ZERO,
            DVec3::Z,
            SourceKind::CurrentLoop {
                current: 0.0,
                radius: 1.0,
            },
        ),
        source(
            DVec3::ZERO,
            DVec3::Z,
            SourceKind::Solenoid {
                current: 0.0,
                radius: 1.0,
                length: 2.0,
                turns: 3,
            },
        ),
    ];
    let set = SourceSet::new(&zero_sources, &limits(), &|| false).expect("zero sources");
    let mut evaluator = Evaluator::new(&set, options(), &|| false).expect("evaluator");
    let sample = evaluator.evaluate(DVec3::new(0.25, 0.5, -0.75), &|| false);
    expect_exact_zero(sample.electric);
    expect_exact_zero(sample.magnetic);
}

#[test]
fn axis_direction_is_not_squared_norm_limited() {
    for axis in [
        DVec3::new(1e308, 0.0, 0.0),
        DVec3::new(f64::from_bits(1), 0.0, 0.0),
        DVec3::new(-f64::from_bits(1), 0.0, 0.0),
    ] {
        let source = source(DVec3::ZERO, axis, SourceKind::StraightWire { current: 1.0 });
        let set = SourceSet::new(&[source], &limits(), &|| false).expect("wire set");
        let mut evaluator = Evaluator::new(&set, options(), &|| false).expect("evaluator");
        let sample = evaluator.evaluate(DVec3::new(0.0, 1.0, 0.0), &|| false);
        let FamilyResult::TargetMet(estimate) = sample.magnetic else {
            panic!("expected wire TargetMet, got {:?}", sample.magnetic);
        };
        let expected = if axis.x.is_sign_negative() {
            DVec3::new(0.0, 0.0, -2.0 / 1.01)
        } else {
            DVec3::new(0.0, 0.0, 2.0 / 1.01)
        };
        assert!(
            (estimate.value - expected).length() <= 2e-15,
            "axis {axis:?} gave {:?}, expected {expected:?}",
            estimate.value
        );
    }
}
