// Ticket 42: `scripts/nightly-version.sh` formats the nightly version and
// `scripts/crate-version.sh` reads the base version from Cargo.toml. Both are
// pure — no clock, no git checkout — so the format contract is testable here.
//
// The workflow supplies `date -u +%Y%m%d` and `git rev-parse HEAD`; the script
// owns validation and the `<base>-nightly.<YYYYMMDD>+<short-sha>` shape.

use std::fs;
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
fn accepts_a_leap_day() {
    let out = run(
        "scripts/nightly-version.sh",
        &["0.1.0", "20240229", "1a2b3c4"],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "0.1.0-nightly.20240229+1a2b3c4");
}

#[test]
fn rejects_a_bad_base_version_date_or_sha() {
    let cases: &[(&[&str], &str)] = &[
        (&["", "20261109", "1a2b3c4"], "base version"),
        (&["0.1", "20261109", "1a2b3c4"], "not semver"),
        (&["0.1.0", "2026119", "1a2b3c4"], "YYYYMMDD"),
        (&["0.1.0", "20261309", "1a2b3c4"], "calendar date"),
        (&["0.1.0", "20261132", "1a2b3c4"], "calendar date"),
        // Real calendar dates, not just ranges: February never has 30 days,
        // and 2025 is not a leap year.
        (&["0.1.0", "20260230", "1a2b3c4"], "calendar date"),
        (&["0.1.0", "20250229", "1a2b3c4"], "calendar date"),
        (&["0.1.0", "20260001", "1a2b3c4"], "calendar date"),
        (&["0.1.0", "20260100", "1a2b3c4"], "calendar date"),
        (&["0.1.0", "20260431", "1a2b3c4"], "calendar date"),
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

// The release job's command order is a safety property: a failure must never
// leave `nightly` pointing at a new commit with the previous night's binaries.
// These are static checks over the workflow text, so they fail on a reorder
// without needing GitHub or `gh`.
fn nightly_workflow() -> String {
    fs::read_to_string(repo_root().join(".github/workflows/nightly.yml"))
        .expect("read .github/workflows/nightly.yml")
}

fn release_job(workflow: &str) -> &str {
    workflow
        .split_once("\n  release:\n")
        .expect("nightly workflow has a release job")
        .1
}

/// The shell command that starts at `start`, following backslash continuations.
fn command_at(text: &str, start: usize) -> &str {
    let mut end = start;
    for line in text[start..].split_inclusive('\n') {
        end += line.len();
        if !line.trim_end().ends_with('\\') {
            break;
        }
    }
    &text[start..end]
}

#[test]
fn the_release_job_uploads_assets_before_moving_the_tag() {
    let workflow = nightly_workflow();
    let release = release_job(&workflow);
    let upload = release
        .find("gh release upload")
        .expect("the release job uploads assets");
    let tag_push = release
        .find("git push --force origin refs/tags/nightly")
        .expect("the release job force-moves the nightly tag");
    assert!(
        upload < tag_push,
        "assets must be uploaded before the nightly tag moves"
    );
}

#[test]
fn a_first_nightly_is_created_as_a_draft() {
    let workflow = nightly_workflow();
    let release = release_job(&workflow);
    let create = release
        .find("gh release create")
        .expect("the release job creates the first nightly");
    let invocation = command_at(release, create);
    assert!(
        invocation.contains("--draft"),
        "the first nightly must start as a draft so a failure cannot publish it"
    );
    assert!(
        !release.contains("--verify-tag"),
        "--verify-tag cannot work when the tag is created at publish time"
    );
}

#[test]
fn a_draft_nightly_is_published_after_the_assets_are_uploaded() {
    let workflow = nightly_workflow();
    let release = release_job(&workflow);
    let upload = release
        .find("gh release upload")
        .expect("the release job uploads assets");
    let publish = release
        .find("--draft=false")
        .expect("the release job publishes the draft nightly");
    assert!(
        upload < publish,
        "the draft must stay unpublished until its assets are attached"
    );
}

#[test]
fn the_nightly_date_is_read_once() {
    let workflow = nightly_workflow();
    let step = workflow
        .split_once("- name: Compute nightly version")
        .expect("nightly workflow computes the version")
        .1
        .split_once("\n      - name:")
        .expect("the version step is followed by another step")
        .0;
    assert_eq!(
        step.matches("date -u").count(),
        1,
        "one clock read: the version date and the display date must agree across midnight UTC"
    );
    assert!(step.contains("display_date="));
}
