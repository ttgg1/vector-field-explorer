//! Explicit streamline seed layout (issue #87 slice S1).
//!
//! A seed is a world position plus an optional record of the plane it was
//! planted on. `position` is authoritative for sampling; the anchor only lets
//! the editing UI reopen a plane and restore the local `u`, `v` coordinates.
//! Moving the selected plane never moves a stored seed.
//!
//! This is the pure document model: S1 stores, validates, sanitises and
//! persists the layout. Streamline generation still uses the automatic grid;
//! slice S2 wires generators, picking and the Flow-section editing on top.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::domain::Domain;
use crate::plane::PlaneAxis;

/// The most seeds one layout may store.
pub const MAX_SEEDS: usize = 256;

/// Absolute round-off tolerance on the inclusive domain bounds.
const DOMAIN_EPSILON: f32 = 1e-5;

/// The plane a seed was planted on, in that plane's local coordinates.
///
/// `index` is the grid slice index the plane was showing when the seed was
/// placed; `u`/`v` are the in-plane coordinates reported by
/// [`PlaneFrame::to_local`](crate::plane::PlaneFrame::to_local).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeedAnchor {
    /// The axis the planting plane is perpendicular to.
    #[serde(with = "axis_serde")]
    pub axis: PlaneAxis,
    /// The grid slice index along `axis`.
    pub index: u32,
    /// Local in-plane coordinate along the frame's `right` vector.
    pub u: f32,
    /// Local in-plane coordinate along the frame's `up` vector.
    pub v: f32,
}

/// One explicit streamline seed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeedPoint {
    /// World position, authoritative for sampling. Always finite and inside
    /// the scene domain after validation.
    #[serde(with = "position_serde")]
    pub position: Vec3,
    /// The planting plane, or `None` for a seed placed without a plane.
    #[serde(default)]
    pub anchor: Option<SeedAnchor>,
}

/// The ordered collection of explicit seeds (insertion order, generators
/// append).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeedLayout {
    pub seeds: Vec<SeedPoint>,
}

impl SeedLayout {
    /// Whether the layout stores no seeds (the automatic grid still applies).
    pub fn is_empty(&self) -> bool {
        self.seeds.is_empty()
    }

    /// The number of stored seeds.
    pub fn len(&self) -> usize {
        self.seeds.len()
    }

    /// Drop every seed.
    pub fn clear(&mut self) {
        self.seeds.clear();
    }

    /// Check the whole layout without repairing it.
    ///
    /// Rejects more than [`MAX_SEEDS`] seeds, any non-finite position, any
    /// position outside `domain` (inclusive, with a small absolute round-off
    /// tolerance) and any non-finite anchor `u`/`v`.
    pub fn validate(&self, domain: &Domain) -> Result<(), String> {
        if self.seeds.len() > MAX_SEEDS {
            return Err(format!(
                "seed layout has {} seeds; at most {MAX_SEEDS} are allowed",
                self.seeds.len()
            ));
        }
        for (index, seed) in self.seeds.iter().enumerate() {
            validate_point(index, seed, domain)?;
        }
        Ok(())
    }

    /// Drop invalid seeds and truncate to [`MAX_SEEDS`], preserving insertion
    /// order. Total: an empty layout is valid, and this never fails.
    pub fn sanitized(self, domain: &Domain) -> Self {
        let mut seeds: Vec<SeedPoint> = self
            .seeds
            .into_iter()
            .filter(|seed| point_is_valid(seed, domain))
            .collect();
        seeds.truncate(MAX_SEEDS);
        Self { seeds }
    }

    /// Append one seed after validating it and enforcing the cap.
    ///
    /// Rejects in full: a failed push leaves the layout unchanged.
    pub fn push(&mut self, point: SeedPoint, domain: &Domain) -> Result<(), String> {
        if self.seeds.len() >= MAX_SEEDS {
            return Err(format!(
                "seed layout is full ({} seeds); at most {MAX_SEEDS} are allowed",
                self.seeds.len()
            ));
        }
        validate_point(self.seeds.len(), &point, domain)?;
        self.seeds.push(point);
        Ok(())
    }
}

fn validate_point(index: usize, seed: &SeedPoint, domain: &Domain) -> Result<(), String> {
    if !seed.position.is_finite() {
        return Err(format!("seed {index}: position is not finite"));
    }
    if !position_in_domain(seed.position, domain) {
        return Err(format!(
            "seed {index}: position [{}, {}, {}] is outside the scene domain",
            seed.position.x, seed.position.y, seed.position.z
        ));
    }
    if let Some(anchor) = &seed.anchor
        && (!anchor.u.is_finite() || !anchor.v.is_finite())
    {
        return Err(format!("seed {index}: anchor coordinate is not finite"));
    }
    Ok(())
}

fn point_is_valid(seed: &SeedPoint, domain: &Domain) -> bool {
    validate_point(0, seed, domain).is_ok()
}

/// Inclusive bounds check with a small absolute epsilon so a position produced
/// by a plane round trip at the boundary still validates.
fn position_in_domain(position: Vec3, domain: &Domain) -> bool {
    position
        .cmpge(domain.min - Vec3::splat(DOMAIN_EPSILON))
        .all()
        && position
            .cmple(domain.max + Vec3::splat(DOMAIN_EPSILON))
            .all()
}

/// Persist `PlaneAxis` as the same lower-case axis token the other records use
/// (`x`, `y`, `z`) rather than depending on an enum's derived spelling.
mod axis_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::plane::PlaneAxis;

    pub fn serialize<S: Serializer>(axis: &PlaneAxis, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(axis.label())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PlaneAxis, D::Error> {
        let token = String::deserialize(deserializer)?;
        match token.as_str() {
            "x" => Ok(PlaneAxis::X),
            "y" => Ok(PlaneAxis::Y),
            "z" => Ok(PlaneAxis::Z),
            other => Err(serde::de::Error::unknown_variant(other, &["x", "y", "z"])),
        }
    }
}

/// Keep `glam` out of the persisted schema: a position is a plain
/// `[f32; 3]`, matching `domain_min`/`domain_max` and the other records.
mod position_serde {
    use glam::Vec3;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(position: &Vec3, serializer: S) -> Result<S::Ok, S::Error> {
        position.to_array().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec3, D::Error> {
        let [x, y, z] = <[f32; 3]>::deserialize(deserializer)?;
        Ok(Vec3::new(x, y, z))
    }
}
