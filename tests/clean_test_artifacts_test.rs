// Ticket 34: `scripts/clean-test-artifacts.sh` keeps `target/debug/deps` from
// growing one stale test executable per `cargo test` run. The tests drive the
// real scripts through `bash` with the target directory pointed at a temp tree
// and assert on exit status and filesystem effects; the only wording assertion
// is the `would remove:` listing, which is the contract of `--dry-run`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const OLD_HASHED: &str = "mycrate-0123456789abcdef";
const NEW_HASHED: &str = "mycrate-fedcba9876543210";
const OTHER_CRATE_HASHED: &str = "othercrate-0011223344556677";
const NON_EXEC_HASHED: &str = "mycrate-aaaabbbbccccdddd";
const NESTED_HASHED: &str = "other-0011223344556677";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn clean_script() -> PathBuf {
    repo_root().join("scripts/clean-test-artifacts.sh")
}

fn test_script() -> PathBuf {
    repo_root().join("scripts/test.sh")
}

fn write_file(path: &Path, executable: bool, modified: SystemTime) {
    fs::write(path, b"fake artifact").unwrap();
    let file = fs::File::options().write(true).open(path).unwrap();
    file.set_modified(modified).unwrap();
    #[cfg(unix)]
    if executable {
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
    #[cfg(not(unix))]
    let _ = executable;
}

/// A command for one of the repo scripts with every cleanup-related env var
/// cleared, so the tests never inherit a caller's configuration.
fn script_command(script: &Path) -> Command {
    let mut cmd = Command::new("bash");
    cmd.arg(script)
        .env_remove("TARGET_DIR")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("KEEP_TEST_ARTIFACTS")
        .env_remove("CLEAN_INCREMENTAL")
        .current_dir(repo_root());
    cmd
}

/// `TARGET_DIR/debug/deps` with two hashed executables sharing a crate prefix
/// and deliberately different mtimes, a third crate, and every kind of
/// neighbour the cleanup must not touch: non-executable hashed file, `.rlib`,
/// `.d`, `.rmeta`, a non-hash executable, and a nested executable.
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
        write_file(&deps.join(OTHER_CRATE_HASHED), true, older);
        // The executable-bit gate: a hashed name without the executable bit is
        // not a test binary and must survive even `--all`.
        write_file(&deps.join(NON_EXEC_HASHED), false, older);
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
        script_command(&clean_script())
            .env("TARGET_DIR", self.target())
            .args(args)
            .output()
            .expect("run scripts/clean-test-artifacts.sh")
    }

    /// A keep file listing absolute paths, as `scripts/test.sh` produces it.
    fn write_keep_file(&self, names: &[&str]) -> PathBuf {
        let keep = self.target().join("keep.txt");
        let body: String = names
            .iter()
            .map(|name| format!("{}\n", self.path(name).display()))
            .collect();
        fs::write(&keep, body).unwrap();
        keep
    }
}

fn assert_removed(path: &Path) {
    assert!(!path.exists(), "expected {} to be removed", path.display());
}

fn assert_kept(path: &Path) {
    assert!(path.exists(), "expected {} to be kept", path.display());
}

