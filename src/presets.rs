//! Built-in preset vector fields, grouped by physical setting.
//!
//! Every formula is written in the app's field syntax — a comma-separated
//! tuple of meval expressions — and must parse via
//! [`crate::field::FieldExpr::parse`]. The singular-looking fields (point
//! charges, wires, dipoles) are softened with `+0.05` in each denominator so
//! they stay finite across the whole default domain; the Coulomb/dipole
//! numerators are not normalised, so magnitudes are illustrative rather than
//! physical.
//!
//! The list is ordered by [`PresetCategory`] in [`PresetCategory::ALL`] order
//! and, within a category, in declaration order. [`presets_in`] is the
//! grouping seam the Presets panel renders, so the panel and the tests agree
//! on both order and membership.

/// The physical setting a preset belongs to; the Presets panel uses these as
/// section headings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresetCategory {
    /// Kinematic demos and generic 2D/3D flows.
    Classic,
    /// Coulomb-like fields from static charges.
    Electrostatics,
    /// Fields around steady currents.
    Magnetostatics,
    /// Static spatial sinusoids: the field oscillates in space, but v1 has no
    /// time variable, so these are snapshots of a wave rather than a moving one.
    Electrodynamics,
}

impl PresetCategory {
    /// The categories in panel order.
    pub const ALL: [PresetCategory; 4] = [
        PresetCategory::Classic,
        PresetCategory::Electrostatics,
        PresetCategory::Magnetostatics,
        PresetCategory::Electrodynamics,
    ];

    /// Section heading text.
    pub fn title(self) -> &'static str {
        match self {
            PresetCategory::Classic => "Classic",
            PresetCategory::Electrostatics => "Electrostatics",
            PresetCategory::Magnetostatics => "Magnetostatics",
            PresetCategory::Electrodynamics => "Electrodynamics",
        }
    }

    /// One-line explanation shown as the heading's tooltip.
    pub fn description(self) -> &'static str {
        match self {
            PresetCategory::Classic => "Kinematic flows: pumps, vortices, shear and 3D demos",
            PresetCategory::Electrostatics => {
                "Coulomb-like fields from static point charges (softened at the source)"
            }
            PresetCategory::Magnetostatics => "Fields around steady currents: wires, dipoles, solenoids",
            PresetCategory::Electrodynamics => {
                "Static spatial sinusoids: the field oscillates in space (no time variable in v1)"
            }
        }
    }
}

pub struct Preset {
    pub name: &'static str,
    pub formula: &'static str,
    pub dimension: usize,
    pub category: PresetCategory,
}

/// Source at (-0.5, 0.1) and sink at (+0.5, 0.1): flow leaves the first
/// point and converges on the second. Both terms are radial outflow
/// forms, each with the same center point in numerator and denominator;
/// the sink term is negated so flow points into it.
const SOURCE_AND_SINK: &str = "((x+0.5)/((x+0.5)^2+(y-0.1)^2+0.05) - (x-0.5)/((x-0.5)^2+(y-0.1)^2+0.05),\
 (y-0.1)/((x+0.5)^2+(y-0.1)^2+0.05) - (y-0.1)/((x-0.5)^2+(y-0.1)^2+0.05))";

/// Dipole: source at (0.5, 0.1) minus the mirrored charge at (-0.5, 0.1).
const DIPOLE: &str = "((x-0.5)/((x-0.5)^2+(y-0.1)^2+0.05) - (x+0.5)/((x+0.5)^2+(y-0.1)^2+0.05),\
 (y-0.1)/((x-0.5)^2+(y-0.1)^2+0.05) - (y-0.1)/((x+0.5)^2+(y-0.1)^2+0.05))";

/// Two equal positive charges at (-0.5, 0) and (+0.5, 0): the superposition
/// of two softened radial outflows, so field lines leave both centres and
/// bend away from the midpoint between them.
const LIKE_CHARGES: &str = "((x+0.5)/((x+0.5)^2+y^2+0.05) + (x-0.5)/((x-0.5)^2+y^2+0.05),\
 y/((x+0.5)^2+y^2+0.05) + y/((x-0.5)^2+y^2+0.05))";

/// Magnetic dipole: the point-dipole field of a current loop, softened with
/// `d = x^2+y^2+z^2+0.05` so the origin stays finite.
const MAGNETIC_DIPOLE: &str = "(3*x*z/(x^2+y^2+z^2+0.05)^2.5, 3*y*z/(x^2+y^2+z^2+0.05)^2.5,\
 (3*z^2-(x^2+y^2+z^2))/(x^2+y^2+z^2+0.05)^2.5)";

pub const START_HERE: [&str; 3] = ["Vortex", "3D swirl", "Dipole"];

