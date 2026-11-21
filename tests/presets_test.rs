//! Built-in field-preset coverage (ticket 31).
//!
//! Moved out of `src/presets.rs`'s inline module and extended for the
//! categorized physics presets: every formula must parse, match its declared
//! dimension, stay finite on the default domain, and appear under its
//! declared category in the declared order.

use std::collections::HashSet;

use glam::{UVec3, Vec3};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::field::FieldExpr;
use vector_field_explorer::presets::{PresetCategory, START_HERE, presets, presets_in};

const REQUIRED: [&str; 31] = [
    // Classic (pre-existing)
    "Pump",
    "Vortex",
    "Shear flow",
    "Curl example",
    "No-curl example",
    "3D radial",
    "3D swirl",
    "3D helix",
    // Electrostatics (pre-existing + ticket 31)
    "Dipole",
    "3D Dipole",
    "Source and sink",
    "Point charge",
    "Like charges",
    "Quadrupole",
    // Magnetostatics (ticket 31)
    "Straight wire",
    "Magnetic dipole",
    "Solenoid",
    // Electrodynamics (ticket 31)
    "Plane wave",
    "Circular wave",
    // Ticket 31 follow-up
    "Magnetic bottle",
    "Damped oscillator",
    // Ticket 91 slice A teaching catalogue
    "Pulsating radial field",
    "Rotating field",
    "Traveling wave field",
    "Standing wave field",
    "Quadrupole (unequal charges)",
    "Saddle potential",
    "Anisotropic well",
    "Double-well potential",
    "Softened dipole",
    "Moving Gaussian packet",
];

/// The ticket-91 formula entries driven by the shared clock.
const ANIMATED: [&str; 4] = [
    "Pulsating radial field",
    "Traveling wave field",
    "Standing wave field",
    "Moving Gaussian packet",
];

#[test]
fn every_preset_parses_with_declared_dimension() {
    assert!(!presets().is_empty(), "preset list must not be empty");
    for preset in presets() {
        let parsed = FieldExpr::parse(preset.formula).unwrap_or_else(|error| {
            panic!(
                "preset `{}` formula `{}` must parse: {error}",
                preset.name, preset.formula
            )
        });
        assert_eq!(
            parsed.dimension, preset.dimension,
            "preset `{}` declared dimension must match its formula",
            preset.name
        );
    }
}

#[test]
fn preset_names_are_unique() {
    let names: HashSet<&str> = presets().iter().map(|preset| preset.name).collect();
    assert_eq!(names.len(), presets().len(), "preset names must be unique");
}

#[test]
fn required_presets_present() {
    let names: HashSet<&str> = presets().iter().map(|preset| preset.name).collect();
    for required in REQUIRED {
        assert!(
            names.contains(required),
            "preset `{required}` must be listed"
        );
    }
}

#[test]
fn preset_count_includes_the_owner_added_3d_dipole() {
    assert_eq!(
        presets().len(),
        REQUIRED.len(),
        "the original 20 plus the 3D dipole"
    );
}

#[test]
fn categories_are_ordered_and_titled() {
    let titles: Vec<&str> = PresetCategory::ALL
        .iter()
        .map(|category| category.title())
        .collect();
    assert_eq!(
        titles,
        [
            "Classic",
            "Electrostatics",
            "Magnetostatics",
            "Electrodynamics"
        ],
        "the panel renders categories in this declared order"
    );
    for category in PresetCategory::ALL {
        assert!(
            !category.title().is_empty(),
            "every category needs a section heading"
        );
        assert!(
            !category.description().is_empty(),
            "every category heading needs an explaining line"
        );
    }
}

#[test]
fn every_category_is_non_empty() {
    for category in PresetCategory::ALL {
        assert!(
            presets_in(category).next().is_some(),
            "category `{}` must list at least one preset",
            category.title()
        );
    }
}

#[test]
fn grouping_preserves_declared_category_order() {
    let mut grouped: Vec<&str> = Vec::new();
    for category in PresetCategory::ALL {
        grouped.extend(presets_in(category).map(|preset| preset.name));
    }
    let declared: Vec<&str> = presets().iter().map(|preset| preset.name).collect();
    assert_eq!(
        grouped, declared,
        "presets() must be ordered by category, declaration order within each"
    );
}

#[test]
fn new_physics_presets_have_the_declared_category_and_dimension() {
    let expected = [
        ("3D Dipole", PresetCategory::Electrostatics, 3),
        ("Point charge", PresetCategory::Electrostatics, 2),
        ("Like charges", PresetCategory::Electrostatics, 2),
        ("Quadrupole", PresetCategory::Electrostatics, 2),
        ("Straight wire", PresetCategory::Magnetostatics, 2),
        ("Magnetic dipole", PresetCategory::Magnetostatics, 3),
        ("Solenoid", PresetCategory::Magnetostatics, 3),
        ("Plane wave", PresetCategory::Electrodynamics, 3),
        ("Circular wave", PresetCategory::Electrodynamics, 3),
        ("Magnetic bottle", PresetCategory::Magnetostatics, 3),
        ("Damped oscillator", PresetCategory::Classic, 2),
    ];
    for (name, category, dimension) in expected {
        let preset = presets()
            .iter()
            .find(|preset| preset.name == name)
            .unwrap_or_else(|| panic!("preset `{name}` must be listed"));
        assert_eq!(preset.category, category, "preset `{name}` category");
        assert_eq!(preset.dimension, dimension, "preset `{name}` dimension");
    }
}