fn assert_success(out: &Output, what: &str) {
    assert!(
        out.status.success(),
        "{what} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A stand-in for `cargo` that reports the given test executables for the
/// `--no-run` probe and exits with `status` for the real test run.
fn fake_cargo(dir: &Path, status: i32, executables: &[&Path]) -> PathBuf {
    let path = dir.join("fake-cargo");
    let artifacts: String = executables
        .iter()
        .map(|exe| {
            format!(
                "printf '%s\\n' '{{\"reason\":\"compiler-artifact\",\"executable\":\"{exe}\"}}'\n",
                exe = exe.display()
            )
        })
        .collect();
    fs::write(
        &path,
        format!(
            "#!/usr/bin/env bash\n\
             case \" $* \" in\n\
               *--no-run*)\n\
             {artifacts}\
                 exit 0\n\
                 ;;\n\
             esac\n\
             exit {status}\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }
    path
}

#[test]
fn default_keeps_newest_hashed_executable_per_crate() {
    let fx = Fixture::new();
    assert_success(&fx.run(&[]), "cleanup");

    assert_removed(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
    // A single executable for another crate is current, not stale.
    assert_kept(&fx.path(OTHER_CRATE_HASHED));
    // Everything that is not a depth-1 executable hashed test binary stays.
    assert_kept(&fx.path(NON_EXEC_HASHED));
    assert_kept(&fx.path("libmycrate-0123456789abcdef.rlib"));
    assert_kept(&fx.path("mycrate-0123456789abcdef.d"));
    assert_kept(&fx.path("mycrate-0123456789abcdef.rmeta"));
    assert_kept(&fx.path("helper-script"));
    assert_kept(&fx.deps().join("nested").join(NESTED_HASHED));
    // Default mode never touches incremental build state.
    assert_kept(&fx.deps().join("nested"));
}

#[test]
fn dry_run_lists_removals_and_removes_nothing() {
    let fx = Fixture::new();
    let out = fx.run(&["--dry-run"]);
    assert_success(&out, "dry run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("would remove:"),
        "dry run must list what it would delete, got: {stdout}"
    );
    assert!(
        stdout.contains(OLD_HASHED),
        "dry run must name the stale binary, got: {stdout}"
    );
    assert!(
        !stdout.contains(NEW_HASHED),
        "dry run must not list the newest binary, got: {stdout}"
    );

    assert_kept(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
    assert_kept(&fx.path(OTHER_CRATE_HASHED));
}

#[test]
fn all_removes_every_hashed_executable() {
    let fx = Fixture::new();
    assert_success(&fx.run(&["--all"]), "cleanup --all");

    assert_removed(&fx.path(OLD_HASHED));
    assert_removed(&fx.path(NEW_HASHED));
    assert_removed(&fx.path(OTHER_CRATE_HASHED));
    // `--all` still means "executable test binaries only".
    assert_kept(&fx.path(NON_EXEC_HASHED));
    assert_kept(&fx.path("libmycrate-0123456789abcdef.rlib"));
    assert_kept(&fx.path("mycrate-0123456789abcdef.d"));
    assert_kept(&fx.path("helper-script"));
    assert_kept(&fx.deps().join("nested").join(NESTED_HASHED));
}

#[test]
fn hashed_file_without_the_executable_bit_is_never_removed() {
    let fx = Fixture::new();
    // `--all` is the most aggressive mode, so it is the strongest check that
    // the executable bit gates every removal.
    assert_success(&fx.run(&["--all"]), "cleanup --all");

    assert_kept(&fx.path(NON_EXEC_HASHED));
}

#[test]
fn incremental_removes_incremental_directory() {
    let fx = Fixture::new();
    let incremental = fx.target().join("debug/incremental");
    fs::create_dir_all(incremental.join("vector_field_explorer-abc")).unwrap();
    fs::write(incremental.join("vector_field_explorer-abc/session"), b"x").unwrap();

    assert_success(&fx.run(&["--incremental"]), "cleanup --incremental");

    assert_removed(&incremental);
    // Incremental pruning does not skip the stale-binary pruning.
    assert_removed(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
}

#[test]
fn missing_target_directory_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let out = script_command(&clean_script())
        .env("TARGET_DIR", tmp.path().join("does-not-exist"))
        .output()
        .expect("run scripts/clean-test-artifacts.sh");

    assert!(
        out.status.success(),
        "missing target directory must not fail: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cargo_target_dir_is_used_when_target_dir_is_unset() {
    let fx = Fixture::new();
    let out = script_command(&clean_script())
        .env("CARGO_TARGET_DIR", fx.target())
        .output()
        .expect("run scripts/clean-test-artifacts.sh");
    assert_success(&out, "cleanup with CARGO_TARGET_DIR");

    assert_removed(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
}

#[test]
fn target_dir_wins_over_cargo_target_dir() {
    let fx = Fixture::new();
    let other = tempfile::tempdir().unwrap();
    fs::create_dir_all(other.path().join("debug/deps")).unwrap();

    let out = script_command(&clean_script())
        .env("TARGET_DIR", fx.target())
        .env("CARGO_TARGET_DIR", other.path())
        .output()
        .expect("run scripts/clean-test-artifacts.sh");
    assert_success(&out, "cleanup with both target vars");

    assert_removed(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
}

#[test]
fn keep_file_keeps_exactly_the_listed_binaries() {
    let fx = Fixture::new();
    // The lib and bin unit-test binaries of one crate, as cargo reports them.
    let keep = fx.write_keep_file(&[OLD_HASHED, NEW_HASHED]);

    assert_success(&fx.run(&["--keep-file", keep.to_str().unwrap()]), "cleanup --keep-file");

    // Both current binaries survive, even though they share a crate prefix.
    assert_kept(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
    // Anything not in the set is stale, whatever its mtime or crate.
    assert_removed(&fx.path(OTHER_CRATE_HASHED));
    // Non-executable neighbours are still untouched.
    assert_kept(&fx.path(NON_EXEC_HASHED));
    assert_kept(&fx.path("libmycrate-0123456789abcdef.rlib"));
    assert_kept(&fx.path("helper-script"));
}

#[test]
fn keep_file_honours_dry_run() {
    let fx = Fixture::new();
    let keep = fx.write_keep_file(&[NEW_HASHED]);

    let out = fx.run(&["--keep-file", keep.to_str().unwrap(), "--dry-run"]);
    assert_success(&out, "cleanup --keep-file --dry-run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("would remove:"), "got: {stdout}");
    assert_kept(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(OTHER_CRATE_HASHED));
}

#[test]
fn strip_debug_dry_run_touches_nothing() {
    let fx = Fixture::new();
    let before = fs::read(fx.path(NEW_HASHED)).unwrap();

    assert_success(&fx.run(&["--strip-debug", "--dry-run"]), "cleanup --strip-debug --dry-run");

    assert_kept(&fx.path(NEW_HASHED));
    assert_eq!(
        fs::read(fx.path(NEW_HASHED)).unwrap(),
        before,
        "dry run must not strip or rewrite kept binaries"
    );
}

#[test]
fn test_script_cleans_with_the_cargo_keep_set_and_preserves_status() {
    let fx = Fixture::new();
    let fake = fake_cargo(&fx.target(), 0, &[&fx.path(NEW_HASHED)]);

    let out = script_command(&test_script())
        .env("TARGET_DIR", fx.target())
        .env("CARGO", &fake)
        // Never let a real cargo start a build from inside the test suite; the
        // fake ignores the argument.
        .arg("--definitely-not-a-cargo-flag")
        .output()
        .expect("run scripts/test.sh");

    assert_success(&out, "scripts/test.sh");
    // The keep set is authoritative: only the executable cargo reported for
    // this run survives.
    assert_kept(&fx.path(NEW_HASHED));
    assert_removed(&fx.path(OLD_HASHED));
    assert_removed(&fx.path(OTHER_CRATE_HASHED));
    assert_kept(&fx.path("libmycrate-0123456789abcdef.rlib"));
}

#[test]
fn test_script_preserves_a_failing_test_status_and_still_cleans() {
    let fx = Fixture::new();
    let fake = fake_cargo(&fx.target(), 1, &[&fx.path(NEW_HASHED)]);

    let out = script_command(&test_script())
        .env("TARGET_DIR", fx.target())
        .env("CARGO", &fake)
        .arg("--definitely-not-a-cargo-flag")
        .output()
        .expect("run scripts/test.sh");

    assert_eq!(
        out.status.code(),
        Some(1),
        "the failing test status must survive cleanup: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_removed(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
}

#[test]
fn test_script_keeps_the_app_binary_hardlinked_into_deps() {
    let fx = Fixture::new();
    // Cargo links `target/debug/<name>` and hardlinks it into deps under a
    // hashed name, so the app binary looks like a test executable there. It is
    // executable, like every cargo-produced binary: a 0644 fixture would be
    // skipped by the executable-bit gate before the keep set is consulted.
    let bin = fx.target().join("debug/vector-field-explorer");
    fs::create_dir_all(bin.parent().unwrap()).unwrap();
    write_file(&bin, true, SystemTime::now());
    let bin_deps_name = "vector_field_explorer-0011223344556677";
    fs::hard_link(&bin, fx.path(bin_deps_name)).unwrap();

    let fake = fake_cargo(
        &fx.target(),
        0,
        &[&bin, &fx.path(NEW_HASHED)],
    );

    let out = script_command(&test_script())
        .env("TARGET_DIR", fx.target())
        .env("CARGO", &fake)
        .arg("--definitely-not-a-cargo-flag")
        .output()
        .expect("run scripts/test.sh");

    assert_success(&out, "scripts/test.sh with a hardlinked bin");
    assert_kept(&fx.path(bin_deps_name));
    assert_kept(&fx.path(NEW_HASHED));
    assert_removed(&fx.path(OLD_HASHED));
}

#[test]
fn test_script_skips_cleanup_for_a_forwarded_layout_flag() {
    let fx = Fixture::new();
    let fake = fake_cargo(&fx.target(), 0, &[&fx.path(NEW_HASHED)]);

    // `--release` makes cargo write to target/release/deps; the keep set would
    // then describe a different tree than the one cleanup prunes.
    let out = script_command(&test_script())
        .env("TARGET_DIR", fx.target())
        .env("CARGO", &fake)
        .args(["--release", "--definitely-not-a-cargo-flag"])
        .output()
        .expect("run scripts/test.sh");

    assert_success(&out, "scripts/test.sh --release");
    // Fail safe: nothing in debug/deps is touched.
    assert_kept(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
    assert_kept(&fx.path(OTHER_CRATE_HASHED));
}

#[test]
fn test_script_skips_cleanup_when_cargo_wrote_outside_the_cleanup_target() {
    let fx = Fixture::new();
    // The probe says cargo built into `release/deps` while cleanup would prune
    // `debug/deps`: the mismatch must win over the keep set.
    let elsewhere = fx.target().join("release/deps").join(NEW_HASHED);
    let fake = fake_cargo(&fx.target(), 0, &[&elsewhere]);

    let out = script_command(&test_script())
        .env("TARGET_DIR", fx.target())
        .env("CARGO", &fake)
        .arg("--definitely-not-a-cargo-flag")
        .output()
        .expect("run scripts/test.sh");

    assert_success(&out, "scripts/test.sh with a mismatched target");
    assert_kept(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
    assert_kept(&fx.path(OTHER_CRATE_HASHED));
}

#[test]
fn test_script_keep_test_artifacts_skips_cleanup() {
    let fx = Fixture::new();
    let fake = fake_cargo(&fx.target(), 0, &[&fx.path(NEW_HASHED)]);

    let out = script_command(&test_script())
        .env("TARGET_DIR", fx.target())
        .env("CARGO", &fake)
        .env("KEEP_TEST_ARTIFACTS", "1")
        .arg("--definitely-not-a-cargo-flag")
        .output()
        .expect("run scripts/test.sh");

    assert_success(&out, "scripts/test.sh KEEP_TEST_ARTIFACTS=1");
    assert_kept(&fx.path(OLD_HASHED));
    assert_kept(&fx.path(OTHER_CRATE_HASHED));
    assert_kept(&fx.path(NEW_HASHED));
}
