//! Issue #170: typed, preview-before-apply CAS app-control actions.
//!
//! The grammar is a small, fixed verb vocabulary on top of the tolerant CAS
//! line from issue #168. Parsing is pure: it validates every value it can see
//! and never touches the app. Actions that need live app state (committed
//! parameter names, library records, the probe cap) are validated again by the
//! application before the first mutation.
//!
//! Accepted verbs, case-insensitively, with values preserved:
//!
//! | Input | Effect |
//! | --- | --- |
//! | `formula <vector formula>` | replace the vector-field formula |
//! | `scalar <surface expr>` | replace the scalar surface source (kind stays surface) |
//! | `param name=value, ...` | set existing named-parameter values (at most 8) |
//! | `domain x, y, z` | half-extents, 0.1..=100 each, centred at the origin |
//! | `density n` | samples per axis |
//! | `2d` / `3d` | switch the projection |
//! | `colors <colormap> [quantity] [mode]` | palette, quantity and scale mode |
//! | `appearance <name>` | apply a library appearance profile |
//! | `probe x, y, z` | add a point probe |
//! | `seeds <name>` | apply a named seed layout |
//! | `camera fit\|reset\|isometric` | reframe the camera |
//! | `play` / `pause` | toggle transport playback |
//!
//! No action creates a source, runs a solver, writes a file or touches preset
//! storage.

use std::fmt;

use crate::cas_intent::MAX_INPUT_BYTES;
use crate::color_mapping::ColorQuantity;
use crate::color_ramp::ScaleMode;
use crate::colormap::Colormap;
use crate::field::FieldExpr;
use crate::scalar::{ScalarContext, ScalarExpr};
use crate::viewmodel::{MAX_DENSITY_2D, MIN_DENSITY};

/// The most parameter assignments one `param` line may carry.
pub const MAX_PARAMETER_ASSIGNMENTS: usize = 8;
/// Smallest accepted domain half-extent, world units.
pub const MIN_HALF_EXTENT: f32 = 0.1;
/// Largest accepted domain half-extent, world units.
pub const MAX_HALF_EXTENT: f32 = 100.0;

/// A validated, typed app-control action.
#[derive(Clone, Debug, PartialEq)]
pub enum CasAction {
    /// Replace the vector-field formula; `FieldExpr::parse_with_parameters`
    /// validates it against the committed set before use.
    SetFormula { formula: String },
    /// Replace the scalar surface source; the plot kind stays `Surface`.
    SetScalarSource { expr: String },
    /// Set values of parameters that already exist in the committed set.
    SetParameters { values: Vec<(String, f32)> },
    /// Replace the domain with a box centred on the origin.
    SetDomain { half_extents: [f32; 3] },
    /// Samples per axis, subject to the current mode's sampling bound.
    SetDensity { samples_per_axis: u32 },
    /// Switch between the 2D and 3D projections.
    SetDimension { three_d: bool },
    /// Replace the palette, quantity and value-to-ramp mode together.
    SetColors {
        colormap: Colormap,
        quantity: ColorQuantity,
        mode: ScaleMode,
    },
    /// Apply a profile resolved by the app from the live library.
    ApplyAppearanceProfile { name: String },
    /// Add one point probe, subject to the shared panel cap.
    AddPointProbe { position: [f32; 3] },
    /// Apply a layout resolved by the app from the live library.
    ApplySeedLayout { name: String },
    /// Reframe the camera without touching simulation state.
    SetCamera { framing: CameraFraming },
    /// Toggle transport playback; runtime-only, never a document edit.
    SetTransport { playing: bool },
}

/// The bounded camera framings offered by the `camera` verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraFraming {
    Fit,
    Reset,
    /// The app's default isometric pose, fitted to the domain.
    Isometric,
}

/// Why an action line was rejected, or why a validated action cannot apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CasActionError {
    Empty,
    TooLong,
    UnknownVerb,
    BadValue {
        field: &'static str,
        reason: String,
    },
    /// The action is valid in general but not available in the current app
    /// state (for example a missing library record).
    UnsupportedHere(String),
}