#[test]
fn every_preset_stays_finite_on_the_default_domain() {
    let domain = Domain::default();
    for preset in presets() {
        let field = FieldExpr::parse(preset.formula)
            .unwrap_or_else(|error| panic!("preset `{}` must parse: {error}", preset.name));
        for point in domain.grid_points(UVec3::new(11, 11, 11)) {
            let value = field.eval3(point).unwrap_or_else(|| {
                panic!("preset `{}` evaluated to NaN/Inf at {point:?}", preset.name)
            });
            assert!(
                value.is_finite()
                    && value.x.is_finite()
                    && value.y.is_finite()
                    && value.z.is_finite(),
                "preset `{}` must stay finite at {point:?}, got {value:?}",
                preset.name
            );
        }
    }
}

#[test]
fn softening_covers_the_origin() {
    // The singular-looking presets are sampled exactly at the origin, the
    // worst point for every softened denominator.
    for name in ["Point charge", "Straight wire", "Magnetic dipole"] {
        let preset = presets()
            .iter()
            .find(|preset| preset.name == name)
            .unwrap_or_else(|| panic!("preset `{name}` must be listed"));
        let field = FieldExpr::parse(preset.formula).expect("preset parses");
        let value = field
            .eval3(Vec3::ZERO)
            .unwrap_or_else(|| panic!("preset `{name}` must be finite at the origin"));
        assert!(value.is_finite(), "preset `{name}` at origin: {value:?}");
    }
}
#[test]
fn start_here_entries_resolve() {
    assert_eq!(
        START_HERE.len(),
        3,
        "START_HERE must name exactly three presets"
    );
    let names: HashSet<&str> = presets().iter().map(|preset| preset.name).collect();
    let mut seen = HashSet::new();
    for name in START_HERE {
        assert!(
            names.contains(name),
            "START_HERE entry `{name}` must resolve in presets()"
        );
        assert!(seen.insert(name), "START_HERE entries must be distinct");
    }
}

fn eval_named(name: &str, point: Vec3, t: f32) -> Vec3 {
    let preset = presets()
        .iter()
        .find(|preset| preset.name == name)
        .unwrap_or_else(|| panic!("preset `{name}` must be listed"));
    let field = FieldExpr::parse(preset.formula)
        .unwrap_or_else(|error| panic!("preset `{name}` must parse: {error}"));
    field
        .eval3_at(point, t)
        .unwrap_or_else(|| panic!("preset `{name}` must be finite at {point:?}, t={t}"))
}

