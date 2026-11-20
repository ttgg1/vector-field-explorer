//! Pure, searchable catalogue over the existing built-in presets.
//!
//! This module deliberately owns no physics and no UI state. It indexes the
//! formula presets from [`crate::presets`] and the source scenes from
//! [`crate::source_presets`] with stable [`PresetId`]s and the metadata later
//! panel work needs: a display name, summary, group, filter fields, documented
//! assumptions, and (eventually) named parameters and exploration prompts.
//!
//! Ordering is deterministic: formula presets follow [`presets()`] order and
//! source presets follow [`source_presets()`] order. IDs are unique across the
//! whole catalogue; construction fails rather than overwriting a duplicate.

use std::collections::HashSet;
use std::fmt;

use crate::presets::{Preset, PresetCategory, preset_approximation_label, presets};
use crate::source_presets::{SourcePreset, source_presets};

/// The longest accepted [`PresetId`], in bytes.
pub const PRESET_ID_MAX_BYTES: usize = 64;

/// The source preset that places one of every source kind; the only entry in
/// [`PresetGroup::Gallery`].
const SOURCE_GALLERY_NAME: &str = "Source gallery";

/// A validated, stable identifier for one catalogue entry.
///
/// The serialized form is the ID itself: ASCII `[a-z0-9]` first, then
/// `[a-z0-9_.-]`, 1..=[`PRESET_ID_MAX_BYTES`] bytes. Formula entries look like
/// `formula.vortex`; source entries look like `source.coulomb_point`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PresetId(String);

impl PresetId {
    /// Validate and wrap an ID. Never panics; malformed input returns a
    /// descriptive [`PresetIdError`].
    pub fn new(value: &str) -> Result<Self, PresetIdError> {
        if value.is_empty() {
            return Err(PresetIdError::Empty);
        }
        if value.len() > PRESET_ID_MAX_BYTES {
            return Err(PresetIdError::TooLong { bytes: value.len() });
        }
        if !value.is_ascii() {
            return Err(PresetIdError::NonAscii);
        }

        let first = value.as_bytes()[0] as char;
        if !is_id_start(first) {
            return Err(PresetIdError::InvalidFirst { character: first });
        }
        for byte in &value.as_bytes()[1..] {
            let character = *byte as char;
            if !is_id_continue(character) {
                return Err(PresetIdError::InvalidCharacter { character });
            }
        }

        Ok(Self(value.to_owned()))
    }

    /// The validated ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PresetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for PresetId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn is_id_start(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit()
}

fn is_id_continue(character: char) -> bool {
    is_id_start(character) || matches!(character, '_' | '.' | '-')
}

/// Why a candidate [`PresetId`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetIdError {
    /// The candidate was empty.
    Empty,
    /// The candidate exceeded [`PRESET_ID_MAX_BYTES`].
    TooLong { bytes: usize },
    /// The candidate was not ASCII.
    NonAscii,
    /// The first byte was not a lowercase letter or digit.
    InvalidFirst { character: char },
    /// A later byte was outside `[a-z0-9_.-]`.
    InvalidCharacter { character: char },
}

impl fmt::Display for PresetIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("preset id must not be empty"),
            Self::TooLong { bytes } => write!(
                formatter,
                "preset id must be at most {PRESET_ID_MAX_BYTES} bytes, got {bytes}"
            ),
            Self::NonAscii => formatter.write_str("preset id must be ASCII"),
            Self::InvalidFirst { character } => write!(
                formatter,
                "preset id must start with a lowercase letter or digit, got `{character}`"
            ),
            Self::InvalidCharacter { character } => write!(
                formatter,
                "preset id contains an invalid character `{character}`; \
                 allowed characters are a-z, 0-9, `_`, `.` and `-`"
            ),
        }
    }
}

impl std::error::Error for PresetIdError {}

/// The catalogue section an entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresetGroup {
    /// Kinematic demos and generic flows from [`PresetCategory::Classic`].
    Classic,
    /// Electrical and magnetic physics formulas.
    Physics,
    /// Source-scene presets.
    Sources,
    /// The source gallery (the last source preset).
    Gallery,
}

/// Filter fields carried by every entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PresetFilters {
    /// Formula dimension; `None` for dimension-agnostic source scenes.
    pub dimension: Option<usize>,
    /// Whether the preset animates over time. Every current preset is static.
    pub animated: bool,
}

/// One indexed preset.
#[derive(Debug, Clone)]
pub struct PresetEntry {
    /// Stable, unique ID.
    pub id: PresetId,
    /// Display name from the owning catalogue.
    pub name: &'static str,
    /// One-line explanation; empty when the catalogue has none.
    pub summary: &'static str,
    /// Catalogue section.
    pub group: PresetGroup,
    /// Filter metadata.
    pub filters: PresetFilters,
    /// Documented approximation, empty when the preset makes no claim.
    pub assumptions: &'static str,
    /// Named parameter bindings; empty until a preset uses them.
    pub parameters: &'static [&'static str],
    /// Optional exploration prompt; empty when none is supplied.
    pub exploration_prompt: &'static str,
}

/// Why a [`PresetLibrary`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetLibraryError {
    /// A catalogue name produced an invalid ID.
    InvalidId {
        /// The preset name that produced the ID.
        preset: &'static str,
        /// The validation failure.
        error: PresetIdError,
    },
    /// The same ID appeared twice.
    DuplicateId(PresetId),
}

