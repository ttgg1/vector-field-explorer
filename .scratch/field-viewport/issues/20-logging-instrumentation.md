# 20: `log`-crate instrumentation (native + wasm)

**What:** adopt **rust-lang/log** as the logging facade and emit logs where they
help debugging. Use standard sinks only — `env_logger` on native (stderr,
`RUST_LOG`, default `info`) and `console_log` on wasm. No custom logger, no log
file, no rotation, no diagnostics UI.

**Status:** ready-for-agent (do first; branch `feature/logging`)

## Why now

There is no logging today: only two `eprintln!` calls in `src/ui/presets.rs`
(preset save/delete failures). A GUI launch hides stderr, wasm has no console
output, and user reports carry no detail. Tickets 15–17 (menus, wasm, release)
will be easier with `log` calls already in place.

## Design

- Use the `log` facade macros (`error!`, `warn!`, `info!`, `debug!`, `trace!`).
  Do **not** add `tracing` or a custom `Log` implementation.
- **Native:** initialize `env_logger` once in `main` before `run_native`, with
  `RUST_LOG` support and default filter `info`. Logs go to stderr.
- **Wasm:** initialize `console_log` at `info` when the wasm entry point lands
  (ticket 16); add the target-gated dependency now so the seam exists. Do not
  parse URL parameters in this ticket.
- **Levels:** `error` = user-visible failure; `warn` = degraded/recovered state;
  `info` = lifecycle (startup, backend, preset loaded, screenshot saved);
  `debug` = per-action detail; `trace` = hot loops only. No per-frame logging
  above `trace`.
- **No custom infrastructure:** no file writer, rotation, tee, panic hook, or
  diagnostics snapshot. The terminal/browser console and existing smoke logs are
  enough for v0.1.0.

## Work

1. Add target-gated dependencies: `log` (all), `env_logger` (native),
   `console_log` (wasm). Commit `Cargo.lock`.
2. Initialize `env_logger` in `src/main.rs`:
   `Builder::from_env(Env::default().default_filter_or("info")).init()`.
3. Replace both `eprintln!` calls in `src/ui/presets.rs` with `warn!`, including
   the error value.
4. Instrument only where it makes sense:
   - startup/version/native `[info]` — once.
   - field parse/eval failure `[debug]` — once per change, not per frame.
   - persistence fallback or migration `[warn]`.
   - preset load/save/delete failure `[warn]`; success `[debug]`.
   - screenshot failure `[warn]`; saved path `[info]`.
   - scene rebuild / particle / click-seed counts `[trace]` only.
5. Add one `AGENTS.md` note: native `RUST_LOG`, wasm console, default `info`.
6. Do not add a log module unless a shared helper is genuinely needed; imports
   from `log` at the call sites are preferred.

## Verification (no new unit tests)

This ticket adds call-site instrumentation, not behavior or custom parsing, so
there is no meaningful pure unit to write. Verify by observation:

- `cargo run` → info startup line on stderr.
- `RUST_LOG=debug cargo run` → debug detail appears.
- `RUST_LOG=error cargo run` → info/debug suppressed.
- `scripts/smoke-run.sh 15` → app stays up, no per-frame info/debug spam.
- `cargo build --all-targets`, `cargo test --all-targets`, clippy
  `--all-targets -- -D warnings` remain green.
- If `wasm32-unknown-unknown` is installed, `cargo check --target` passes;
  otherwise ticket 16 verifies browser console output.

If a helper with real logic is added despite the guidance above, it must get a
failing test first.

## Acceptance

- [ ] `log` macros are used at the listed points; production `eprintln!`/
      `println!` are gone.
- [ ] Default native run logs at `info`; `RUST_LOG` changes the filter.
- [ ] No per-frame info/debug output during the 15 s smoke run.
- [ ] Full gates and smoke pass.
- [ ] No custom logger, file output, rotation, panic hook, or diagnostics UI.
- [ ] `AGENTS.md` documents `RUST_LOG` and the wasm console.

## Non-goals

Log files, rotation, structured/JSON logs, telemetry, crash reporting, log
viewer, diagnostics export, or per-frame performance instrumentation.

## Dispatch

- Implementer: `opencode-go/deepseek-v4.1-flash#high`, background.
- Reviewer: `opencode-go/claude-haiku-5-5#high`, background.
- Brief = this ticket; ledger the dependency ruling.