impl fmt::Display for CasActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("CAS action is empty"),
            Self::TooLong => write!(f, "CAS action exceeds the {MAX_INPUT_BYTES}-byte limit"),
            Self::UnknownVerb => f.write_str(
                "unrecognised CAS action; accepted verbs are formula, scalar, \
                 param, domain, density, 2d, 3d, colors, appearance, probe, \
                 seeds, camera, play and pause",
            ),
            Self::BadValue { field, reason } => write!(f, "{field}: {reason}"),
            Self::UnsupportedHere(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for CasActionError {}

/// Parse one CAS line into an action. Pure: no app state is read or changed.
///
/// Unrecognised leading words return [`CasActionError::UnknownVerb`] so the
/// panel can fall back to the mathematical intent grammar from issue #168.
pub fn parse_action(text: &str) -> Result<CasAction, CasActionError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(CasActionError::Empty);
    }
    if text.len() > MAX_INPUT_BYTES {
        return Err(CasActionError::TooLong);
    }
    let (verb, rest) = split_verb(text);
    match verb.as_str() {
        "formula" => {
            let formula = rest.trim();
            if formula.is_empty() {
                return Err(bad("formula", "a vector formula is required"));
            }
            FieldExpr::parse(formula).map_err(|error| bad("formula", error.to_string()))?;
            Ok(CasAction::SetFormula {
                formula: formula.to_owned(),
            })
        }
        "scalar" => {
            let expr = rest.trim();
            if expr.is_empty() {
                return Err(bad("scalar", "a surface expression is required"));
            }
            ScalarExpr::parse(expr, ScalarContext::Surface)
                .map_err(|error| bad("scalar", error.to_string()))?;
            Ok(CasAction::SetScalarSource {
                expr: expr.to_owned(),
            })
        }
        "param" => Ok(CasAction::SetParameters {
            values: parse_assignments(rest)?,
        }),
        "domain" => {
            let values = parse_numbers(rest, "domain")?;
            for value in values {
                if !(MIN_HALF_EXTENT..=MAX_HALF_EXTENT).contains(&value) {
                    return Err(bad(
                        "domain",
                        format!(
                            "half-extent {value} is outside \
                             {MIN_HALF_EXTENT}..={MAX_HALF_EXTENT}"
                        ),
                    ));
                }
            }
            Ok(CasAction::SetDomain {
                half_extents: [values[0], values[1], values[2]],
            })
        }
        "density" => {
            let value = rest.trim();
            let samples_per_axis = value.parse::<u32>().map_err(|_| {
                bad(
                    "density",
                    format!("`{value}` is not a whole number of samples"),
                )
            })?;
            if !(MIN_DENSITY..=MAX_DENSITY_2D).contains(&samples_per_axis) {
                return Err(bad(
                    "density",
                    format!(
                        "{samples_per_axis} samples per axis is outside \
                         {MIN_DENSITY}..={MAX_DENSITY_2D}"
                    ),
                ));
            }
            Ok(CasAction::SetDensity { samples_per_axis })
        }
        "2d" | "3d" => {
            if rest.trim().is_empty() {
                Ok(CasAction::SetDimension {
                    three_d: verb == "3d",
                })
            } else {
                Err(bad("dimension", "`2d`/`3d` take no arguments"))
            }
        }
        "colors" | "colours" => parse_colors(rest),
        "appearance" => {
            let name = rest.trim();
            if name.is_empty() {
                Err(bad("appearance", "a profile name is required"))
            } else {
                Ok(CasAction::ApplyAppearanceProfile {
                    name: name.to_owned(),
                })
            }
        }
        "probe" => {
            let values = parse_numbers(rest, "probe")?;
            Ok(CasAction::AddPointProbe {
                position: [values[0], values[1], values[2]],
            })
        }
        "seeds" => {
            let name = rest.trim();
            if name.is_empty() {
                Err(bad("seeds", "a seed-layout name is required"))
            } else {
                Ok(CasAction::ApplySeedLayout {
                    name: name.to_owned(),
                })
            }
        }
        "camera" => {
            let value = rest.trim().to_ascii_lowercase();
            let framing = match value.as_str() {
                "fit" => CameraFraming::Fit,
                "reset" => CameraFraming::Reset,
                "isometric" => CameraFraming::Isometric,
                _ => {
                    return Err(bad(
                        "camera",
                        "accepted framings are fit, reset and isometric",
                    ));
                }
            };
            Ok(CasAction::SetCamera { framing })
        }
        "play" | "pause" => {
            let field = if verb == "play" { "play" } else { "pause" };
            if rest.trim().is_empty() {
                Ok(CasAction::SetTransport {
                    playing: verb == "play",
                })
            } else {
                Err(bad(field, format!("`{field}` takes no arguments")))
            }
        }
        _ => Err(CasActionError::UnknownVerb),
    }
}