impl fmt::Display for PresetLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId { preset, error } => {
                write!(formatter, "preset `{preset}` has an invalid id: {error}")
            }
            Self::DuplicateId(id) => write!(formatter, "duplicate preset id `{id}`"),
        }
    }
}

impl std::error::Error for PresetLibraryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidId { error, .. } => Some(error),
            Self::DuplicateId(_) => None,
        }
    }
}

/// An ordered, indexed collection of presets.
#[derive(Debug, Clone)]
pub struct PresetLibrary {
    entries: Vec<PresetEntry>,
}

impl PresetLibrary {
    /// Build the catalogue over the current built-in formula and source
    /// presets. Fails only if a name cannot form a valid ID or two entries
    /// collide.
    pub fn builtin() -> Result<Self, PresetLibraryError> {
        let mut entries = Vec::with_capacity(presets().len() + source_presets().len());
        for preset in presets() {
            entries.push(formula_entry(preset)?);
        }
        for preset in source_presets() {
            entries.push(source_entry(preset)?);
        }
        Self::from_entries(entries)
    }

    /// Build from an explicit entry list in catalogue order, rejecting
    /// duplicate IDs.
    pub fn from_entries(entries: Vec<PresetEntry>) -> Result<Self, PresetLibraryError> {
        let mut seen = HashSet::with_capacity(entries.len());
        for entry in &entries {
            if !seen.insert(entry.id.clone()) {
                return Err(PresetLibraryError::DuplicateId(entry.id.clone()));
            }
        }
        Ok(Self { entries })
    }

    /// All entries in catalogue order.
    pub fn entries(&self) -> &[PresetEntry] {
        &self.entries
    }

    /// Number of indexed presets.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalogue is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries whose name, summary or ID contains `query`, trimmed and
    /// case-insensitively. A blank query returns every entry. Results preserve
    /// catalogue order.
    pub fn search(&self, query: &str) -> Vec<&PresetEntry> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return self.entries.iter().collect();
        }
        self.entries
            .iter()
            .filter(|entry| {
                entry.name.to_ascii_lowercase().contains(&needle)
                    || entry.summary.to_ascii_lowercase().contains(&needle)
                    || entry.id.as_str().contains(&needle)
            })
            .collect()
    }

    /// Entries matching the filter fields. `dimension: Some(n)` keeps formulas
    /// of dimension `n` and every dimension-agnostic source; `animated: true`
    /// keeps only animated entries, while `false` applies no animated filter.
    pub fn filtered(&self, filters: PresetFilters) -> Vec<&PresetEntry> {
        self.entries
            .iter()
            .filter(|entry| matches_filters(entry, filters))
            .collect()
    }

    /// [`Self::search`] narrowed by [`Self::filtered`], equivalent to
    /// filtering the search results by the same predicates.
    pub fn search_filtered(&self, query: &str, filters: PresetFilters) -> Vec<&PresetEntry> {
        self.search(query)
            .into_iter()
            .filter(|entry| matches_filters(entry, filters))
            .collect()
    }

    /// The entry with `id`, if indexed.
    pub fn get(&self, id: &PresetId) -> Option<&PresetEntry> {
        self.entries.iter().find(|entry| entry.id == *id)
    }
}

fn matches_filters(entry: &PresetEntry, filters: PresetFilters) -> bool {
    let dimension_matches = match filters.dimension {
        Some(dimension) => {
            entry.filters.dimension.is_none() || entry.filters.dimension == Some(dimension)
        }
        None => true,
    };
    let animated_matches = !filters.animated || entry.filters.animated;
    dimension_matches && animated_matches
}

fn formula_entry(preset: &'static Preset) -> Result<PresetEntry, PresetLibraryError> {
    let id = derive_id("formula", preset.name)?;
    Ok(PresetEntry {
        id,
        name: preset.name,
        summary: "",
        group: match preset.category {
            PresetCategory::Classic => PresetGroup::Classic,
            PresetCategory::Electrostatics
            | PresetCategory::Magnetostatics
            | PresetCategory::Electrodynamics => PresetGroup::Physics,
        },
        filters: PresetFilters {
            dimension: Some(preset.dimension),
            animated: false,
        },
        assumptions: preset_approximation_label(preset.name).unwrap_or(""),
        parameters: &[],
        exploration_prompt: "",
    })
}

fn source_entry(preset: &'static SourcePreset) -> Result<PresetEntry, PresetLibraryError> {
    let id = derive_id("source", preset.name)?;
    let group = if preset.name == SOURCE_GALLERY_NAME {
        PresetGroup::Gallery
    } else {
        PresetGroup::Sources
    };
    Ok(PresetEntry {
        id,
        name: preset.name,
        summary: preset.summary,
        group,
        filters: PresetFilters {
            dimension: None,
            animated: false,
        },
        assumptions: "",
        parameters: &[],
        exploration_prompt: "",
    })
}

fn derive_id(namespace: &str, name: &'static str) -> Result<PresetId, PresetLibraryError> {
    let candidate = format!("{namespace}.{}", slug(name));
    PresetId::new(&candidate).map_err(|error| PresetLibraryError::InvalidId {
        preset: name,
        error,
    })
}

/// Lowercase `name`, replacing each run of non-`[a-z0-9]` bytes with `_`.
fn slug(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut in_separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            in_separator = false;
        } else if !in_separator {
            slug.push('_');
            in_separator = true;
        }
    }
    slug
}
