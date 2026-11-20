//! The persisted user appearance-profile library (issue #85 slice A2a).
//!
//! [`ProfileLibrary`] keeps validated user profiles beside the shipped
//! [`builtins`]. It owns no UI and no `State`: capturing a profile from the
//! live state is [`AppearanceProfile::capture`](crate::appearance_profile::AppearanceProfile::capture),
//! and the Appearance panel wires both in slice A2b.
//!
//! Duplicate names are rejected case-sensitively, the library is capped at
//! [`MAX_USER_PROFILES`] entries, and imports are atomic: a document either
//! merges completely or leaves the library untouched.

use std::collections::HashSet;

use crate::appearance_profile::{
    AppearanceProfile, PROFILE_FORMAT_VERSION, ProfileDocument, builtins,
};

/// Largest number of user profiles the library keeps.
pub const MAX_USER_PROFILES: usize = 32;

/// Validated user profiles plus the shipped built-ins.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileLibrary {
    user: Vec<AppearanceProfile>,
}

impl ProfileLibrary {
    /// Build a library from user profiles, validating each record, rejecting
    /// duplicate names (case-sensitively) and enforcing [`MAX_USER_PROFILES`].
    pub fn new(user: Vec<AppearanceProfile>) -> Result<Self, String> {
        validate_profiles(&user)?;
        if user.len() > MAX_USER_PROFILES {
            return Err(cap_error(user.len()));
        }
        Ok(Self { user })
    }

    /// The user profiles, in library order.
    pub fn user(&self) -> &[AppearanceProfile] {
        &self.user
    }

    /// The number of user profiles.
    pub fn len(&self) -> usize {
        self.user.len()
    }

    /// Whether the library holds no user profiles.
    pub fn is_empty(&self) -> bool {
        self.user.is_empty()
    }

    /// Insert a validated profile, replacing a same-named entry in place (order
    /// preserved) or appending a new one. Replacing never grows the library.
    pub fn upsert(&mut self, profile: AppearanceProfile) -> Result<(), String> {
        profile
            .validate()
            .map_err(|error| format!("invalid profile {:?}: {error}", profile.name))?;
        if let Some(existing) = self
            .user
            .iter_mut()
            .find(|existing| existing.name == profile.name)
        {
            *existing = profile;
            return Ok(());
        }
        if self.user.len() >= MAX_USER_PROFILES {
            return Err(cap_error(self.user.len() + 1));
        }
        self.user.push(profile);
        Ok(())
    }

    /// Remove the profile named `name`; reports whether one was removed.
    pub fn remove(&mut self, name: &str) -> bool {
        let Some(index) = self.user.iter().position(|profile| profile.name == name) else {
            return false;
        };
        self.user.remove(index);
        true
    }

    /// The shipped looks in [`builtins`] order, followed by the user profiles.
    pub fn all(&self) -> Vec<AppearanceProfile> {
        let mut all = builtins();
        all.extend(self.user.iter().cloned());
        all
    }

    /// Parse a [`ProfileDocument`] and merge every profile atomically, returning
    /// the user-profile count after the merge. Any error (malformed document,
    /// duplicate or invalid profile, over-cap result) leaves the library
    /// unchanged.
    pub fn import_text(&mut self, text: &str) -> Result<usize, String> {
        let document = ProfileDocument::from_json(text)?;
        let mut candidate = self.clone();
        for profile in document.profiles {
            candidate.upsert(profile)?;
        }
        *self = candidate;
        Ok(self.user.len())
    }

    /// Serialize the user profiles as a compact [`ProfileDocument`] JSON text.
    ///
    /// Every stored profile is already validated, so the only failure is an
    /// internal serialization error; it logs a warning and returns an empty
    /// string rather than panicking.
    pub fn export_text(&self) -> String {
        let document = ProfileDocument {
            version: PROFILE_FORMAT_VERSION,
            profiles: self.user.clone(),
        };
        document.to_json().unwrap_or_else(|error| {
            log::warn!("user appearance profiles could not be exported: {error}");
            String::new()
        })
    }
}

fn validate_profiles(profiles: &[AppearanceProfile]) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(profiles.len());
    for profile in profiles {
        profile
            .validate()
            .map_err(|error| format!("invalid profile {:?}: {error}", profile.name))?;
        if !seen.insert(profile.name.as_str()) {
            return Err(format!(
                "duplicate profile name {:?}; profile names must be unique",
                profile.name
            ));
        }
    }
    Ok(())
}

fn cap_error(count: usize) -> String {
    format!("at most {MAX_USER_PROFILES} user profiles are supported; got {count}")
}
