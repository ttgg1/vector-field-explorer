// Ticket 42: the compile-time version seam. A nightly build stamps
// `VFE_BUILD_VERSION`; every other build falls back to the crate version so
// About never shows an empty or stale string.

use vector_field_explorer::version::{CARGO_VERSION, VERSION, resolve_version};

use std::path::PathBuf;

const NIGHTLY: &str = "0.1.0-nightly.20261109+1a2b3c4";

#[test]
fn version_without_an_override_is_the_crate_version() {
    assert_eq!(VERSION, CARGO_VERSION);
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn a_build_override_wins() {
    assert_eq!(resolve_version(Some(NIGHTLY)), NIGHTLY);
}

#[test]
fn an_unset_override_falls_back_to_the_crate_version() {
    assert_eq!(resolve_version(None), CARGO_VERSION);
}

#[test]
fn an_empty_override_falls_back_to_the_crate_version() {
    // The shared packaging workflow always exports VFE_BUILD_VERSION; a
    // versioned release passes an empty value, which must not blank About.
    assert_eq!(resolve_version(Some("")), CARGO_VERSION);
}

#[test]
fn the_startup_log_reports_the_build_version() {
    // The native entry point has no test harness, so pin the source: the
    // startup line must use the same seam as About, not the crate version.
    let main =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
            .expect("read src/main.rs");
    assert!(
        !main.contains("env!(\"CARGO_PKG_VERSION\")"),
        "the startup log must not read the crate version directly"
    );
    assert!(
        main.contains("version::VERSION"),
        "the startup log must use version::VERSION so it matches About"
    );
}