fn assert_close(actual: Vec3, expected: Vec3, label: &str) {
    assert!(
        (actual - expected).length() < 1e-5,
        "{label}: expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn new_teaching_presets_have_representative_values() {
    // Static fields are defined by their value at a stated point.
    assert_close(
        eval_named("Rotating field", Vec3::new(1.0, 0.0, 0.0), 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        "rotating field at (1, 0)",
    );
    assert_close(
        eval_named(
            "Quadrupole (unequal charges)",
            Vec3::new(1.0, 2.0, 0.0),
            0.0,
        ),
        Vec3::new(1.0, -2.0, 0.0),
        "quadrupole at (1, 2)",
    );
    assert_close(
        eval_named("Saddle potential", Vec3::new(1.0, 2.0, 0.0), 0.0),
        Vec3::new(1.0, -2.0, 0.0),
        "saddle at (1, 2)",
    );
    assert_close(
        eval_named("Anisotropic well", Vec3::new(1.0, 2.0, 0.0), 0.0),
        Vec3::new(-1.0, -8.0, 0.0),
        "anisotropic well at (1, 2)",
    );
    assert_close(
        eval_named("Double-well potential", Vec3::new(0.5, 2.0, 0.0), 0.0),
        Vec3::new(1.5, -2.0, 0.0),
        "double well at (0.5, 2)",
    );

    // Genuine regularized dipole: +1 at x = 1.25 and -1 at x = 0.75, each
    // softened with s^2 = 0.0625.
    //
    // At the origin, D+ = 1.25^2 + 0.0625 = 13/8 and
    // D- = 0.75^2 + 0.0625 = 5/8, so
    // fx = -20*sqrt(2)/(13*sqrt(13)) + 12*sqrt(2)/(5*sqrt(5))
    //    = 0.914459014917179...; fy = 0.
    assert_close(
        eval_named("Softened dipole", Vec3::ZERO, 0.0),
        Vec3::new(0.914_459, 0.0, 0.0),
        "softened dipole at the origin",
    );
    // At the midpoint x = 1 both centres contribute equally:
    // D+ = D- = 0.25^2 + 0.0625 = 1/8 and
    // fx = (-0.25 - 0.25) / (1/8)^1.5 = -0.5 / (1/8)^1.5 = -8*sqrt(2).
    assert_close(
        eval_named("Softened dipole", Vec3::new(1.0, 0.0, 0.0), 0.0),
        Vec3::new(-11.313_708, 0.0, 0.0),
        "softened dipole at the midpoint",
    );
    // Softening keeps both charge centres finite. At either centre only the
    // opposite charge contributes: fx = -0.5 / (0.3125)^1.5 = -32*sqrt(5)/25.
    for centre in [0.75, 1.25] {
        assert_close(
            eval_named("Softened dipole", Vec3::new(centre, 0.0, 0.0), 0.0),
            Vec3::new(-2.862_167, 0.0, 0.0),
            "softened dipole at a softened charge centre",
        );
    }
    // The two softened charges make the field symmetric outside the pair.
    assert_close(
        eval_named("Softened dipole", Vec3::new(2.0, 0.0, 0.0), 0.0),
        eval_named("Softened dipole", Vec3::ZERO, 0.0),
        "softened dipole symmetry about x = 1",
    );
}

#[test]
fn new_animated_presets_move_with_the_shared_clock() {
    // Traveling wave: sin(x - t) along x.
    assert_close(
        eval_named("Traveling wave field", Vec3::ZERO, 0.0),
        Vec3::ZERO,
        "traveling wave at x = 0, t = 0",
    );
    assert_close(
        eval_named(
            "Traveling wave field",
            Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            0.0,
        ),
        Vec3::X,
        "traveling wave at x = pi/2, t = 0",
    );
    assert_close(
        eval_named(
            "Traveling wave field",
            Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            std::f32::consts::FRAC_PI_2,
        ),
        Vec3::ZERO,
        "traveling wave at x = t = pi/2",
    );

    // Standing wave: sin(x) cos(t) along x.
    assert_close(
        eval_named(
            "Standing wave field",
            Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            0.0,
        ),
        Vec3::X,
        "standing wave antinode at t = 0",
    );
    assert_close(
        eval_named(
            "Standing wave field",
            Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            std::f32::consts::FRAC_PI_2,
        ),
        Vec3::ZERO,
        "standing wave node at t = pi/2",
    );

    // Pulsating radial field: (1 + 0.5 sin t) * (x, y).
    assert_close(
        eval_named("Pulsating radial field", Vec3::new(1.0, 2.0, 0.0), 0.0),
        Vec3::new(1.0, 2.0, 0.0),
        "pulsating radial at t = 0",
    );
    assert_close(
        eval_named(
            "Pulsating radial field",
            Vec3::new(1.0, 2.0, 0.0),
            std::f32::consts::FRAC_PI_2,
        ),
        Vec3::new(1.5, 3.0, 0.0),
        "pulsating radial at maximum",
    );

    // Moving Gaussian packet: centred at x = t.
    let packet = eval_named("Moving Gaussian packet", Vec3::new(1.0, 0.0, 0.0), 0.0);
    assert!(
        packet.x < 0.0,
        "packet points toward its centre: {packet:?}"
    );
    assert_close(
        eval_named("Moving Gaussian packet", Vec3::new(1.0, 0.0, 0.0), 1.0),
        Vec3::ZERO,
        "moving packet at its centre",
    );
}

#[test]
fn only_the_clock_driven_teaching_presets_reference_t() {
    for preset in presets() {
        let field = FieldExpr::parse(preset.formula)
            .unwrap_or_else(|error| panic!("preset `{}` must parse: {error}", preset.name));
        assert_eq!(
            field.is_time_dependent(),
            ANIMATED.contains(&preset.name),
            "preset `{}` time dependence",
            preset.name
        );
    }
}

#[test]
fn loading_a_new_animated_preset_leaves_unrelated_state_untouched() {
    use vector_field_explorer::source_scene::SourceScene;
    use vector_field_explorer::ui::presets::PresetPanel;
    use vector_field_explorer::viewmodel::State;

    let mut state = State::new(false);
    let before_sources = state.sources.clone();
    assert_eq!(before_sources, SourceScene::default());
    let mut panel = PresetPanel::default();
    assert!(panel.load_field("Traveling wave field", &mut state));
    assert_eq!(state.sources, before_sources, "formula load keeps sources");
    assert!(!state.mode_3d, "the preset declares a 2D field");
    assert_eq!(state.formula, "(sin(x-t), 0)");
    assert_eq!(state.field.dimension, 2);
}

#[test]
fn start_here_covers_two_and_three_d() {
    let dimensions: Vec<usize> = START_HERE
        .iter()
        .map(|name| {
            presets()
                .iter()
                .find(|preset| preset.name == *name)
                .unwrap_or_else(|| panic!("START_HERE entry `{name}` must resolve"))
                .dimension
        })
        .collect();
    assert!(
        dimensions.contains(&2),
        "START_HERE must include at least one 2D preset, got {dimensions:?}"
    );
    assert!(
        dimensions.contains(&3),
        "START_HERE must include at least one 3D preset, got {dimensions:?}"
    );
}
