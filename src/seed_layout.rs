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

use std::fmt;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::domain::Domain;
use crate::plane::{PlaneAxis, PlaneFrame};
use crate::streamlines::explicit_streamline_seeds;

/// The most seeds one layout may store.
pub const MAX_SEEDS: usize = 256;

/// The most seeds one generator or placement application may add at once.
pub const MAX_GENERATED_PER_APPLICATION: usize = 64;

/// Absolute round-off tolerance on the inclusive domain bounds.
const DOMAIN_EPSILON: f32 = 1e-5;

/// Why a generator or an application was rejected.
///
/// Every variant is reported to the user verbatim; generated points are never
/// clipped, and an over-cap application is rejected in full rather than
/// truncated.
#[derive(Debug, Clone, PartialEq)]
pub enum SeedError {
    /// An input field (domain, endpoint, centre, radius, bound) is not finite.
    NonFinite(&'static str),
    /// The requested geometry is degenerate (coincident endpoints, a zero
    /// radius, an inverted or zero-sized region).
    Degenerate(&'static str),
    /// A generator was asked for no seeds.
    ZeroCount,
    /// A generator's count is below its geometry's minimum.
    CountTooSmall { count: u32, min: u32 },
    /// A generation request exceeds [`MAX_GENERATED_PER_APPLICATION`].
    CountTooLarge { count: usize, max: usize },
    /// A generated point lies outside the scene domain.
    OutsideDomain(Vec3),
    /// Appending would exceed [`MAX_SEEDS`]; nothing was applied.
    LayoutFull {
        requested: usize,
        existing: usize,
        max: usize,
    },
    /// The resulting layout failed the ordinary collection validation.
    Invalid(String),
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite(what) => write!(f, "{what} is not finite"),
            Self::Degenerate(what) => write!(f, "{what}"),
            Self::ZeroCount => write!(f, "seed count must be at least 1"),
            Self::CountTooSmall { count, min } => {
                write!(f, "seed count {count} is below the minimum of {min}")
            }
            Self::CountTooLarge { count, max } => write!(
                f,
                "seed count {count} exceeds the per-application cap of {max}"
            ),
            Self::OutsideDomain(position) => write!(
                f,
                "generated point [{}, {}, {}] is outside the scene domain",
                position.x, position.y, position.z
            ),
            Self::LayoutFull {
                requested,
                existing,
                max,
            } => write!(
                f,
                "adding {requested} seeds to {existing} would exceed the {max}-seed layout cap"
            ),
            Self::Invalid(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for SeedError {}

/// `count` seeds evenly spaced from `from` to `to`, both endpoints included.
///
/// `count >= 2`, both endpoints finite and distinct, and every generated point
/// inside `domain`. Nothing is clamped.
pub fn line_seeds(
    domain: &Domain,
    from: Vec3,
    to: Vec3,
    count: u32,
) -> Result<Vec<Vec3>, SeedError> {
    validate_domain(domain)?;
    let count = validate_count(count, 2)?;
    if !from.is_finite() || !to.is_finite() {
        return Err(SeedError::NonFinite("line endpoint"));
    }
    if (to - from).length_squared() <= 0.0 {
        return Err(SeedError::Degenerate("line endpoints coincide"));
    }
    let mut seeds = Vec::with_capacity(count);
    for index in 0..count {
        let t = index as f32 / (count - 1) as f32;
        let point = from + (to - from) * t;
        ensure_inside(domain, point)?;
        seeds.push(point);
    }
    Ok(seeds)
}

/// `count` points on a circle in the plane normal to `axis`, starting at
/// angle 0 and stepping evenly.
///
/// The basis matches [`PlaneFrame`] (`Z → +X/+Y`, `X → -Z/+Y`,
/// `Y → -X/+Z`). `count >= 3`, `radius > 0`, and every point inside `domain`.
pub fn circle_seeds(
    domain: &Domain,
    center: Vec3,
    axis: PlaneAxis,
    radius: f32,
    count: u32,
) -> Result<Vec<Vec3>, SeedError> {
    validate_domain(domain)?;
    let count = validate_count(count, 3)?;
    if !center.is_finite() || !radius.is_finite() {
        return Err(SeedError::NonFinite("circle center or radius"));
    }
    if radius <= 0.0 {
        return Err(SeedError::Degenerate("circle radius must be positive"));
    }
    let (right, up) = match axis {
        PlaneAxis::X => (-Vec3::Z, Vec3::Y),
        PlaneAxis::Y => (-Vec3::X, Vec3::Z),
        PlaneAxis::Z => (Vec3::X, Vec3::Y),
    };
    let mut seeds = Vec::with_capacity(count);
    for index in 0..count {
        let angle = std::f32::consts::TAU * index as f32 / count as f32;
        let point = center + right * (radius * angle.cos()) + up * (radius * angle.sin());
        ensure_inside(domain, point)?;
        seeds.push(point);
    }
    Ok(seeds)
}

/// A deterministic jittered lattice of at most `count` seeds inside
/// `[min, max]`.
///
/// The lattice reuses [`explicit_streamline_seeds`]'s per-axis growth, jitter
/// and splitmix salt so a sub-region is reproducible without coinciding with
/// the automatic grid. The salt is derived from the requested bounds. A
/// degenerate region, non-finite input, `count == 0`, a count above
/// [`MAX_GENERATED_PER_APPLICATION`] and points outside `domain` are rejected.
pub fn region_seeds(
    domain: &Domain,
    min: Vec3,
    max: Vec3,
    count: u32,
) -> Result<Vec<Vec3>, SeedError> {
    validate_domain(domain)?;
    let count = validate_count(count, 1)?;
    if !min.is_finite() || !max.is_finite() {
        return Err(SeedError::NonFinite("region bound"));
    }
    if max.cmplt(min).any() {
        return Err(SeedError::Degenerate("region bounds are inverted"));
    }
    let extents = max - min;
    if extents.max_element() <= 0.0 {
        return Err(SeedError::Degenerate("region has zero size"));
    }
    let region = Domain::new(min, max);
    let three_dimensional = extents.min_element() > 0.0;
    let salt = (min.x.to_bits() as u64)
        ^ ((min.y.to_bits() as u64) << 21)
        ^ ((min.z.to_bits() as u64) << 42)
        ^ (max.x.to_bits() as u64).rotate_left(11)
        ^ (max.y.to_bits() as u64).rotate_left(32)
        ^ (max.z.to_bits() as u64).rotate_left(53);
    let seeds = explicit_streamline_seeds(&region, count as u32, three_dimensional, salt);
    if seeds.len() < count {
        return Err(SeedError::Degenerate(
            "region cannot hold the requested number of seeds",
        ));
    }
    let mut result = Vec::with_capacity(count);
    for point in seeds.into_iter().take(count) {
        ensure_inside(domain, point)?;
        result.push(point);
    }
    Ok(result)
}

/// Clamp a dragged local coordinate into `frame`'s in-plane bounds.
///
/// The normal component is dropped: a plane drag always lands on the plane.
/// Non-finite components collapse to the plane centre instead of escaping.
pub fn clamp_seed_local(frame: &PlaneFrame, local: Vec3) -> Vec3 {
    let clamp = |value: f32, half: f32| {
        if value.is_finite() {
            value.clamp(-half, half)
        } else {
            0.0
        }
    };
    Vec3::new(
        clamp(local.x, frame.half_u),
        clamp(local.y, frame.half_v),
        0.0,
    )
}

fn validate_domain(domain: &Domain) -> Result<(), SeedError> {
    if !domain.min.is_finite() || !domain.max.is_finite() {
        return Err(SeedError::NonFinite("scene domain"));
    }
    if domain.max.cmplt(domain.min).any() {
        return Err(SeedError::Degenerate("scene domain is inverted"));
    }
    Ok(())
}

fn validate_count(count: u32, min: u32) -> Result<usize, SeedError> {
    if count == 0 {
        return Err(SeedError::ZeroCount);
    }
    if count < min {
        return Err(SeedError::CountTooSmall { count, min });
    }
    if count as usize > MAX_GENERATED_PER_APPLICATION {
        return Err(SeedError::CountTooLarge {
            count: count as usize,
            max: MAX_GENERATED_PER_APPLICATION,
        });
    }
    Ok(count as usize)
}

fn ensure_inside(domain: &Domain, point: Vec3) -> Result<(), SeedError> {
    if !point.is_finite() {
        return Err(SeedError::NonFinite("generated point"));
    }
    if !position_in_domain(point, domain) {
        return Err(SeedError::OutsideDomain(point));
    }
    Ok(())
}

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
