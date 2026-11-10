# Review record — ticket 72: Windows x86_64 packaging

**Ticket:** 72
**Reviewed:** 2026-11-10
**Branch:** `feature/ticket-72`
**Merge:** fast-forward into `main` (this repository keeps a linear history)

## Commits

| Commit | Subject |
| --- | --- |
| `8387e36` | feat: Windows x86_64 packaging, CI gate, icon embedding (ticket 72) |
| `b518f5c` | fix: address ticket 72 review findings (ticket 72) |
| `886031a` | fix: gate the Unix-only workflow tests on Windows (ticket 72) |

## Rounds

| Round | Range | Verdict | Open at the end |
| --- | --- | --- | --- |
| 1 (whole task) | base..`8387e36` | Needs fixes | Critical: the Windows zip-validation heredoc was indented, so the Package step died with `IndentationError` before the upload; Critical: `tests/clean_test_artifacts_test.rs` is Unix-only but was registered unconditionally, so the new Windows CI job's test run would be red; Important: the download page had no Windows entry; six minors |
| 2 (fix 1) | `8387e36`..`b518f5c` | Approved | none |
| 3 (CI follow-up) | `b518f5c`..`886031a` | Approved (scoped) | none |

## Findings and how they closed

- **Critical — the validation heredoc.** The body is at column 0, so the
  extracted script exits 0 on a good zip and 1 on a bad one.
- **Critical — the Unix-only test module.** The module is gated with
  `#[cfg(unix)]`, and the Windows target check
  (`cargo check --all-targets --target x86_64-pc-windows-msvc`) passes.
- **Important — the download page.** It carries the Windows asset and the
  SmartScreen note.
- **Minors.** The packaging workflow header, `packaging/ARTWORK.md`,
  `docs/CODEBASE.md`, the README's DX12 wording and the `build.rs` trailing
  newline.
- **CI follow-up.** The first run of the new Windows job was red: eleven
  tests failed because `tests/nightly_version_test.rs` and
  `tests/notices_test.rs` shell out to POSIX scripts that cannot run on the
  Windows runner. Both modules are now gated with `#[cfg(unix)]`; the Linux
  CI job still runs them. A scoped review found no other ungated Unix-only
  module.

## Not verified here

The MSVC build, the `rc.exe` icon embedding and the Windows job at runtime
cannot be exercised on Linux. A `workflow_dispatch` dry run of
`release.yml` or `nightly.yml` is required before any real tag is cut.
