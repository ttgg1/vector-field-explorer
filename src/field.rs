//! Formula field parsing and evaluation.
//!
//! A field is written as `(u, v)` or `(u, v, w)` where each component is a
//! math expression in `x`, `y`, `z` parsed by `meval`.

use glam::{DVec3, Vec2, Vec3};

use crate::parameters::{ParameterSet, RESERVED_NAMES};

#[derive(Debug, Clone)]
pub enum FieldError {
    Parse(String),
}

#[derive(Debug, Clone)]
pub struct FieldExpr {
    pub dimension: usize,
    components: Vec<meval::Expr>,
    time_dependent: bool,
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldError::Parse(msg) => write!(f, "field parse error: {msg}"),
        }
    }
}

impl std::error::Error for FieldError {}

/// Normalize the Unicode punctuation a formula editor commonly receives from
/// pasted or IME-composed text into the ASCII spelling the parser understands.
///
/// Only the input is rewritten: ASCII and the math alphanumeric block
/// (U+1D400..U+1D7FF) pass through unchanged, so callers can compare and store
/// the returned text without a second normalization step. Invalid text is still
/// returned as-is (normalized); the caller's parser remains the authority on
/// validity.
pub fn normalize_formula_input(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for ch in src.chars() {
        out.push(match ch {
            // Full-width ASCII forms, U+FF01..U+FF5E.
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
            '\u{3000}' | '\u{00A0}' => ' ',
            '\u{2212}' | '\u{2013}' | '\u{2014}' => '-',
            '\u{00D7}' => '*',
            '\u{00F7}' | '\u{2215}' | '\u{2044}' => '/',
            '\u{2329}' | '\u{27E8}' | '\u{3008}' => '(',
            '\u{232A}' | '\u{27E9}' | '\u{3009}' => ')',
            _ => ch,
        });
    }
    out
}

/// Splits `src` on commas that are not nested inside parentheses.
fn split_top_level(src: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: usize = 0;
    let mut start = 0;
    for (i, ch) in src.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&src[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&src[start..]);
    parts
}

impl FieldExpr {
    /// Parses a permissive formula: an unknown identifier is accepted here and
    /// only fails at evaluation, which is the historical behaviour. Startup and
    /// the formula editor use [`FieldExpr::parse_with_parameters`] instead.
    pub fn parse(src: &str) -> Result<Self, FieldError> {
        Self::parse_components(src, &ParameterSet::default(), false)
    }

    /// Parses a formula with the committed named parameters bound.
    ///
    /// Every free identifier that exactly names a parameter is rewritten to a
    /// parenthesised full-precision literal before `meval` sees it. Identifiers
    /// in call position (`name(`), the coordinates/time, meval's constants and
    /// builtin function names are never rewritten. An identifier still free
    /// after substitution is reported against its component instead of being
    /// silently accepted.
    pub fn parse_with_parameters(src: &str, parameters: &ParameterSet) -> Result<Self, FieldError> {
        Self::parse_components(src, parameters, true)
    }

    fn parse_components(
        src: &str,
        parameters: &ParameterSet,
        reject_free_names: bool,
    ) -> Result<Self, FieldError> {
        // The outer parentheses are list tokens, not part of a component.
        let body = src.trim();
        let body = if body.starts_with('(') && body.ends_with(')') {
            &body[1..body.len() - 1]
        } else {
            body
        };

        let parts: Vec<&str> = split_top_level(body);
        if parts.len() != 2 && parts.len() != 3 {
            return Err(FieldError::Parse(format!(
                "expected 2 or 3 comma-separated components, found {}: `{}`",
                parts.len(),
                src
            )));
        }

        let mut components = Vec::with_capacity(parts.len());
        let mut time_dependent = false;
        for part in &parts {
            let component = part.trim();
            let substituted = substitute_parameters(component, parameters);
            let expr = substituted
                .parse::<meval::Expr>()
                .map_err(|e| FieldError::Parse(format!("in component `{component}`: {e}")))?;
            // `meval` parses an unknown variable fine and only fails when it is
            // bound. Binding here reports a still-free value name honestly,
            // prefixed with the component. The permissive `parse` skips this.
            if reject_free_names && let Err(e) = expr.clone().bind4("x", "y", "z", "t") {
                return Err(FieldError::Parse(format!(
                    "in component `{component}`: {e}"
                )));
            }
            // Detect time dependence on the substituted component: a
            // parameter value can never introduce `t`, and a `t(...)` call is a
            // function, not a variable.
            if component_time_dependent(&substituted) {
                time_dependent = true;
            }
            components.push(expr);
        }

        Ok(Self {
            dimension: parts.len(),
            components,
            time_dependent,
        })
    }

    /// Evaluates component `i` at `(x, y, z, t)`. `None` if it cannot be bound
    /// (unknown variables) or the result is NaN/infinite.
    fn eval_component(&self, i: usize, x: f64, y: f64, z: f64, t: f64) -> Option<f64> {
        let f = self
            .components
            .get(i)?
            .clone()
            .bind4("x", "y", "z", "t")
            .ok()?;
        let v = f(x, y, z, t);
        if v.is_finite() { Some(v) } else { None }
    }

