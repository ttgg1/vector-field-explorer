# Test layout and profile

Technical reference; [Workflow](../../../references/workflow.md) owns current
gate commands, toolchain, concurrency limits and verification waivers.

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
  `use super::*` do not resolve, and tests see only `pub` items. Consult
  [engineering notes](../../../references/rust.md) before adding a test seam.
- Filter suite modules/names (`cargo test particles_test::`); only the isolated
  config target uses `cargo test --test config_test`.
- `scripts/test.sh` runs the full suite and prunes stale test binaries;
  `KEEP_TEST_ARTIFACTS=1` skips cleanup. Keep the all-targets runner.

## Build cost

`Cargo.toml` sets `[profile.test] opt-level = 2` on top of the dev profile's
`debug = 1` / `strip = "debuginfo"` (`.cargo/config.toml`). Ticket 21 cut the
warm suite from ~3.98 s to ~0.72 s median. Accepted cost: the cold
`cargo test --all-targets --no-run` build is 65.0 s versus 26.5 s at
opt-level 0 (2.45×), paid once per dependency/profile change; the incremental
edit loop improved from 5.6 s to ~1.3 s.

Historical source: ticket 21 benchmark/accepted profile tradeoff in
`.superpowers/sdd/2026-11-05-vector-field-explorer/archive/progress-through-2026-11-11.md`.
