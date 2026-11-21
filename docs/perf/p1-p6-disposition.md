# P1–P6 performance disposition (2026-11-21)

Task (c) of #18. Evidence: the maintained CPU/GPU harness and its baseline
`docs/perf/baselines/2026-11-21-rx9070xt.txt`, the present-path floor
verification in `docs/perf/baselines/2026-11-21-present-rx9070xt.txt`, and the
object-renderer measurements in `docs/perf/object-renderer.md`. This record
dispositions the historical recommendations; it promotes no code by itself and
changes no floor.

## Baseline facts used

- Native floors PASS at 1280×800 on the RX 9070 XT: default-2D p50 5.553 ms
  (180.1 fps) and densest-3D p50 5.570 ms (179.5 fps). Both p50s sit at the
  179.92 Hz refresh cap, which bounds the measurement and cannot distinguish
  "fast enough" from "faster than the display"; no GPU headroom is claimed. The
  floors are met on this discrete host; the integrated-GPU reference floor
  remains **unverified** and this record does not evidence it.
- CPU snapshot p50 is 523 µs default-2D, 5.4 ms densest-3D, 9.3 ms dipole-3D and
  87 ms for the density-40 grid stress. `snapshot()` includes sampling, so these
  are "sample + geometry" costs.
- Scalar heatmap view assembly is 12.1 ms p50; a harmonic-density continuity
  report is 193 ms; the field-probe report is 465 µs. These are on-demand paths.
- Object rendering adds ≈0.055 ms (solid style; ≈0.071 ms wireframe) of GPU time for 64 objects / 35,712 triangles,
  and uploads are already generation-gated.

## Disposition

### P1 — bound/cache field evaluations: PROMOTE (first)

Densest-3D and dipole-3D CPU snapshots are dominated by repeated formula
evaluation, and an interactive drag re-samples the same points across frames.
Promote a bounded evaluation cache keyed on formula, parameter set, domain and
time quantisation, with a documented memory cap and an honest miss path.
Acceptance: measured improvement on the existing harness scenes with unchanged
values, plus a regression test that the cache cannot return a stale result after
a parameter, formula, domain or time change.

### P2 — buffer reuse and dirty uploads: PARTIALLY DELIVERED, measure before more

Object instances are already uploaded only when the scene generation changes
(#112), and the object pass reuses one colour+depth target. Remaining buffers
(arrows, lines, trails) are rebuilt with each snapshot, but the present-path
result shows no visible frame-rate pressure. Promote only if P1's measurements
show a geometry-build cost that reuse would remove; do not undertake a broad
buffer refactor on speculation.

### P3 — flow-field bake and GPU advection: DEFER post-v1

This changes the simulation model, the accuracy story and the browser compute
strategy, and the owner has ruled that it needs an ADR and a browser/compute
plan. Defer until after v1; no partial bake without that record.

### P4 — trail sort reduction: PROMOTE (small; recommended after P1)

The worst-particle scene shows 5.6 ms CPU p50 with 4,000 particles and 32 trail
segments. A cheaper back-to-front ordering removes measurable cost at the densest
trail setting without changing visuals. Acceptance: identical instance order
where it matters visually, measured improvement on `worst-particles`, and a test
pinning the ordering contract.

### P5 — browser performance harness: NOT AUTHORIZED

The owner tests the web build manually and files issues as needed; no browser
harness is authorized. The web floor stays unverified, not passed, and this
record does not claim otherwise.

### P6 — integrated-GPU validation: UNVERIFIABLE HERE

This host exposes no integrated GPU. The integrated floor stays unverified; do
not cite the discrete-GPU result as evidence for it. Revisit if hardware becomes
available.

## Order and guardrails

P1 then P4 (recommended order), each under its own claim, TDD and independent review; P2 only if P1
shows it is worth doing; P3, P5 and P6 remain deferred with the reasons above.
No task in this group may lower a documented floor, add a CI performance
threshold, or claim visual verification, which remains owner-waived.

## Reproduction

```sh
scripts/bench-perf-baseline.sh --gpu          # CPU/GPU baseline (task a)
scripts/bench-present-floor.sh                # present-path floors (task b)
```

Both write bounded `PERF` lines into `docs/perf/baselines/` and keep raw detail
in the local log.
