//! Built-in preset vector fields.
//!
//! Every formula is written in the app's field syntax — a comma-separated
//! tuple of meval expressions — and must parse via
//! [`crate::field::FieldExpr::parse`]. The source and sink compositions
//! are softened with `+0.05` in each denominator so the field stays
//! finite across the whole default domain.

pub struct Preset {
    pub name: &'static str,
    pub formula: &'static str,
    pub dimension: usize,
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

pub fn presets() -> &'static [Preset] {
    &[
        Preset {
            name: "Pump",
            formula: "(x, y)",
            dimension: 2,
        },
        Preset {
            name: "Vortex",
            formula: "(-y, x)",
            dimension: 2,
        },
        Preset {
            name: "Dipole",
            formula: DIPOLE,
            dimension: 2,
        },
        Preset {
            name: "Shear flow",
            formula: "(1 - y*y, 0.05*x)",
            dimension: 2,
        },
        Preset {
            name: "Source and sink",
            formula: SOURCE_AND_SINK,
            dimension: 2,
        },
        Preset {
            name: "Curl example",
            formula: "(-y, x)",
            dimension: 2,
        },
        Preset {
            name: "No-curl example",
            formula: "(x, y)",
            dimension: 2,
        },
        Preset {
            name: "3D radial",
            formula: "(x, y, z)",
            dimension: 3,
        },
        Preset {
            name: "3D swirl",
            formula: "(-y, x, 0)",
            dimension: 3,
        },
        Preset {
            name: "3D helix",
            formula: "(-y, x, 0.5)",
            dimension: 3,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldExpr;
    use std::collections::HashSet;

    #[test]
    fn presets_nonempty_and_all_parse() {
        let list = presets();
        assert!(!list.is_empty(), "preset list must not be empty");
        for p in list {
            let parsed = FieldExpr::parse(p.formula).unwrap_or_else(|e| {
                panic!("preset `{}` formula `{}` must parse: {e}", p.name, p.formula)
            });
            assert_eq!(
                parsed.dimension, p.dimension,
                "preset `{}` declared dimension must match its formula",
                p.name
            );
        }
    }

    #[test]
    fn preset_names_are_unique() {
        let names: HashSet<&str> = presets().iter().map(|p| p.name).collect();
        assert_eq!(names.len(), presets().len(), "preset names must be unique");
    }

    #[test]
    fn required_presets_present() {
        let names: HashSet<&str> = presets().iter().map(|p| p.name).collect();
        for required in [
            "Pump",
            "Vortex",
            "Dipole",
            "Shear flow",
            "Source and sink",
            "Curl example",
            "No-curl example",
            "3D radial",
            "3D swirl",
            "3D helix",
        ] {
            assert!(names.contains(required), "preset `{required}` must be listed");
        }
    }
}
