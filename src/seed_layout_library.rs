//! The named seed-layout library (issue #87 slice S3a).
//!
//! [`SeedLayoutLibrary`] keeps validated user [`SeedLayoutProfile`]s beside the
//! built-in empty `Default` layout. It owns no UI and no `State`: applying a
//! layout, the Flow-section panel and history wiring arrive in later slices.
//!
//! The built-in `Default` is the empty layout (the automatic grid). It is never
//! stored, exported or deleted. User layouts live in the settings envelope,
//! capped at [`MAX_USER_LAYOUTS`], validated in full, with duplicate names
//! rejected case-sensitively. Imports are atomic: a document either merges
//! completely or leaves the library untouched.
//!
//! Portable documents use [`SEED_LAYOUT_FORMAT_VERSION`], reject unknown
//! fields and export compact JSON text. Because a seed's validity depends on
//! the scene domain, [`SeedLayoutDocument::to_json`] and
//! [`SeedLayoutDocument::from_json`] check the version, names and uniqueness;
//! [`SeedLayoutDocument::validate`] additionally checks every seed against a
//! supplied [`Domain`], and [`SeedLayoutLibrary::import_text`] runs that full
//! check before merging.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::domain::Domain;
use crate::seed_layout::SeedLayout;

/// The portable seed-layout document format version.
pub const SEED_LAYOUT_FORMAT_VERSION: u32 = 1;

/// The most UTF-8 bytes a layout name may occupy.
pub const MAX_LAYOUT_NAME_BYTES: usize = 48;

/// The most user layouts the library keeps.
pub const MAX_USER_LAYOUTS: usize = 16;

/// The name of the built-in empty layout.
pub const DEFAULT_LAYOUT_NAME: &str = "Default";

/// The stable name substitution used when a stored name cannot be repaired.
const FALLBACK_LAYOUT_NAME: &str = "Layout";

/// One named, portable seed layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedLayoutProfile {
    /// Trimmed, non-empty, at most [`MAX_LAYOUT_NAME_BYTES`] UTF-8 bytes and
    /// never [`DEFAULT_LAYOUT_NAME`].
    pub name: String,
    /// The seeds this layout stores.
    pub seeds: SeedLayout,
}

impl SeedLayoutProfile {
    /// Reject an invalid name or any seed [`SeedLayout::validate`] refuses.
    pub fn validate(&self, domain: &Domain) -> Result<(), String> {
        validate_name(&self.name)?;
        self.seeds.validate(domain)
    }

    /// Repair a stored record without ever failing.
    ///
    /// The name is trimmed, truncated on a UTF-8 character boundary to
    /// [`MAX_LAYOUT_NAME_BYTES`], and reset to the stable placeholder
    /// `"Layout"` when trimming leaves it empty or when it names the built-in
    /// `Default`. Invalid seeds are dropped and the remainder is capped by
    /// [`SeedLayout::sanitized`]. The result always validates against a domain
    /// the seeds were sanitised with.
    pub fn sanitized(self, domain: &Domain) -> Self {
        Self {
            name: sanitize_name(&self.name),
            seeds: self.seeds.sanitized(domain),
        }
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("layout name must not be empty".to_owned());
    }
    if name.trim() != name {
        return Err("layout name must be trimmed".to_owned());
    }
    if name.len() > MAX_LAYOUT_NAME_BYTES {
        return Err(format!(
            "layout name is {} bytes; the limit is {MAX_LAYOUT_NAME_BYTES}",
            name.len()
        ));
    }
    if name == DEFAULT_LAYOUT_NAME {
        return Err(format!(
            "{DEFAULT_LAYOUT_NAME:?} is a built-in layout and cannot be used as a user layout name"
        ));
    }
    Ok(())
}

fn version_error(version: u32) -> String {
    format!(
        "seed layout version {version} is not supported; expected version \
         {SEED_LAYOUT_FORMAT_VERSION}"
    )
}

fn sanitize_name(name: &str) -> String {
    let mut cleaned = name.trim().to_owned();
    if cleaned.len() > MAX_LAYOUT_NAME_BYTES {
        let mut end = MAX_LAYOUT_NAME_BYTES;
        while !cleaned.is_char_boundary(end) {
            end -= 1;
        }
        cleaned.truncate(end);
        cleaned = cleaned.trim_end().to_owned();
    }
    if cleaned.is_empty() || cleaned == DEFAULT_LAYOUT_NAME {
        return FALLBACK_LAYOUT_NAME.to_owned();
    }
    cleaned
}

