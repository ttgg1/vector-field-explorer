// Ticket 34: `scripts/clean-test-artifacts.sh` keeps `target/debug/deps` from
// growing one stale test executable per `cargo test` run. The tests drive the
// real script through `bash` with `TARGET_DIR` pointed at a temp tree and
// assert on exit status and filesystem effects only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const OLD_HASHED: &str = "mycrate-0123456789abcdef";
const NEW_HASHED: &str = "mycrate-fedcba9876543210";
const NESTED_HASHED: &str = "other-0011223344556677";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write_file(path: &Path, executable: bool, modified: SystemTime) {
    fs::write(path, b"fake artifact").unwrap();
    let file = fs::File::options().write(true).open(path).unwrap();
    file.set_modified(modified).unwrap();
    if executable {
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

/// `TARGET_DIR/debug/deps` with two hashed executables sharing a crate prefix
/// and deliberately different mtimes, plus every kind of neighbour the cleanup
/// must not touch.
struct Fixture {
    tmp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let deps = tmp.path().join("debug/deps");
        fs::create_dir_all(&deps).unwrap();
        let now = SystemTime::now();
        let older = now - Duration::from_secs(3600);

        write_file(&deps.join(OLD_HASHED), true, older);
        write_file(&deps.join(NEW_HASHED), true, now);
        // Cargo's non-executable outputs and an executable whose name is not a
        // hash-suffixed test binary.
        write_file(&deps.join("libmycrate-0123456789abcdef.rlib"), false, older);
        write_file(&deps.join("mycrate-0123456789abcdef.d"), false, older);
        write_file(&deps.join("mycrate-0123456789abcdef.rmeta"), false, older);
        write_file(&deps.join("helper-script"), true, older);
        // A nested executable must never be considered: only depth 1 counts.
        let nested = deps.join("nested");
        fs::create_dir_all(&nested).unwrap();
        write_file(&nested.join(NESTED_HASHED), true, older);

        Self { tmp }
    }

    fn target(&self) -> PathBuf {
        self.tmp.path().to_path_buf()
    }

    fn deps(&self) -> PathBuf {
        self.target().join("debug/deps")
    }

    fn path(&self, name: &str) -> PathBuf {
        self.deps().join(name)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new("bash")
            .arg(repo_root().join("scripts/clean-test-artifacts.sh"))
            .args(args)
            .env("TARGET_DIR", self.target())
            .current_dir(repo_root())
            .output()
            .expect("run scripts/clean-test-artifacts.sh")
    }
}

fn assert_removed(path: &Path) {
    assert!(!path.exists(), "expected {} to be removed", path.display());
}

fn assert_kept(path: &Path) {
    assert!(path.exists(), "expected {} to be kept", path.display());
}

#[test]
fn default_keeps_newest_hashed_executable_per_crate() {
    let fx = Fixture::new();
    let out = fx.run(&[]);
    assert!(
        out.status.success(),
        "cleanup failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_removed(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
    // Everything that is not a depth-1 hashed test executable stays.
    assert_kept(&fx.path("libmycrate-0123456789abcdef.rlib"));
    assert_kept(&fx.path("mycrate-0123456789abcdef.d"));
    assert_kept(&fx.path("mycrate-0123456789abcdef.rmeta"));
    assert_kept(&fx.path("helper-script"));
    assert_kept(&fx.deps().join("nested").join(NESTED_HASHED));
    // Default mode never touches incremental build state.
    assert_kept(&fx.deps().join("nested"));
}

#[test]
fn dry_run_removes_nothing() {
    let fx = Fixture::new();
    let out = fx.run(&["--dry-run"]);
    assert!(
        out.status.success(),
        "dry run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_kept(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
}

#[test]
fn all_removes_every_hashed_executable() {
    let fx = Fixture::new();
    let out = fx.run(&["--all"]);
    assert!(
        out.status.success(),
        "cleanup --all failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_removed(&fx.path(OLD_HASHED));
    assert_removed(&fx.path(NEW_HASHED));
    // `--all` still means "test executables only".
    assert_kept(&fx.path("libmycrate-0123456789abcdef.rlib"));
    assert_kept(&fx.path("mycrate-0123456789abcdef.d"));
    assert_kept(&fx.path("helper-script"));
    assert_kept(&fx.deps().join("nested").join(NESTED_HASHED));
}

#[test]
fn incremental_removes_incremental_directory() {
    let fx = Fixture::new();
    let incremental = fx.target().join("debug/incremental");
    fs::create_dir_all(incremental.join("vector_field_explorer-abc")).unwrap();
    fs::write(incremental.join("vector_field_explorer-abc/session"), b"x").unwrap();

    let out = fx.run(&["--incremental"]);
    assert!(
        out.status.success(),
        "cleanup --incremental failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_removed(&incremental);
    // Incremental pruning does not skip the stale-binary pruning.
    assert_removed(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
}

#[test]
fn missing_target_directory_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new("bash")
        .arg(repo_root().join("scripts/clean-test-artifacts.sh"))
        .env("TARGET_DIR", tmp.path().join("does-not-exist"))
        .current_dir(repo_root())
        .output()
        .expect("run scripts/clean-test-artifacts.sh");

    assert!(
        out.status.success(),
        "missing target directory must not fail: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
