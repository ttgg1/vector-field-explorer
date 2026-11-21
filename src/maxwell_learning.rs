//! Plain-text teaching statements for the four Maxwell equations.
//!
//! This is a pure data model: no solver, no sampling, no renderer and no
//! persistence. Each [`EquationSpec`] carries both the integral and the
//! differential statement in normalized teaching units, names every symbol it
//! uses, and states the limit in which the equation is being applied. The
//! catalogue is deliberately a teaching aid; none of the texts claim to solve
//! the coupled time-dependent Maxwell system.

/// One of the four Maxwell equations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaxwellEquation {
    GaussElectric,
    GaussMagnetic,
    Faraday,
    AmpereMaxwell,
}

impl MaxwellEquation {
    /// Every equation in teaching order.
    pub const ALL: [MaxwellEquation; 4] = [
        Self::GaussElectric,
        Self::GaussMagnetic,
        Self::Faraday,
        Self::AmpereMaxwell,
    ];

    /// Short human-readable name.
    pub fn title(self) -> &'static str {
        match self {
            Self::GaussElectric => "Gauss's law for electricity",
            Self::GaussMagnetic => "Gauss's law for magnetism",
            Self::Faraday => "Faraday's law of induction",
            Self::AmpereMaxwell => "Ampere-Maxwell law",
        }
    }
}

/// Which statement form is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquationForm {
    Integral,
    Differential,
}

/// The integral and differential text for one equation, plus its glossary.
///
/// `form` is the canonical form returned by [`spec`]. Both text fields are
/// always populated, so callers can show either form without re-querying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquationSpec {
    pub equation: MaxwellEquation,
    pub form: EquationForm,
    pub title: &'static str,
    pub integral: &'static str,
    pub differential: &'static str,
    pub symbols: &'static [(&'static str, &'static str)],
    pub assumptions: &'static str,
}

/// The complete teaching record for `equation`.
pub fn spec(equation: MaxwellEquation) -> EquationSpec {
    match equation {
        MaxwellEquation::GaussElectric => EquationSpec {
            equation,
            form: EquationForm::Integral,
            title: equation.title(),
            integral: "Closed surface S: surface integral of E dot dA equals Q_enc divided by eps0. Normalized teaching units; Q_enc is the enclosed charge and eps0 is the vacuum permittivity.",
            differential: "div E equals rho divided by eps0. Normalized teaching units; rho is the charge density.",
            symbols: &[
                ("E", "electric field"),
                ("S", "closed surface"),
                ("dA", "outward area element"),
                ("Q_enc", "enclosed charge"),
                ("eps0", "vacuum permittivity"),
                ("div E", "divergence of the electric field"),
                ("rho", "charge density"),
            ],
            assumptions: "Electrostatic limit: charges are at rest, so the field is steady. In these normalized teaching units eps0 = 1 and mu0 = 1, so the residual check compares div E with rho directly. This is a teaching statement, not a solver, and no time-dependent evolution is computed.",
        },
        MaxwellEquation::GaussMagnetic => EquationSpec {
            equation,
            form: EquationForm::Integral,
            title: equation.title(),
            integral: "Closed surface S: surface integral of B dot dA equals zero. Normalized teaching units; there is no magnetic charge.",
            differential: "div B equals zero. Normalized teaching units; the magnetic field is solenoidal.",
            symbols: &[
                ("B", "magnetic field"),
                ("S", "closed surface"),
                ("dA", "outward area element"),
                ("div B", "divergence of the magnetic field"),
            ],
            assumptions: "Magnetostatic limit: steady currents produce the field and there are no magnetic monopoles. This is a teaching statement, not a solver, and no time-dependent evolution is computed.",
        },
        MaxwellEquation::Faraday => EquationSpec {
            equation,
            form: EquationForm::Integral,
            title: equation.title(),
            integral: "Closed curve C: line integral of E dot dl equals minus dPhi_B/dt. Normalized teaching units; Phi_B is the magnetic flux through C.",
            differential: "curl E equals minus dB/dt. Normalized teaching units; dB/dt is the rate of change of B.",
            symbols: &[
                ("E", "electric field"),
                ("dl", "line element along the closed curve"),
                ("C", "closed curve bounding the surface"),
                ("Phi_B", "magnetic flux through the surface"),
                ("dPhi_B/dt", "rate of change of the magnetic flux"),
                ("curl E", "curl of the electric field"),
                ("B", "magnetic field"),
                ("dB/dt", "rate of change of the magnetic field"),
            ],
            assumptions: "Quasistatic to full time-dependent limit: induction is included, but this page does not solve the coupled field evolution. This is a teaching statement, not a solver.",
        },
        MaxwellEquation::AmpereMaxwell => EquationSpec {
            equation,
            form: EquationForm::Integral,
            title: equation.title(),
            integral: "Closed curve C: line integral of B dot dl equals mu0 times (I_enc plus eps0 times dPhi_E/dt). Normalized teaching units; the displacement term is shown explicitly.",
            differential: "curl B equals mu0 times j plus mu0 eps0 times dE/dt. Normalized teaching units; j is the current density and dE/dt is the displacement current density.",
            symbols: &[
                ("B", "magnetic field"),
                ("dl", "line element along the closed curve"),
                ("C", "closed curve bounding the surface"),
                ("mu0", "vacuum permeability"),
                ("I_enc", "enclosed current"),
                ("eps0", "vacuum permittivity"),
                ("Phi_E", "electric flux through the surface"),
                ("dPhi_E/dt", "rate of change of the electric flux"),
                ("curl B", "curl of the magnetic field"),
                ("j", "current density"),
                ("E", "electric field"),
                ("dE/dt", "rate of change of the electric field"),
            ],
            assumptions: "Magnetostatic, quasistatic or full time-dependent limit: the displacement current is retained only when the model supplies it. In these normalized teaching units eps0 = 1 and mu0 = 1, so the residual check compares curl B with j plus dE/dt directly. This is a teaching statement, not a solver, and no coupled time-dependent evolution is solved.",
        },
    }
}