pub fn presets() -> &'static [Preset] {
    let all = &[
        // Classic: kinematic demos and generic flows.
        Preset {
            name: "Pump",
            formula: "(x, y)",
            dimension: 2,
            category: PresetCategory::Classic,
        },
        Preset {
            name: "Vortex",
            formula: "(-y, x)",
            dimension: 2,
            category: PresetCategory::Classic,
        },
        Preset {
            name: "Shear flow",
            formula: "(1 - y*y, 0.05*x)",
            dimension: 2,
            category: PresetCategory::Classic,
        },
        Preset {
            name: "Curl example",
            formula: "(-y, x)",
            dimension: 2,
            category: PresetCategory::Classic,
        },
        Preset {
            name: "No-curl example",
            formula: "(x, y)",
            dimension: 2,
            category: PresetCategory::Classic,
        },
        Preset {
            name: "3D radial",
            formula: "(x, y, z)",
            dimension: 3,
            category: PresetCategory::Classic,
        },
        Preset {
            name: "3D swirl",
            formula: "(-y, x, 0)",
            dimension: 3,
            category: PresetCategory::Classic,
        },
        Preset {
            name: "3D helix",
            formula: "(-y, x, 0.5)",
            dimension: 3,
            category: PresetCategory::Classic,
        },
        // Inward spiral with linear damping: a spiral sink with eigenvalues -0.3 ± i.
        Preset {
            name: "Damped oscillator",
            formula: "(-0.3*x - y, x - 0.3*y)",
            dimension: 2,
            category: PresetCategory::Classic,
        },
        // Electrostatics: softened Coulomb fields from static charges.
        Preset {
            name: "Dipole",
            formula: DIPOLE,
            dimension: 2,
            category: PresetCategory::Electrostatics,
        },
        Preset {
            name: "Source and sink",
            formula: SOURCE_AND_SINK,
            dimension: 2,
            category: PresetCategory::Electrostatics,
        },
        // A single positive point charge at the origin: radial Coulomb field.
        Preset {
            name: "Point charge",
            formula: "(x/(x^2+y^2+0.05), y/(x^2+y^2+0.05))",
            dimension: 2,
            category: PresetCategory::Electrostatics,
        },
        // Two equal positive charges at (±0.5, 0): field lines repel at the
        // midpoint.
        Preset {
            name: "Like charges",
            formula: LIKE_CHARGES,
            dimension: 2,
            category: PresetCategory::Electrostatics,
        },
        // Linear saddle field, -grad(x^2 - y^2) = (-2x, 2y): a saddle potential
        // rather than a literal point quadrupole.
        Preset {
            name: "Quadrupole",
            formula: "(-2*x, 2*y)",
            dimension: 2,
            category: PresetCategory::Electrostatics,
        },
        // Magnetostatics: fields of steady currents.
        // Azimuthal field around a straight current along z.
        Preset {
            name: "Straight wire",
            formula: "(-y/(x^2+y^2+0.05), x/(x^2+y^2+0.05))",
            dimension: 2,
            category: PresetCategory::Magnetostatics,
        },
        // Point dipole of a current loop, softened at the origin.
        Preset {
            name: "Magnetic dipole",
            formula: MAGNETIC_DIPOLE,
            dimension: 3,
            category: PresetCategory::Magnetostatics,
        },
        // Solenoid-like field: azimuthal winding around the axis plus a uniform
        // axial component everywhere (a simplified coil with no end effects).
        Preset {
            name: "Solenoid",
            formula: "(-y/(x^2+y^2+0.05), x/(x^2+y^2+0.05), 0.5)",
            dimension: 3,
            category: PresetCategory::Magnetostatics,
        },
        // Axisymmetric converging field with zero divergence (paraxial magnetic mirror).
        Preset {
            name: "Magnetic bottle",
            formula: "(-x, -y, 2*z)",
            dimension: 3,
            category: PresetCategory::Magnetostatics,
        },
        // Electrodynamics: static snapshots of sinusoidally varying fields (v1
        // has no time variable). Plane-wave snapshot varying along x, polarised
        // along y.
        Preset {
            name: "Plane wave",
            formula: "(0, sin(x), 0)",
            dimension: 3,
            category: PresetCategory::Electrodynamics,
        },
        // Circularly polarised snapshot along x: y and z components are 90° out
        // of phase.
        Preset {
            name: "Circular wave",
            formula: "(0, sin(x), cos(x))",
            dimension: 3,
            category: PresetCategory::Electrodynamics,
        },
    ];
    #[cfg(debug_assertions)]
    {
        let mut seen = std::collections::HashSet::<&str>::new();
        for name in START_HERE {
            debug_assert!(
                all.iter().any(|preset| preset.name == name),
                "START_HERE entry `{name}` must resolve in presets()"
            );
            debug_assert!(seen.insert(name), "START_HERE entries must be distinct");
        }
    }
    all
}

/// The presets in one category, in declaration order. This is the grouping
/// seam the Presets panel iterates, so panel order matches [`presets`].
pub fn presets_in(category: PresetCategory) -> impl Iterator<Item = &'static Preset> {
    presets()
        .iter()
        .filter(move |preset| preset.category == category)
}
