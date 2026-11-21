//! Bounded field-evaluation cache (issue #240).
//!
//! A sampling pass over the formula field is the dominant CPU cost of the
//! densest scenes. Most state changes (arrow style, appearance profile, overlay
//! toggles, window layout) do not change *what* is sampled, so the next rebuild
//! can reuse the previous [`ArrowSample`] list. This module supplies the pure
//! pieces: a [`FieldSampleKey`] that captures every sampler input and a
//! single-entry [`FieldSampleCache`] that refuses oversized passes.
//!
//! The cache is runtime-only: [`crate::viewmodel::State`] owns one, it is never
//! serialised into a document or scene, and cloning a state resets it.

use crate::domain::Domain;
use crate::parameters::ParameterSet;
use crate::plane::PlaneFrame;
use crate::sampling::{ArrowData, ViewOptions};
use crate::source_scene::FieldMode;

/// Quoted here so callers and tests can refer to the sample type by its cache
/// role; it is the same value [`crate::sampling`] returns.
pub type ArrowSample = ArrowData;

/// Largest sampling pass the cache will retain, in samples.
///
/// The worst supported grid is 3D `MAX_DENSITY_3D³ = 40³ = 64_000` samples;
/// 2D's `MAX_DENSITY_2D² = 200² = 40_000` is smaller. The retained entry is
/// therefore at most `SAMPLE_CAP × size_of::<ArrowSample>()` bytes (about
/// 2 MiB for the 32-byte arrow sample), plus the key's small owned strings.
pub const SAMPLE_CAP: usize = 64_000;

/// Everything that can change one sampling pass.
///
/// Exact equality is the invalidation rule: a changed formula, parameter,
/// domain, density, projection, plane frame, field mode or colour input is a
/// miss, never a tolerance match. `view` is part of the key because
/// [`ArrowSample`] carries the painted colour, which is a sampler output.
/// Simulation time is deliberately absent: time-dependent fields bypass the
/// cache entirely, so there is no time-dependent hit to key.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSampleKey {
    /// Formula text as committed, including a text that failed to parse (the
    /// previous field is still sampled, so the conservative miss is correct).
    pub formula: String,
    /// The full committed parameter set: name, value, step and bounds.
    pub parameters: ParameterSet,
    pub domain: Domain,
    /// Committed density, before any mode cap. A cap-only difference is a
    /// harmless extra miss.
    pub density: u32,
    pub mode_3d: bool,
    /// The plane sampler's frame, or `None` for the world-domain sampler.
    pub plane_frame: Option<PlaneFrame>,
    /// The active adapter; only Formula mode reaches this cache today, but the
    /// key keeps the sampler choice explicit.
    pub field_mode: FieldMode,
    /// Colormap/range/colour-mapping inputs: colours are part of the samples.
    pub view: ViewOptions,
}

/// At most one cached sampling pass. `get` returns a hit only for an exactly
/// equal key; `insert` replaces the previous entry or refuses an oversized one.
#[derive(Debug, Default)]
pub struct FieldSampleCache {
    entry: Option<FieldSampleEntry>,
}

#[derive(Debug)]
struct FieldSampleEntry {
    key: FieldSampleKey,
    samples: Vec<ArrowSample>,
}

impl Clone for FieldSampleCache {
    /// Runtime cache only: a cloned state starts empty (and out of history).
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl FieldSampleCache {
    /// The retained samples for `key`, if the last inserted key matched.
    pub fn get(&self, key: &FieldSampleKey) -> Option<&[ArrowSample]> {
        self.entry
            .as_ref()
            .filter(|entry| &entry.key == key)
            .map(|entry| entry.samples.as_slice())
    }

    /// Store `samples` as the single entry. Returns `false` and drops any
    /// previous entry when `samples` exceeds [`SAMPLE_CAP`].
    pub fn insert(&mut self, key: FieldSampleKey, samples: Vec<ArrowSample>) -> bool {
        if samples.len() > SAMPLE_CAP {
            self.entry = None;
            return false;
        }
        self.entry = Some(FieldSampleEntry { key, samples });
        true
    }

    /// Drop the retained entry.
    pub fn clear(&mut self) {
        self.entry = None;
    }

    pub fn is_empty(&self) -> bool {
        self.entry.is_none()
    }

    /// Number of retained samples, or `None` when the cache is empty.
    pub fn sample_count(&self) -> Option<usize> {
        self.entry.as_ref().map(|entry| entry.samples.len())
    }
}
