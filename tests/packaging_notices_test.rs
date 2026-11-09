//! AppImage system-library notice logic (ticket 19).
//!
//! The packaging workflow calls `scripts/package-notices.sh`; this test drives
//! it against a temporary AppDir/sysroot with a fake `dpkg` on `PATH`, so the
//! owner resolution, common-licence copying, and post-extraction validation
//! run as real shell code instead of being asserted from the workflow text.
//!
//! The AppImage is Linux-only, so these tests are compiled only on Linux.

#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const SCRIPT: &str = "scripts/package-notices.sh";

fn script(root: &Path) -> PathBuf {
    root.join(SCRIPT)
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod");
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir {}: {e}", parent.display()));
    }
    fs::write(path, contents).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

/// Build an AppDir with one bundled library and a fake `dpkg` that prints
/// `output` for every `dpkg -S` call.
fn install_fixture(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
    let appdir = temp.path().join("AppDir");
    write(
        &appdir.join("usr/lib/x86_64-linux-gnu/libfoo.so.1"),
        "not a real library",
    );
    let sysroot = temp.path().join("sysroot");
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).expect("fake bin");
    write_executable(
        &bin.join("dpkg"),
        "#!/bin/sh\nif [ -n \"${DPKG_OUTPUT:-}\" ]; then\n  printf '%s\\n' \"$DPKG_OUTPUT\"\nelse\n  exit 1\nfi\n",
    );
    (appdir, sysroot, bin)
}