/// One-line, user-visible description of exactly what `action` will do.
///
/// Pure: it never mutates and never samples.
pub fn preview(action: &CasAction) -> String {
    match action {
        CasAction::SetFormula { formula } => format!("Formula: {formula}"),
        CasAction::SetScalarSource { expr } => format!("Surface source: {expr}"),
        CasAction::SetParameters { values } => format!(
            "Parameters: {}",
            values
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        CasAction::SetDomain { half_extents } => format!(
            "Domain half-extents: {} × {} × {} (centred at the origin)",
            half_extents[0], half_extents[1], half_extents[2]
        ),
        CasAction::SetDensity { samples_per_axis } => {
            format!("Density: {samples_per_axis} samples per axis")
        }
        CasAction::SetDimension { three_d } => {
            if *three_d {
                "Switch to 3D".to_owned()
            } else {
                "Switch to 2D".to_owned()
            }
        }
        CasAction::SetColors {
            colormap,
            quantity,
            mode,
        } => format!(
            "Colours: {}, {}, {}",
            colormap.label(),
            quantity.label(),
            mode_label(*mode)
        ),
        CasAction::ApplyAppearanceProfile { name } => {
            format!("Appearance profile: {name}")
        }
        CasAction::AddPointProbe { position } => format!(
            "Point probe at ({}, {}, {})",
            position[0], position[1], position[2]
        ),
        CasAction::ApplySeedLayout { name } => format!("Seed layout: {name}"),
        CasAction::SetCamera { framing } => format!(
            "Camera: {}",
            match framing {
                CameraFraming::Fit => "fit",
                CameraFraming::Reset => "reset",
                CameraFraming::Isometric => "isometric",
            }
        ),
        CasAction::SetTransport { playing } => {
            if *playing {
                "Transport: play".to_owned()
            } else {
                "Transport: pause".to_owned()
            }
        }
    }
}

fn mode_label(mode: ScaleMode) -> String {
    match mode {
        ScaleMode::Linear => "linear".to_owned(),
        ScaleMode::SignedLog { half_width } => format!("signed log (half width {half_width})"),
    }
}

fn parse_colors(rest: &str) -> Result<CasAction, CasActionError> {
    let tokens = split_values(rest);
    if tokens.is_empty() {
        return Err(bad("colors", "a colormap name is required"));
    }
    let token = tokens[0];
    let colormap = match token.to_ascii_lowercase().as_str() {
        "viridis" => Colormap::Viridis,
        "turbo" => Colormap::Turbo,
        "mono" => Colormap::Mono,
        _ => {
            return Err(bad(
                "colors",
                format!("unknown colormap `{token}`; use viridis, turbo or mono"),
            ));
        }
    };
    let quantity = match tokens.get(1) {
        None => ColorQuantity::Magnitude,
        Some(token) => parse_quantity(token)?,
    };
    let mode = match tokens.get(2) {
        None => ScaleMode::Linear,
        Some(token) => parse_mode(token)?,
    };
    if let Some(extra) = tokens.get(3) {
        return Err(bad(
            "colors",
            format!("unexpected extra value `{extra}`; use colors <colormap> [quantity] [mode]"),
        ));
    }
    Ok(CasAction::SetColors {
        colormap,
        quantity,
        mode,
    })
}

fn parse_quantity(token: &str) -> Result<ColorQuantity, CasActionError> {
    match token.to_ascii_lowercase().as_str() {
        "magnitude" => Ok(ColorQuantity::Magnitude),
        "x" | "componentx" | "component_x" => Ok(ColorQuantity::ComponentX),
        "y" | "componenty" | "component_y" => Ok(ColorQuantity::ComponentY),
        "z" | "componentz" | "component_z" => Ok(ColorQuantity::ComponentZ),
        "direction" | "direction2d" | "direction_2d" => Ok(ColorQuantity::Direction2d),
        other => Err(bad(
            "colors",
            format!("unknown quantity `{other}`; use magnitude, x, y, z or direction"),
        )),
    }
}

fn parse_mode(token: &str) -> Result<ScaleMode, CasActionError> {
    let token = token.to_ascii_lowercase();
    if token == "linear" {
        return Ok(ScaleMode::Linear);
    }
    let width = token
        .strip_prefix("signedlog:")
        .or_else(|| token.strip_prefix("signedlog="))
        .ok_or_else(|| {
            bad(
                "colors",
                format!("unknown mode `{token}`; use linear or signedlog:<half_width>"),
            )
        })?;
    let half_width = width.parse::<f32>().map_err(|_| {
        bad(
            "colors",
            format!("`{width}` is not a signed-log half width"),
        )
    })?;
    let mode = ScaleMode::SignedLog { half_width };
    mode.validate().map_err(|reason| bad("colors", reason))?;
    Ok(mode)
}

fn parse_assignments(rest: &str) -> Result<Vec<(String, f32)>, CasActionError> {
    let tokens = split_values(rest);
    if tokens.is_empty() {
        return Err(bad(
            "param",
            "at least one name=value assignment is required",
        ));
    }
    if tokens.len() > MAX_PARAMETER_ASSIGNMENTS {
        return Err(bad(
            "param",
            format!(
                "{} assignments exceed the {MAX_PARAMETER_ASSIGNMENTS}-assignment limit",
                tokens.len()
            ),
        ));
    }
    let mut values: Vec<(String, f32)> = Vec::with_capacity(tokens.len());
    for token in tokens {
        let (name, value) = token
            .split_once('=')
            .ok_or_else(|| bad("param", format!("`{token}` is not name=value")))?;
        let name = name.trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(bad("param", format!("`{name}` is not a parameter name")));
        }
        let value = value
            .trim()
            .parse::<f32>()
            .map_err(|_| bad("param", format!("`{value}` is not a finite number")))?;
        if !value.is_finite() {
            return Err(bad("param", format!("parameter `{name}` must be finite")));
        }
        if values.iter().any(|(existing, _)| existing == name) {
            return Err(bad("param", format!("duplicate assignment for `{name}`")));
        }
        values.push((name.to_owned(), value));
    }
    Ok(values)
}

