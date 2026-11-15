//! Illustrative dimensionless scalar examples, not physical solvers.

use crate::plot::PlotKind;

pub struct PlotExample {
    pub kind: PlotKind,
    pub name: &'static str,
    pub source: &'static str,
}

/// The charge examples use a dimensionless 0.1 regularizer, not a unit system.
pub const EXAMPLES: &[PlotExample] = &[
    PlotExample {
        kind: PlotKind::Curve2D,
        name: "Harmonic oscillator potential",
        source: "0.5*x^2",
    },
    PlotExample {
        kind: PlotKind::Curve2D,
        name: "Gaussian wave packet",
        source: "exp(-x^2)*cos(8*x)",
    },
    PlotExample {
        kind: PlotKind::Curve2D,
        name: "Damped oscillator",
        source: "exp(-0.2*abs(x))*sin(3*x)",
    },
    PlotExample {
        kind: PlotKind::Surface3D,
        name: "Point-charge potential",
        source: "1/(sqrt(x^2+y^2)+0.1)",
    },
    PlotExample {
        kind: PlotKind::Surface3D,
        name: "Two like charges",
        source: "1/(sqrt((x-0.6)^2+y^2)+0.1) + 1/(sqrt((x+0.6)^2+y^2)+0.1)",
    },
    PlotExample {
        kind: PlotKind::Surface3D,
        name: "Quantum oscillator ground state",
        source: "exp(-(x^2+y^2))",
    },
    PlotExample {
        kind: PlotKind::Surface3D,
        name: "Standing wave",
        source: "sin(3*x)*cos(3*y)",
    },
    PlotExample {
        kind: PlotKind::Surface3D,
        name: "Radial outgoing wave",
        source: "sin(6*sqrt(x^2+y^2))/(1+sqrt(x^2+y^2))",
    },
];
