//! Third-party notice bundle guard (ticket 19).
//!
//! `packaging/THIRD_PARTY_NOTICES.txt` is generated from the Cargo graph, so it
//! must be regenerated whenever `Cargo.lock` changes. The generator's `--check`
//! mode regenerates into a temporary file and compares it with the committed
//! bundle; this test runs that check instead of duplicating the comparison.

use std::path::Path;
use std::process::Command;

#[test]
fn committed_notice_bundle_matches_the_cargo_graph() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("scripts/generate-notices.sh");
    assert!(
        script.is_file(),
        "missing notice generator: {}",
        script.display(),
    );

    let output = Command::new("bash")
        .arg(&script)
        .arg("--check")
        .current_dir(root)
        .output()
        .expect("run scripts/generate-notices.sh --check");

    assert!(
        output.status.success(),
        "the committed notice bundle is stale or the generator failed; \
         run scripts/generate-notices.sh and commit the result\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let bundle = root.join("packaging/THIRD_PARTY_NOTICES.txt");
    let text = std::fs::read_to_string(&bundle)
        .unwrap_or_else(|e| panic!("read {}: {e}", bundle.display()));
    for section in ["## Packages", "## Licence texts"] {
        assert!(
            text.contains(section),
            "{} is missing the {section} section",
            bundle.display(),
        );
    }
    assert!(
        text.contains("### eframe "),
        "the generated bundle must cover the Cargo dependency graph",
    );
    assert!(
        text.contains("Elected licence: Apache-2.0"),
        "dual/triple-licensed packages without a crate licence file must elect Apache-2.0",
    );
    assert!(
        text.contains("Canonical Apache-2.0"),
        "the bundle must contain the canonical Apache-2.0 text section",
    );
    assert!(
        text.contains("Elected licence: MIT"),
        "MIT-only packages must cite a checked-in upstream text",
    );
    assert_eq!(
        text.matches("NO LICENCE FILE:").count(),
        1,
        "only a licence that needs no binary notice (BSL-1.0) may stay textless",
    );
    for machine_path in ["/home/", ".cargo/registry", "index.crates.io"] {
        assert!(
            !text.contains(machine_path),
            "the generated bundle must not embed machine-specific paths ({machine_path})",
        );
    }
}
