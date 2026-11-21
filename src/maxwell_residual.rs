//! Residual diagnostics for the four Maxwell equations at one sample point.
//!
//! Callers supply already-evaluated field terms in normalized teaching units.
//! A missing term is reported as [`TermStatus::Unsupported`], never replaced by
//! a fabricated zero; a non-finite term is [`TermStatus::Singular`], never
//! propagated as NaN. This module checks one sample point only and never claims
//! to solve the coupled time-dependent Maxwell system.
//!
//! Status precedence is deliberate and observable: a missing input is reported
//! as `Unsupported` before any non-finite input is considered `Singular`, and
//! within one equation the left term is checked before the right term. So a
//! Gauss electric sample with `div_e = NaN` and `rho` missing reports
//! `Unsupported("rho")`, not `Singular("div_e")`.
//!
//! Both reported values are scaled, never raw: `vector_residual` divides every
//! term by `max(1, |largest component|)` before adding or subtracting, so it
//! stays finite for any finite input even when the raw sum would overflow.
//! `TermStatus::Numerical` is reserved for slice B sampling estimates; this
//! slice reports only `Analytic`, `Unsupported` and `Singular`, and the 1.0
//! normalization floor never underflows the residual to a zero that would need
//! a `Numerical` estimate.

use crate::maxwell_learning::MaxwellEquation;

/// Provenance of one evaluated term or of the whole residual check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TermStatus {
    /// Exact for the selected teaching case.
    Analytic,
    /// Numerical estimate with a reported absolute error.
    ///
    /// Reserved for slice B sampling estimates; the current one-sample-point
    /// check never reports it.
    Numerical { error: f64 },
    /// A term the active mode cannot supply, named explicitly.
    Unsupported(&'static str),
    /// A pole, interface or otherwise non-finite sample, named explicitly.
    Singular(&'static str),
}

/// One sample point of field terms, in normalized teaching units.
///
/// Every term is optional so that an unavailable input stays distinguishable
/// from a genuine zero.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FieldTerms {
    pub div_e: Option<f64>,
    pub div_b: Option<f64>,
    pub curl_e: Option<[f64; 3]>,
    pub curl_b: Option<[f64; 3]>,
    pub rho: Option<f64>,
    pub j: Option<[f64; 3]>,
    pub de_dt: Option<[f64; 3]>,
    pub db_dt: Option<[f64; 3]>,
}

/// The outcome of checking one equation at one sample point.
#[derive(Debug, Clone, PartialEq)]
pub struct Residual {
    pub equation: MaxwellEquation,
    /// Scalar left-hand side (divergence equations only).
    pub lhs: Option<f64>,
    /// Scalar right-hand side (divergence equations only).
    pub rhs: Option<f64>,
    /// Scaled vector left-minus-right residual (curl equations only), divided
    /// by `max(1, |largest supplied component|)` so it stays finite.
    pub vector_residual: Option<[f64; 3]>,
    /// `|residual| / max(1, |lhs|, |rhs|)`, or the vector-magnitude analogue.
    pub normalized: Option<f64>,
    pub status: TermStatus,
    pub explanation: String,
}

/// Note appended to every explanation; the module never claims a full solve.
const SOLVER_NOTE: &str = "This diagnostic evaluates one sample point in normalized teaching units and does not solve the coupled time-dependent Maxwell system.";

/// Evaluate `equation` from the supplied terms at one sample point.
///
/// Missing inputs keep the residual unavailable instead of contributing zero.
/// Missing inputs are checked before non-finite inputs and left before right,
/// so `Unsupported` wins over `Singular` (see the module docs). Any non-finite
/// input marks the check singular, with no value reported.
pub fn residual(equation: MaxwellEquation, terms: &FieldTerms) -> Residual {
    match equation {
        MaxwellEquation::GaussElectric => scalar_residual(
            equation,
            terms.div_e,
            "div_e",
            terms.rho,
            "rho",
            "the divergence of E equals rho divided by eps0 (eps0 = 1 in these units)",
        ),
        MaxwellEquation::GaussMagnetic => {
            let Some(div_b) = terms.div_b else {
                return unsupported(equation, "div_b");
            };
            if !div_b.is_finite() {
                return singular(equation, "div_b");
            }
            let normalized = normalized_scalar(div_b, 0.0);
            Residual {
                equation,
                lhs: Some(div_b),
                rhs: Some(0.0),
                vector_residual: None,
                normalized: Some(normalized),
                status: TermStatus::Analytic,
                explanation: format!(
                    "{}: div B is compared with an explicit zero right-hand side. This magnetic Gauss law always keeps rhs = 0. {SOLVER_NOTE}",
                    equation.title()
                ),
            }
        }
        MaxwellEquation::Faraday => {
            let Some(curl_e) = terms.curl_e else {
                return unsupported(equation, "curl_e");
            };
            let Some(db_dt) = terms.db_dt else {
                return unsupported(equation, "db_dt");
            };
            if !finite3(curl_e) {
                return singular(equation, "curl_e");
            }
            if !finite3(db_dt) {
                return singular(equation, "db_dt");
            }
            let scalar = 1.0_f64.max(max_abs3(curl_e)).max(max_abs3(db_dt));
            let scaled_lhs = scale3(curl_e, scalar);
            let scaled_rhs = neg3(scale3(db_dt, scalar));
            let vector_residual = sub3(scaled_lhs, scaled_rhs);
            let normalized = normalized_scaled(scaled_lhs, scaled_rhs, scalar);
            Residual {
                equation,
                lhs: None,
                rhs: None,
                vector_residual: Some(vector_residual),
                normalized: Some(normalized),
                status: TermStatus::Analytic,
                explanation: format!(
                    "{}: curl E plus dB/dt is checked as a vector residual. {SOLVER_NOTE}",
                    equation.title()
                ),
            }
        }
        MaxwellEquation::AmpereMaxwell => {
            let Some(curl_b) = terms.curl_b else {
                return unsupported(equation, "curl_b");
            };
            let Some(j) = terms.j else {
                return unsupported(equation, "j");
            };
            let Some(de_dt) = terms.de_dt else {
                return unsupported(equation, "displacement current (de_dt)");
            };
            if !finite3(curl_b) {
                return singular(equation, "curl_b");
            }
            if !finite3(j) {
                return singular(equation, "j");
            }
            if !finite3(de_dt) {
                return singular(equation, "de_dt");
            }
            let scalar = 1.0_f64
                .max(max_abs3(curl_b))
                .max(max_abs3(j))
                .max(max_abs3(de_dt));
            let scaled_lhs = scale3(curl_b, scalar);
            let scaled_rhs = add3(scale3(j, scalar), scale3(de_dt, scalar));
            let vector_residual = sub3(scaled_lhs, scaled_rhs);
            let normalized = normalized_scaled(scaled_lhs, scaled_rhs, scalar);
            Residual {
                equation,
                lhs: None,
                rhs: None,
                vector_residual: Some(vector_residual),
                normalized: Some(normalized),
                status: TermStatus::Analytic,
                explanation: format!(
                    "{}: the displacement current is included because de_dt was supplied; curl B is checked against j plus dE/dt. {SOLVER_NOTE}",
                    equation.title()
                ),
            }
        }
    }
}