/// A strict, versioned JSON document holding seed layouts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedLayoutDocument {
    /// Document format version; must be [`SEED_LAYOUT_FORMAT_VERSION`].
    pub version: u32,
    /// Layouts in document order.
    pub layouts: Vec<SeedLayoutProfile>,
}

impl SeedLayoutDocument {
    /// Check the version, every name and case-sensitive uniqueness without a
    /// domain, so JSON helpers can reject structural problems.
    fn validate_structure(&self) -> Result<(), String> {
        if self.version != SEED_LAYOUT_FORMAT_VERSION {
            return Err(version_error(self.version));
        }
        let mut seen = HashSet::with_capacity(self.layouts.len());
        for layout in &self.layouts {
            validate_name(&layout.name)
                .map_err(|error| format!("invalid layout {:?}: {error}", layout.name))?;
            if !seen.insert(layout.name.as_str()) {
                return Err(format!(
                    "duplicate layout name {:?}; layout names must be unique",
                    layout.name
                ));
            }
        }
        Ok(())
    }

    /// Check the version first, then every profile (including its seeds
    /// against `domain`), then case-sensitive name uniqueness.
    pub fn validate(&self, domain: &Domain) -> Result<(), String> {
        if self.version != SEED_LAYOUT_FORMAT_VERSION {
            return Err(version_error(self.version));
        }
        for layout in &self.layouts {
            layout
                .validate(domain)
                .map_err(|error| format!("invalid layout {:?}: {error}", layout.name))?;
        }
        let mut seen = HashSet::with_capacity(self.layouts.len());
        for layout in &self.layouts {
            if !seen.insert(layout.name.as_str()) {
                return Err(format!(
                    "duplicate layout name {:?}; layout names must be unique",
                    layout.name
                ));
            }
        }
        Ok(())
    }

    /// Serialize a structurally valid document as compact JSON with pinned
    /// field order: `version`, `layouts`; per layout `name`, `seeds`; per seed
    /// `position`, `anchor`; per anchor `axis`, `index`, `u`, `v`.
    ///
    /// Seed positions are domain-dependent, so this cannot run the domain
    /// validation; the library only exports already validated profiles.
    pub fn to_json(&self) -> Result<String, String> {
        self.validate_structure()
            .map_err(|error| format!("invalid seed layouts: {error}"))?;
        serde_json::to_string(self).map_err(|error| format!("invalid seed layout JSON: {error}"))
    }

    /// Parse a document, rejecting unknown fields. The version is probed
    /// first, so a future version is reported before any layout body is
    /// parsed. Structural name and duplicate checks run too; seed positions
    /// are checked by [`Self::validate`] once a domain is known.
    pub fn from_json(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct VersionProbe {
            version: u32,
        }

        let probe: VersionProbe = serde_json::from_str(text)
            .map_err(|error| format!("invalid seed layout JSON: {error}"))?;
        if probe.version != SEED_LAYOUT_FORMAT_VERSION {
            return Err(format!(
                "invalid seed layouts: seed layout version {} is not supported; \
                 expected version {SEED_LAYOUT_FORMAT_VERSION}",
                probe.version
            ));
        }

        let document: Self = serde_json::from_str(text)
            .map_err(|error| format!("invalid seed layout JSON: {error}"))?;
        document
            .validate_structure()
            .map_err(|error| format!("invalid seed layouts: {error}"))?;
        Ok(document)
    }
}

/// Validated user seed layouts beside the built-in empty `Default`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeedLayoutLibrary {
    user: Vec<SeedLayoutProfile>,
}

impl SeedLayoutLibrary {
    /// Build a library from user layouts, validating each record against
    /// `domain`, rejecting the built-in name, duplicate names
    /// (case-sensitively) and more than [`MAX_USER_LAYOUTS`] entries.
    pub fn new(user: Vec<SeedLayoutProfile>, domain: &Domain) -> Result<Self, String> {
        validate_profiles(&user, domain)?;
        if user.len() > MAX_USER_LAYOUTS {
            return Err(cap_error(user.len()));
        }
        Ok(Self { user })
    }

