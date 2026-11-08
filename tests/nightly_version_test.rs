// Ticket 42: `scripts/nightly-version.sh` formats the nightly version and
// `scripts/crate-version.sh` reads the base version from Cargo.toml. Both are
// pure — no clock, no git checkout — so the format contract is testable here.
//
// The workflow supplies `date -u +%Y%m%d` and `git rev-parse HEAD`; the script
// owns validation and the `<base>-nightly.<YYYYMMDD>+<short-sha>` shape.

use std::path::PathBuf;
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(script: &str, args: &[&str]) -> Output {
    Command::new("bash")
        .arg(repo_root().join(script))
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("run repo script")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn formats_the_nightly_version() {
    let out = run(
        "scripts/nightly-version.sh",
        &["0.1.0", "20261109", "1a2b3c4deadbeef"],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "0.1.0-nightly.20261109+1a2b3c4");
}

#[test]
fn shortens_and_lowercases_the_sha() {
    let out = run(
        "scripts/nightly-version.sh",
        &["0.1.0", "20261109", "1A2B3C4DEADBEEF"],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "0.1.0-nightly.20261109+1a2b3c4");
}

#[test]
fn keeps_a_prerelease_base_version() {
    let out = run(
        "scripts/nightly-version.sh",
        &["0.1.0-rc.1", "20261109", "1a2b3c4"],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "0.1.0-rc.1-nightly.20261109+1a2b3c4");
}

#[test]
fn rejects_a_bad_base_version_date_or_sha() {
    let cases: &[(&[&str], &str)] = &[
        (&["", "20261109", "1a2b3c4"], "base version"),
        (&["0.1", "20261109", "1a2b3c4"], "not semver"),
        (&["0.1.0", "2026119", "1a2b3c4"], "YYYYMMDD"),
        (&["0.1.0", "20261309", "1a2b3c4"], "calendar date"),
        (&["0.1.0", "20261132", "1a2b3c4"], "calendar date"),
        (&["0.1.0", "20261109", "1a2b3c"], "7-40 hex"),
        (&["0.1.0", "20261109", "nothex!"], "7-40 hex"),
    ];
    for (args, needle) in cases {
        let out = run("scripts/nightly-version.sh", args);
        assert!(
            !out.status.success(),
            "expected failure for {args:?}, got {:?}",
            stdout(&out)
        );
        assert!(
            stderr(&out).contains(needle),
            "for {args:?} expected stderr to contain {needle:?}, got {:?}",
            stderr(&out)
        );
    }
}

#[test]
fn missing_arguments_print_usage() {
    let out = run("scripts/nightly-version.sh", &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("usage: scripts/nightly-version.sh"));
}

#[test]
fn crate_version_script_matches_the_package_version() {
    let out = run("scripts/crate-version.sh", &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), env!("CARGO_PKG_VERSION"));
}