fn scalar_residual(
    equation: MaxwellEquation,
    lhs: Option<f64>,
    lhs_name: &'static str,
    rhs: Option<f64>,
    rhs_name: &'static str,
    detail: &str,
) -> Residual {
    let Some(lhs) = lhs else {
        return unsupported(equation, lhs_name);
    };
    let Some(rhs) = rhs else {
        return unsupported(equation, rhs_name);
    };
    if !lhs.is_finite() {
        return singular(equation, lhs_name);
    }
    if !rhs.is_finite() {
        return singular(equation, rhs_name);
    }
    let normalized = normalized_scalar(lhs, rhs);
    Residual {
        equation,
        lhs: Some(lhs),
        rhs: Some(rhs),
        vector_residual: None,
        normalized: Some(normalized),
        status: TermStatus::Analytic,
        explanation: format!("{}: {detail}. {SOLVER_NOTE}", equation.title()),
    }
}

fn unsupported(equation: MaxwellEquation, symbol: &'static str) -> Residual {
    Residual {
        equation,
        lhs: None,
        rhs: None,
        vector_residual: None,
        normalized: None,
        status: TermStatus::Unsupported(symbol),
        explanation: format!(
            "Cannot evaluate {}: the symbol {symbol} is unavailable in the active mode, so no zero was substituted. {SOLVER_NOTE}",
            equation.title()
        ),
    }
}

fn singular(equation: MaxwellEquation, symbol: &'static str) -> Residual {
    Residual {
        equation,
        lhs: None,
        rhs: None,
        vector_residual: None,
        normalized: None,
        status: TermStatus::Singular(symbol),
        explanation: format!(
            "Cannot evaluate {}: {symbol} is non-finite at this sample, so no value is reported. {SOLVER_NOTE}",
            equation.title()
        ),
    }
}

/// `|lhs - rhs| / max(1, |lhs|, |rhs|)`, scaled before subtracting so the
/// result stays finite for every pair of finite inputs.
fn normalized_scalar(lhs: f64, rhs: f64) -> f64 {
    let scale = 1.0_f64.max(lhs.abs()).max(rhs.abs());
    ((lhs / scale) - (rhs / scale)).abs()
}

/// `|lhs - rhs| / max(1, |lhs|, |rhs|)` for vector terms, with the same
/// finite-scaling guarantee. Both terms are already divided by `scalar`, the
/// common scale `max(1, |largest raw component|)`.
fn normalized_scaled(lhs_scaled: [f64; 3], rhs_scaled: [f64; 3], scalar: f64) -> f64 {
    let difference = sub3(lhs_scaled, rhs_scaled);
    let denominator = norm3(lhs_scaled).max(norm3(rhs_scaled)).max(1.0 / scalar);
    norm3(difference) / denominator
}

fn finite3(value: [f64; 3]) -> bool {
    value.iter().all(|component| component.is_finite())
}

fn max_abs3(value: [f64; 3]) -> f64 {
    value
        .iter()
        .fold(0.0_f64, |largest, component| largest.max(component.abs()))
}

fn norm3(value: [f64; 3]) -> f64 {
    value
        .iter()
        .map(|component| component * component)
        .sum::<f64>()
        .sqrt()
}

fn scale3(value: [f64; 3], scalar: f64) -> [f64; 3] {
    [value[0] / scalar, value[1] / scalar, value[2] / scalar]
}

fn add3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn sub3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn neg3(value: [f64; 3]) -> [f64; 3] {
    [-value[0], -value[1], -value[2]]
}