    /// The user layouts, in library order.
    pub fn user(&self) -> &[SeedLayoutProfile] {
        &self.user
    }

    /// The number of user layouts.
    pub fn len(&self) -> usize {
        self.user.len()
    }

    /// Whether the library holds no user layouts.
    pub fn is_empty(&self) -> bool {
        self.user.is_empty()
    }

    /// Insert a validated layout, replacing a same-named entry in place (order
    /// preserved) or appending a new one. The built-in name and the cap are
    /// refused; a rejected upsert changes nothing.
    pub fn upsert(&mut self, profile: SeedLayoutProfile, domain: &Domain) -> Result<(), String> {
        profile
            .validate(domain)
            .map_err(|error| format!("invalid layout {:?}: {error}", profile.name))?;
        if profile.name == DEFAULT_LAYOUT_NAME {
            return Err(format!(
                "{DEFAULT_LAYOUT_NAME:?} is a built-in layout and cannot be overwritten"
            ));
        }
        if let Some(existing) = self
            .user
            .iter_mut()
            .find(|existing| existing.name == profile.name)
        {
            *existing = profile;
            return Ok(());
        }
        if self.user.len() >= MAX_USER_LAYOUTS {
            return Err(cap_error(self.user.len() + 1));
        }
        self.user.push(profile);
        Ok(())
    }

    /// Remove the user layout named `name`; the built-in is never removed.
    pub fn remove(&mut self, name: &str) -> bool {
        if name == DEFAULT_LAYOUT_NAME {
            return false;
        }
        let Some(index) = self.user.iter().position(|layout| layout.name == name) else {
            return false;
        };
        self.user.remove(index);
        true
    }

    /// The built-in empty `Default` first, then the user layouts in library
    /// order. The built-in is created on demand and never stored.
    pub fn all(&self, _domain: &Domain) -> Vec<SeedLayoutProfile> {
        let mut all = Vec::with_capacity(self.user.len() + 1);
        all.push(SeedLayoutProfile {
            name: DEFAULT_LAYOUT_NAME.to_owned(),
            seeds: SeedLayout::default(),
        });
        all.extend(self.user.iter().cloned());
        all
    }

    /// Parse a [`SeedLayoutDocument`], validate every seed against `domain`
    /// and merge every layout atomically, returning the user-layout count after
    /// the merge. Any error leaves the library unchanged.
    pub fn import_text(&mut self, text: &str, domain: &Domain) -> Result<usize, String> {
        let document = SeedLayoutDocument::from_json(text)?;
        document
            .validate(domain)
            .map_err(|error| format!("invalid seed layouts: {error}"))?;
        let mut candidate = self.clone();
        for layout in document.layouts {
            candidate.upsert(layout, domain)?;
        }
        *self = candidate;
        Ok(self.user.len())
    }

    /// Serialize the user layouts as compact [`SeedLayoutDocument`] JSON text,
    /// or an empty string when the library has no user layouts. The built-in
    /// is never exported.
    pub fn export_text(&self) -> String {
        if self.user.is_empty() {
            return String::new();
        }
        let document = SeedLayoutDocument {
            version: SEED_LAYOUT_FORMAT_VERSION,
            layouts: self.user.clone(),
        };
        document.to_json().unwrap_or_else(|error| {
            log::warn!("user seed layouts could not be exported: {error}");
            String::new()
        })
    }
}

fn validate_profiles(profiles: &[SeedLayoutProfile], domain: &Domain) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(profiles.len());
    for profile in profiles {
        profile
            .validate(domain)
            .map_err(|error| format!("invalid layout {:?}: {error}", profile.name))?;
        if profile.name == DEFAULT_LAYOUT_NAME {
            return Err(format!(
                "{DEFAULT_LAYOUT_NAME:?} is a built-in layout and cannot be stored twice"
            ));
        }
        if !seen.insert(profile.name.as_str()) {
            return Err(format!(
                "duplicate layout name {:?}; layout names must be unique",
                profile.name
            ));
        }
    }
    Ok(())
}

fn cap_error(count: usize) -> String {
    format!("at most {MAX_USER_LAYOUTS} user layouts are supported; got {count}")
}