    /// Whether any component references `t` as a free variable (detected at
    /// parse time).
    pub fn is_time_dependent(&self) -> bool {
        self.time_dependent
    }

    /// Evaluates the first two components; `None` for any NaN/∞.
    fn eval_uv(&self, x: f64, y: f64, z: f64, t: f64) -> Option<(f64, f64)> {
        let u = self.eval_component(0, x, y, z, t)?;
        let v = self.eval_component(1, x, y, z, t)?;
        Some((u, v))
    }

    /// Evaluates the field at a 3D point at time `t` in f64 precision. A 2D
    /// field evaluates over `p.xy` and returns `Vec3::new(u, v, 0.0)`.
    ///
    /// This is the ODE integration boundary: the components are computed in
    /// f64 and only the final finite result is cast to f32 for the existing
    /// field API.
    pub fn eval3_at_f64(&self, p: DVec3, t: f64) -> Option<Vec3> {
        let components = self.eval3_components_at_f64(p, t)?;
        let out = Vec3::new(
            components[0] as f32,
            components[1] as f32,
            components[2] as f32,
        );
        if out.is_finite() { Some(out) } else { None }
    }

    /// f64 components behind [`FieldExpr::eval3_at_f64`]; the ODE solver uses
    /// this directly so its RK4 error is not floored by f32 rounding.
    pub(crate) fn eval3_components_at_f64(&self, p: DVec3, t: f64) -> Option<[f64; 3]> {
        let z = if self.dimension == 3 { p.z } else { 0.0 };
        let (u, v) = self.eval_uv(p.x, p.y, z, t)?;
        let w = if self.dimension == 3 {
            self.eval_component(2, p.x, p.y, z, t)?
        } else {
            0.0
        };
        let out = [u, v, w];
        if out.iter().all(|value| value.is_finite()) {
            Some(out)
        } else {
            None
        }
    }

    /// Evaluates the field at a 3D point at time `t`. A 2D field evaluates
    /// over `p.xy` and returns `Vec3::new(u, v, 0.0)`.
    pub fn eval3_at(&self, p: Vec3, t: f32) -> Option<Vec3> {
        self.eval3_at_f64(DVec3::new(p.x as f64, p.y as f64, p.z as f64), t as f64)
    }

    /// Evaluates the field at a 3D point. Delegates to [`eval3_at`] at `t = 0`.
    pub fn eval3(&self, p: Vec3) -> Option<Vec3> {
        self.eval3_at(p, 0.0)
    }

    /// Evaluates the first two components of the field at a 2D point (z = 0).
    pub fn eval2(&self, p: Vec2) -> Option<Vec2> {
        let (u, v) = self.eval_uv(p.x as f64, p.y as f64, 0.0, 0.0)?;
        let out = Vec2::new(u as f32, v as f32);
        if out.is_finite() { Some(out) } else { None }
    }
}

/// Rewrites each free identifier that exactly names a committed parameter to
/// its value as a parenthesised full-precision literal (`k*x` with `k = 2.5`
/// becomes `(2.5)*x`).
///
/// An identifier followed by `(` is a function call, never a value. The
/// coordinates, time, meval constants and builtin function names are left
/// alone even when a raw set contains them, so a set that skipped validation
/// can never shadow the formula environment. Numeric literals, including an
/// exponent such as `1e5`, are copied verbatim.
fn substitute_parameters(component: &str, parameters: &ParameterSet) -> String {
    let bytes = component.as_bytes();
    let mut out = String::with_capacity(component.len());
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte.is_ascii_digit() || byte == b'.' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            // Consume an exponent only when it is well formed, so `2e` is not
            // swallowed and `1e5` never becomes an identifier.
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let mut end = i + 1;
                if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                    end += 1;
                }
                if end < bytes.len() && bytes[end].is_ascii_digit() {
                    i = end;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            out.push_str(&component[start..i]);
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let name = &component[start..i];
            let is_call = component[i..].trim_start().starts_with('(');
            match parameters.value(name) {
                Some(value) if !is_call && !RESERVED_NAMES.contains(&name) => {
                    out.push('(');
                    out.push_str(&format!("{value}"));
                    out.push(')');
                }
                _ => out.push_str(name),
            }
        } else {
            // Copy any non-ASCII/operator character whole so a multi-byte
            // char can never be split.
            let ch = component[i..].chars().next().expect("valid char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Whether a single component string references `t` as a free variable.
fn component_time_dependent(src: &str) -> bool {
    match meval::tokenizer::tokenize(src) {
        Ok(tokens) => tokens
            .iter()
            .any(|tok| matches!(tok, meval::tokenizer::Token::Var(name) if name == "t")),
        Err(_) => false,
    }
}
