//! Portable palette document (issue #83 slice C1).
//!
//! A strict, versioned JSON form for a custom [`ColorRamp`] so a palette can be
//! copied between scenes and machines. This is a pure model: it does not touch
//! the renderer, persistence or any UI.

use crate::color_ramp::{ColorRamp, ColorStop, RampKind, Rgb};

/// The only palette document version this module writes or accepts.
pub const PALETTE_FORMAT_VERSION: u32 = 1;

/// One colour stop in a portable [`PaletteDocument`].
///
/// Colours are stored as text rather than bytes so imports can validate and
/// name the offending stop. Exports use canonical lowercase `#rrggbb`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteStop {
    /// Normalized ramp position, in `0.0..=1.0`.
    pub position: f32,
    /// Hex colour text, parsed on import.
    pub color: String,
}

/// A versioned, strict JSON document describing a [`ColorRamp`].
///
/// Unknown fields are rejected by serde, and field order is pinned by
/// declaration order so exports diff readably. Validation happens on import:
/// an invalid document is reported, never repaired.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteDocument {
    /// Document format version; must be [`PALETTE_FORMAT_VERSION`].
    pub version: u32,
    /// Semantic use of the ramp.
    pub kind: RampKind,
    /// Whether sampling reads the ramp back to front.
    pub reversed: bool,
    /// Stops in ramp order.
    pub stops: Vec<PaletteStop>,
}

impl PaletteDocument {
    /// Copies `ramp` into a versioned document with lowercase hex colours.
    pub fn from_ramp(ramp: &ColorRamp) -> Self {
        Self {
            version: PALETTE_FORMAT_VERSION,
            kind: ramp.kind,
            reversed: ramp.reversed,
            stops: ramp
                .stops
                .iter()
                .map(|stop| PaletteStop {
                    position: stop.position,
                    color: stop.color.to_hex(),
                })
                .collect(),
        }
    }

    /// Rebuilds and validates the ramp described by this document.
    ///
    /// Rejects an unsupported version first, then parses each colour in order
    /// (accepting uppercase and an optional `#`), then returns the exact
    /// [`ColorRamp::validate`] error when the ramp is structurally invalid.
    /// No stop is sanitised, clamped or dropped.
    pub fn to_ramp(&self) -> Result<ColorRamp, String> {
        if self.version != PALETTE_FORMAT_VERSION {
            return Err(format!(
                "palette version {} is not supported; expected version {}",
                self.version, PALETTE_FORMAT_VERSION
            ));
        }

        let mut stops = Vec::with_capacity(self.stops.len());
        for (index, palette_stop) in self.stops.iter().enumerate() {
            let color = Rgb::from_hex(&palette_stop.color).ok_or_else(|| {
                format!("stop {index} has invalid colour {:?}", palette_stop.color)
            })?;
            stops.push(ColorStop {
                position: palette_stop.position,
                color,
            });
        }

        let ramp = ColorRamp {
            stops,
            reversed: self.reversed,
            kind: self.kind,
        };
        ramp.validate()?;
        Ok(ramp)
    }

    /// Serializes the document as compact JSON.
    ///
    /// Returns an empty string when serialization fails, which can only happen
    /// for a hand-built [`ColorRamp`] carrying non-finite stop positions,
    /// because `serde_json` rejects non-finite floats. Ramps produced by
    /// [`ColorRamp::builtin`], by [`Self::from_ramp`] on a validated ramp, or
    /// by [`Self::to_ramp`] always serialize; the fallback keeps this method
    /// panic-free for any other value.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Parses a palette document, rejecting unknown fields.
    ///
    /// Every serde failure maps to one `invalid palette JSON: {error}` message.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|error| format!("invalid palette JSON: {error}"))
    }
}

/// Serializes `ramp` as a portable palette document.
///
/// Returns an empty string when serialization fails, which can only happen for
/// a hand-built [`ColorRamp`] with non-finite stop positions; see
/// [`PaletteDocument::to_json`] for the full conditions.
pub fn export_ramp(ramp: &ColorRamp) -> String {
    PaletteDocument::from_ramp(ramp).to_json()
}

/// Parses and validates a portable palette document into a [`ColorRamp`].
pub fn import_ramp(text: &str) -> Result<ColorRamp, String> {
    PaletteDocument::from_json(text)?.to_ramp()
}
