//! The compile-time version seam (ticket 42).
//!
//! A nightly build sets `VFE_BUILD_VERSION` when it is compiled, so the About
//! dialog and any diagnostics identify the exact `main` commit. Every other
//! build falls back to the crate version, which a release tag already pins.
//!
//! The workflow formats the nightly string with
//! `scripts/nightly-version.sh`; this module only chooses which string the
//! binary reports.

/// The crate version from `Cargo.toml`.
pub const CARGO_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version shown in About: `VFE_BUILD_VERSION` when it is set and
/// non-empty, otherwise [`CARGO_VERSION`].
pub const VERSION: &str = resolve_version(option_env!("VFE_BUILD_VERSION"));

/// Pick the build version. An unset or empty override falls back to the crate
/// version, so the shared packaging workflow can always export the variable
/// (a versioned release passes an empty value) without blanking About.
pub const fn resolve_version(override_version: Option<&str>) -> &str {
    match override_version {
        Some(version) => {
            if version.is_empty() {
                CARGO_VERSION
            } else {
                version
            }
        }
        None => CARGO_VERSION,
    }
}
