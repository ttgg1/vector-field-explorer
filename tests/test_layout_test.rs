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

/// The single suite harness (`tests/suite.rs`) lists every test file as a
/// module (ticket 21). A new `tests/*_test.rs` that nobody registers would
/// silently never run, so this guard fails on an unregistered file and pins the
/// test inventory: deleting a real test drops the count below the baseline.
#[test]
fn every_top_level_test_file_is_registered_in_the_suite() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let suite = fs::read_to_string(tests.join("suite.rs")).expect("read tests/suite.rs");
    let mut missing = Vec::new();
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
        if !suite.lines().any(|line| line.trim() == format!("mod {stem};")) {
            missing.push(stem.to_string());
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        test_attributes += text.lines().filter(|line| line.trim() == "#[test]").count();
    }

    assert!(
        files > 40,
        "the walk must find the test files, found {files} under {}",
        tests.display(),
    );
    assert!(
        missing.is_empty(),
        "tests/suite.rs must include every tests/*_test.rs as a module; missing: {}",
        missing.join(", "),
    );
    // 704 tests at the ticket-21 baseline, plus this guard (and one `#[test]`
    // line whose sibling platform gate excludes it from the harness).
    assert!(
        test_attributes >= 706,
        "the suite lost tests: found {test_attributes} `#[test]` lines (baseline 706)",
    );
}
