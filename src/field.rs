//! Formula field parsing and evaluation.
//!
//! A field is written as `(u, v)` or `(u, v, w)` where each component is a
//! math expression in `x`, `y`, `z` parsed by `meval`.

use glam::{Vec2, Vec3};

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
    pub fn parse(src: &str) -> Result<Self, FieldError> {
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
            let expr = component
                .parse::<meval::Expr>()
                .map_err(|e| FieldError::Parse(format!("in component `{component}`: {e}")))?;
            // Detect time dependence at parse time by tokenizing the component
            // and looking for a free variable named `t`. A `t(...)` call is a
            // function, not a variable, so it does not count.
            if component_time_dependent(component) {
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
        let f = self.components.get(i)?.clone().bind4("x", "y", "z", "t").ok()?;
        let v = f(x, y, z, t);
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
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

    /// Evaluates the field at a 3D point at time `t`. A 2D field evaluates
    /// over `p.xy` and returns `Vec3::new(u, v, 0.0)`.
    pub fn eval3_at(&self, p: Vec3, t: f32) -> Option<Vec3> {
        let t = t as f64;
        let x = p.x as f64;
        let y = p.y as f64;
        let z = if self.dimension == 3 { p.z } else { 0.0 } as f64;
        let (u, v) = self.eval_uv(x, y, z, t)?;
        let w = if self.dimension == 3 {
            self.eval_component(2, x, y, z, t)?
        } else {
            0.0
        };
        let out = Vec3::new(u as f32, v as f32, w as f32);
        if out.is_finite() { Some(out) } else { None }
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

/// Whether a single component string references `t` as a free variable.
fn component_time_dependent(src: &str) -> bool {
    match meval::tokenizer::tokenize(src) {
        Ok(tokens) => tokens
            .iter()
            .any(|tok| matches!(tok, meval::tokenizer::Token::Var(name) if name == "t")),
        Err(_) => false,
    }
}

