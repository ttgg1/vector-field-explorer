//! Layout guard (ticket 13): production code lives in `src/`, every test lives
//! in `tests/`.
//!
//! Integration tests under `tests/` compile as separate crates and can only
//! reach `pub` items, so an inline `#[cfg(test)]` module in `src/` silently
//! widens what a test may touch. This guard fails the suite as soon as one
//! comes back: it walks every `.rs` file under `src/` and rejects the
//! `#[cfg(test)]` attribute.

use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir`, recursively.
fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext.to_str() == Some("rs")) {
            out.push(path);
        }
    }
}

#[test]
fn src_carries_no_inline_test_modules() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    rust_sources(&src, &mut sources);
    assert!(
        sources.len() > 10,
        "the walk must find the source tree, found {} files under {}",
        sources.len(),
        src.display(),
    );

    let mut offenders = Vec::new();
    for path in &sources {
        let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        if text.contains("#[cfg(test)]") {
            offenders.push(path.display().to_string());
        }
    }

    assert!(
        offenders.is_empty(),
        "tests belong in tests/, not in src/; move the inline modules out of:\n{}",
        offenders.join("\n"),
    );
}

/// The `name` of every `[[test]]` target declared in `Cargo.toml`.
fn declared_test_targets(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_test = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_test = line == "[[test]]";
            continue;
        }
        if !in_test {
            continue;
        }
        let Some(value) = line.strip_prefix("name") else {
            continue;
        };
        let Some(value) = value.trim_start().strip_prefix('=') else {
            continue;
        };
        names.push(value.trim().trim_matches('"').to_string());
    }
    names
}

/// Every test file must be registered for the single suite harness, either as
/// a `mod` in `tests/suite.rs` (ticket 21) or as its own `[[test]]` target when
/// it needs process isolation (for example `config_test` and its `set_var`).
/// An unregistered `tests/*_test.rs` would silently never run, so this guard
/// fails on one and pins the test inventory: deleting a real test drops the
/// count below the baseline. It also rejects `#[ignore]`, because ticket 21
/// requires every test to run.
#[test]
fn every_top_level_test_file_is_registered_in_the_suite() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests = manifest_dir.join("tests");
    let suite = fs::read_to_string(tests.join("suite.rs")).expect("read tests/suite.rs");
    let manifest = fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("read Cargo.toml");
    let test_targets = declared_test_targets(&manifest);
    let mut missing = Vec::new();
    let mut ignored = Vec::new();
    let mut test_attributes = 0usize;
    let mut files = 0usize;

    for entry in fs::read_dir(&tests).expect("read tests/") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).expect("utf-8 file stem");
        if stem == "suite" {
            continue;
        }
        files += 1;
        let in_suite = suite.lines().any(|line| line.trim() == format!("mod {stem};"));
        let own_target = test_targets.iter().any(|name| name == stem);
        if !in_suite && !own_target {
            missing.push(stem.to_string());
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        test_attributes += text.lines().filter(|line| line.trim() == "#[test]").count();
        if text.lines().any(|line| line.trim().starts_with("#[ignore")) {
            ignored.push(stem.to_string());
        }
    }

    assert!(
        files > 40,
        "the walk must find the test files, found {files} under {}",
        tests.display(),
    );
    assert!(
        missing.is_empty(),
        "every tests/*_test.rs must be registered as a `mod` in tests/suite.rs \
         or as its own `[[test]]` target in Cargo.toml; missing: {}",
        missing.join(", "),
    );
    assert!(
        ignored.is_empty(),
        "ticket 21 requires every test to run; remove #[ignore] from: {}",
        ignored.join(", "),
    );
    // 704 tests at the ticket-21 baseline, plus this guard (and one `#[test]`
    // line whose sibling platform gate excludes it from the harness).
    assert!(
        test_attributes >= 706,
        "the suite lost tests: found {test_attributes} `#[test]` lines (baseline 706)",
    );
}