fn package_notices(
    root: &Path,
    bin: &Path,
    args: &[&str],
    dpkg_output: &str,
) -> Output {
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new("bash")
        .arg(script(root))
        .args(args)
        .env("PATH", path)
        .env("DPKG_OUTPUT", dpkg_output)
        .output()
        .expect("run package-notices.sh")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn sysroot_copyright(sysroot: &Path, package: &str, text: &str) {
    write(
        &sysroot.join(format!("usr/share/doc/{package}/copyright")),
        text,
    );
}

#[test]
fn runtime_owner_is_preferred_over_dev_and_dbg() {
    let temp = TempDir::new().expect("tempdir");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (appdir, sysroot, bin) = install_fixture(&temp);
    for package in ["libfoo1", "libfoo-dev", "libfoo-dbg"] {
        sysroot_copyright(
            &sysroot,
            package,
            "Licence: GPL-3\nSee /usr/share/common-licenses/GPL-3.\n",
        );
    }
    write(&sysroot.join("usr/share/common-licenses/GPL-3"), "GPL three\n");

    let output = package_notices(
        root,
        &bin,
        &[
            "install",
            appdir.to_str().expect("utf-8 appdir"),
            "--sysroot",
            sysroot.to_str().expect("utf-8 sysroot"),
        ],
        "libfoo1:amd64: /usr/lib/x86_64-linux-gnu/libfoo.so.1\nlibfoo-dev:amd64: /usr/lib/x86_64-linux-gnu/libfoo.so.1\nlibfoo-dbg:amd64: /usr/lib/x86_64-linux-gnu/libfoo.so.1",
    );

    assert!(
        output.status.success(),
        "install failed\nstdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output),
    );
    let notice_dir = appdir.join("usr/share/doc/vector-field-explorer/system-libraries");
    let sources = fs::read_to_string(notice_dir.join("SOURCES.txt")).expect("SOURCES.txt");
    assert!(
        sources.contains("libfoo.so.1\tlibfoo1"),
        "runtime owner must be recorded, got:\n{sources}"
    );
    assert!(notice_dir.join("libfoo1.copyright").is_file());
    assert!(!notice_dir.join("libfoo-dev.copyright").exists());
    assert!(!notice_dir.join("libfoo-dbg.copyright").exists());
    assert!(notice_dir.join("common-licenses/GPL-3").is_file());
}

#[test]
fn two_runtime_owners_fail_with_the_library_path() {
    let temp = TempDir::new().expect("tempdir");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (appdir, sysroot, bin) = install_fixture(&temp);
    let output = package_notices(
        root,
        &bin,
        &[
            "install",
            appdir.to_str().expect("utf-8 appdir"),
            "--sysroot",
            sysroot.to_str().expect("utf-8 sysroot"),
        ],
        "libfoo1:amd64: /usr/lib/x86_64-linux-gnu/libfoo.so.1\nlibfoo2:amd64: /usr/lib/x86_64-linux-gnu/libfoo.so.1",
    );

    assert!(!output.status.success(), "ambiguous ownership must fail");
    let error = stderr(&output);
    assert!(error.contains("ambiguous"), "stderr:\n{error}");
    assert!(error.contains("libfoo.so.1"), "stderr:\n{error}");
    assert!(error.contains("libfoo1"), "stderr:\n{error}");
    assert!(error.contains("libfoo2"), "stderr:\n{error}");
}

#[test]
fn a_library_with_no_package_owner_fails() {
    let temp = TempDir::new().expect("tempdir");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (appdir, sysroot, bin) = install_fixture(&temp);
    let output = package_notices(
        root,
        &bin,
        &[
            "install",
            appdir.to_str().expect("utf-8 appdir"),
            "--sysroot",
            sysroot.to_str().expect("utf-8 sysroot"),
        ],
        "",
    );

    assert!(!output.status.success(), "a missing owner must fail");
    let error = stderr(&output);
    assert!(error.contains("no dpkg package owns"), "stderr:\n{error}");
    assert!(error.contains("libfoo.so.1"), "stderr:\n{error}");
}

#[test]
fn a_missing_common_licence_file_fails() {
    let temp = TempDir::new().expect("tempdir");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (appdir, sysroot, bin) = install_fixture(&temp);
    sysroot_copyright(
        &sysroot,
        "libfoo1",
        "Licence: GPL-3\nSee /usr/share/common-licenses/GPL-3.\n",
    );

    let output = package_notices(
        root,
        &bin,
        &[
            "install",
            appdir.to_str().expect("utf-8 appdir"),
            "--sysroot",
            sysroot.to_str().expect("utf-8 sysroot"),
        ],
        "libfoo1:amd64: /usr/lib/x86_64-linux-gnu/libfoo.so.1",
    );

    assert!(!output.status.success(), "a missing common licence must fail");
    let error = stderr(&output);
    assert!(error.contains("references missing"), "stderr:\n{error}");
    assert!(error.contains("GPL-3"), "stderr:\n{error}");
}

#[test]
fn a_path_traversal_common_licence_reference_fails() {
    let temp = TempDir::new().expect("tempdir");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (appdir, sysroot, bin) = install_fixture(&temp);
    sysroot_copyright(
        &sysroot,
        "libfoo1",
        "Licence: GPL-3\nSee /usr/share/common-licenses/../GPL-3.\n",
    );
    write(&sysroot.join("usr/share/GPL-3"), "outside the directory\n");

    let output = package_notices(
        root,
        &bin,
        &[
            "install",
            appdir.to_str().expect("utf-8 appdir"),
            "--sysroot",
            sysroot.to_str().expect("utf-8 sysroot"),
        ],
        "libfoo1:amd64: /usr/lib/x86_64-linux-gnu/libfoo.so.1",
    );

    assert!(!output.status.success(), "a '..' reference must fail");
    let error = stderr(&output);
    assert!(
        error.contains("unsafe") || error.contains(".."),
        "stderr:\n{error}"
    );
}

#[test]
fn validation_fails_when_a_shipped_notice_is_missing() {
    let temp = TempDir::new().expect("tempdir");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let extracted = temp.path().join("squashfs-root");
    let notice_dir = extracted.join("usr/share/doc/vector-field-explorer/system-libraries");
    write(
        &notice_dir.join("libfoo1.copyright"),
        "Licence: GPL-3\nSee /usr/share/common-licenses/GPL-3.\n",
    );
    write(&notice_dir.join("SOURCES.txt"), "libfoo.so.1\tlibfoo1\n");
    fs::create_dir_all(notice_dir.join("common-licenses")).expect("common-licenses dir");

    let output = package_notices(
        root,
        temp.path(),
        &["validate", extracted.to_str().expect("utf-8 root")],
        "",
    );

    assert!(!output.status.success(), "a missing shipped notice must fail");
    let error = stderr(&output);
    assert!(error.contains("missing"), "stderr:\n{error}");
    assert!(error.contains("GPL-3"), "stderr:\n{error}");
}

#[test]
fn validation_passes_when_every_reference_resolves() {
    let temp = TempDir::new().expect("tempdir");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let extracted = temp.path().join("squashfs-root");
    let notice_dir = extracted.join("usr/share/doc/vector-field-explorer/system-libraries");
    write(
        &notice_dir.join("libfoo1.copyright"),
        "Licence: GPL-3\nSee /usr/share/common-licenses/GPL-3.\n",
    );
    write(&notice_dir.join("SOURCES.txt"), "libfoo.so.1\tlibfoo1\n");
    write(
        &notice_dir.join("common-licenses/GPL-3"),
        "GPL three\n",
    );

    let output = package_notices(
        root,
        temp.path(),
        &["validate", extracted.to_str().expect("utf-8 root")],
        "",
    );

    assert!(
        output.status.success(),
        "validate failed\nstdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output),
    );
}