/// Parse exactly three finite numbers. The fixed `[f32; 3]` return type is the
/// contract: a shorter list is rejected rather than silently zero-filled.
fn parse_numbers(rest: &str, field: &'static str) -> Result<[f32; 3], CasActionError> {
    const EXPECTED: usize = 3;
    let tokens = split_values(rest);
    if tokens.len() != EXPECTED {
        return Err(bad(
            field,
            format!("expected {EXPECTED} numbers, found {}", tokens.len()),
        ));
    }
    let mut values = [0.0_f32; 3];
    for (slot, token) in values.iter_mut().zip(tokens) {
        *slot = token
            .parse::<f32>()
            .map_err(|_| bad(field, format!("`{token}` is not a number")))?;
        if !slot.is_finite() {
            return Err(bad(field, format!("`{token}` is not finite")));
        }
    }
    Ok(values)
}

fn split_verb(text: &str) -> (String, &str) {
    match text.find(char::is_whitespace) {
        Some(index) => (text[..index].to_ascii_lowercase(), &text[index..]),
        None => (text.to_ascii_lowercase(), ""),
    }
}

fn split_values(rest: &str) -> Vec<&str> {
    rest.split(|ch: char| ch == ',' || ch.is_whitespace())
        .filter(|token| !token.is_empty())
        .collect()
}

fn bad(field: &'static str, reason: impl Into<String>) -> CasActionError {
    CasActionError::BadValue {
        field,
        reason: reason.into(),
    }
}
