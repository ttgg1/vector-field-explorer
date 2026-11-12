# Engineering notes

Read for Rust/UI/rendering changes. [Workflow](workflow.md) owns execution and
gates; [CODEBASE](../docs/CODEBASE.md) owns the module map.

## Testable boundaries

- Keep GPU paint behind `SceneGPU`; put pure logic in testable seams.
- Test layout/imports/profile: [test reference](../docs/wiki/process/test-layout-and-gates.md).
  A genuinely needed test seam is one `#[doc(hidden)] pub` item, with its reason
  in the commit body and ledger ruling; widen no further.
- Exercise the production decision and user-visible path, not a duplicate
  predicate. [Historical examples](../docs/wiki/process/closing-review-findings.md).

## UI and input

- egui 0.37 uses `App::ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`,
  not `update(ctx, frame)`. Use `egui::Panel::top/bottom/left/right(id)` against
  `Ui`, and `egui::CentralPanel::default_margins().show(ui, …)`.
- Game movement uses `Event::Key.physical_key` (QWERTY positions), falling back
  to logical `key` where the backend omits it (eframe web today).
  `InputState::keys_down` is logical only; accumulate held movement from events
  and clear on focus loss.
- Customization belongs in the owning feature panel: primary controls in the
  section body; advanced parameters in nested `CollapsingHeader`s with tooltips
  and a section reset. Split oversized panels by concept into panels/columns
  and ledger the reason; avoid a global Advanced page/flat settings dump.
  Future Sources/Materials/Maxwell/Plot panels follow the same rule.

## Platform and state

- wgpu comes through eframe re-exports. `rfd = "0.15"` is deliberate and
  native-only. Gate filesystem/dialog code out of wasm; use eframe `WebRunner`
  and trunk for the browser build. Keep one codebase with a wasm platform seam.
- Persistence is one RON envelope under `eframe::APP_KEY`; camera resets on
  launch by design. Corrupt UI settings fall back/clamp; camera presets fail fast.
- Rust 2024 makes `std::env::set_var` / `remove_var` unsafe; isolate tests that
  mutate process-global environment.
- Use `log` macros at call sites. Native uses `env_logger` on stderr with
  `RUST_LOG` (default info); wasm uses `console_log`. Per-frame output is trace.
