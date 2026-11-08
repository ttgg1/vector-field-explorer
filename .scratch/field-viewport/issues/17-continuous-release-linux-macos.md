# 17: Continuous GitHub release for Linux and macOS

**What to build:** a tag-driven GitHub Actions release pipeline that builds and
publishes Linux and macOS binaries, so a `v*` tag produces a downloadable
release without manual packaging.

**Status:** ready-for-agent

## Trigger

- Push of a tag matching `v*` (e.g. `v0.1.0`).
- `workflow_dispatch` with a tag input for dry runs.

## Jobs

1. **verify** (ubuntu-latest): checkout, Rust stable, cache; run
   `cargo build --all-targets`, `cargo test --all-targets`,
   `cargo clippy --all-targets -- -D warnings`. The release jobs depend on it.
2. **build** (matrix):
   - `ubuntu-latest` → `x86_64-unknown-linux-gnu`
   - `macos-13` → `x86_64-apple-darwin`
   - `macos-14` → `aarch64-apple-darwin`
   - Linux deps: `libxkbcommon-dev libwayland-dev libx11-dev libxi-dev
     libgl1-mesa-dev libvulkan-dev pkg-config`.
   - `cargo build --release --locked`; strip the binary.
   - Package:
     - Linux: `vector-field-explorer-<version>-x86_64-unknown-linux-gnu.tar.gz`
       containing the binary, `README.md`, `LICENSE`.
     - macOS: `vector-field-explorer-<version>-<target>.zip` containing a
       minimal `.app` bundle (`Contents/MacOS/vector-field-explorer`,
       `Contents/Info.plist` with bundle name/version) plus `LICENSE` and
       `README.md`.
   - Upload artifacts to the workflow run.
3. **release** (ubuntu-latest, `contents: write`): download artifacts, create or
   update the GitHub Release for the tag (`gh release create` or
   `softprops/action-gh-release`), generate notes from commits, attach all
   assets.

## Guards

- Fail if `Cargo.toml` `version` does not match the tag without the leading
  `v` (small `scripts/check-version.sh` or inline shell).
- Run only for tags reachable from `main`.
- No signing/notarization in v1: document that macOS users may need
  right-click → Open (Gatekeeper).
- Cache cargo builds; keep macOS minutes in mind on the private repo (10×
  billing).

## Acceptance

- [ ] Pushing `v0.1.0` publishes a release with three assets (Linux tar.gz,
      macOS x86_64 zip, macOS arm64 zip).
- [ ] Each archive contains the binary/app, LICENSE, and README.
- [ ] Version/tag mismatch fails the workflow.
- [ ] Linux binary launches on a clean-ish Ubuntu with the documented deps;
      macOS binaries launch on both architectures (manual check).
- [ ] `gh release view v0.1.0` shows the assets; workflow is green.
- [ ] Commit `ci: tag-driven Linux/macOS release workflow`.

## Non-goals

Windows, AppImage/deb/rpm packaging, code signing, notarization, Homebrew tap,
auto-update.

## Scheduling

Implement before ticket 16 (Pages download links) and before ticket 09's final
review. Do **not** push the first real tag until ticket 09's whole-branch
review has merged and `main` is green; test the workflow with a pre-release tag
or `workflow_dispatch` first.
