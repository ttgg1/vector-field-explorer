# Test layout and gates — as practised from 2026-11-10

## The claim

Tests live in `tests/`, one file per module, registered in `tests/suite.rs`.
`src/` carries no `#[cfg(test)]` modules. Three commands gate every change.

## Test layout

- Every test lives in `tests/<module>_test.rs`; `tests/suite.rs` includes each
  one as a module (`autotests = false` plus one main `[[test]]`), so
  `cargo test --all-targets` runs the suite in a single process.
- A test that mutates process-global environment must be its own `[[test]]`
  target so it runs alone — `tests/config_test.rs` (which sets
  `XDG_CONFIG_HOME`) is the example. Only `config_test` has its own `--test`
  target; the other files are suite modules.
- Shared helpers live in `tests/support/` and are declared once in
  `tests/suite.rs`.
- `tests/test_layout_test.rs` fails the suite if a `#[cfg(test)]` module
  reappears in `src/`, if a test file is unregistered, or if a test is
  `#[ignore]`d.
- Integration tests import `vector_field_explorer::{…}` — `crate::` and
  `use super::*` do not resolve, and they see only `pub` items. A helper a test
  genuinely needs is promoted to a single `#[doc(hidden)] pub` seam with the
  reason in the commit body and the ledger; the API is never widened further.

## Gates

```
cargo build
CARGO_BUILD_JOBS=4 scripts/test.sh     # runs cargo test --all-targets and prunes stale test binaries
cargo clippy --all-targets -- -D warnings
```

`scripts/test.sh` honours `KEEP_TEST_ARTIFACTS=1` to skip cleanup. Clippy
needs the `--` separator.

## Build cost

`Cargo.toml` sets `[profile.test] opt-level = 2` on top of the dev profile's
`debug = 1` / `strip = "debuginfo"` (`.cargo/config.toml`). Ticket 21 cut the
warm suite from ~3.98 s to ~0.72 s median. Accepted cost: the cold
`cargo test --all-targets --no-run` build is 65.0 s versus 26.5 s at
opt-level 0 (2.45×), paid once per dependency/profile change; the incremental
edit loop improved from 5.6 s to ~1.3 s.

## Runtime smoke

Build and test create no wgpu device. After touching `src/renderer.rs`,
shaders, windowing or input, run `scripts/smoke-run.sh 15`; it fails on
panics and wgpu validation errors. CI runs it headlessly. A UI-only change
that touches neither renderer nor windowing does not need it — the ticket 69
and 71 implementations on 2026-11-10 skipped it for that reason and were
not faulted for it.

## Related

- [Dispatch, review and fix loop](dispatch-and-review-loop.md)
