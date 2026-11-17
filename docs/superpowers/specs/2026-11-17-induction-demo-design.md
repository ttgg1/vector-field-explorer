# Induction teaching demo — detailed design

**Issue:** #27 `[Physics / #17] Induction teaching demo — changing B, analytic/CAS derivatives and shared clock`
**Status:** PROPOSED — detailed design awaiting owner approval
**Date:** 2026-11-17
**Dependencies:** #24 source evaluator, #16 shared-clock animation, #12 scalar plotting, #13 plane slicing, #25/#26 source panel/glyphs/export provenance — all merged. #15 CLI work remains separate.
**Owner decisions already made:** implementation after this design is approved, delivered through sub-issues under #27 using subagent-driven development. Physical scope: one editable current loop **and** one finite solenoid, with the rigorous softened vector-potential reconstruction for `E`.

## 1. Intent

Build a teaching-quality Faraday induction demo. A single axisymmetric conductor —
a circular current loop or a finite continuous solenoid — carries a sinusoidal
current. The viewer shows the changing magnetic field `B` and the induced
Faraday electric field `E` side by side or separately, never summed, on the
existing shared simulation clock.

The app must state its physical assumptions instead of pretending that
differentiating an arbitrary `B(x,t)` determines `E`. Under the selected
assumptions the induced field is reconstructed from the softened vector
potential, and the result is checked against independent Faraday-loop and
vector-potential references.

## 2. Goals and non-goals

### Goals

- Editable loop/solenoid geometry, current amplitude and frequency.
- One global `SimClock`: play, pause, reset-time, step and speed remain the only
  time source. No private timer.
- `B` and `E` are separate physical outputs with separate colours and provenance.
- `E` is reconstructed analytically/axisymmetrically from the softened vector
  potential, not from an inverse-Faraday solve and not from a finite difference
  in time.
- Native CAS differentiates the waveform; the web build uses the same analytic
  derivative without requiring a wasm CAS port.
- Reuse the existing source evaluator for `B`, the existing source-glyph model,
  the existing scene/export/provenance seams, and the existing clock/transport.
- Invalid, non-finite or over-budget results are never drawn or exported as
  valid vectors.
- Persist the demo configuration; preserve Formula and Sources modes unchanged.
- Document the assumptions, model distortion and numerical limitations.

### Non-goals

- No displacement current, radiation, retardation, full Maxwell evolution,
  mutual inductance, coupled loops or general inverse-Faraday solver.
- No CAS solve/integrate/history; CAS only differentiates the selected waveform.
- No per-source softening, private animation timer, new render backend or CLI
  extension.
- No new geometry kind beyond the current loop and finite solenoid already in the
  source evaluator.
- No formula-to-source conversion and no replacement of the formula/source modes.
- No particle or streamline advection in Induction mode in this delivery (see
  §7); the shared clock still governs the field, and existing formula particle
  state is not modified by switching modes.
- No change to #13 plane selection/seeding semantics or #15 CLI behavior.

## 3. Approach chosen

Three approaches were considered:

1. **Generated Formula-mode presets.** Cheap, but cannot express a dual E/B field,
   family selection, accuracy provenance, or an induction-specific panel, and
   would force users to switch formulas manually. Rejected.
2. **Time-varying source kinds inside Sources mode.** Reuses the source list but
   mixes static electrostatics/magnetostatics with a dynamic induction concept,
   makes all source rows potentially time-dependent, and leaves no place for the
   E reconstruction and CAS waveform contract. Rejected.
3. **Dedicated `FieldMode::Induction`.** A concept-specific adapter beside Formula
   and Sources. It owns one axisymmetric induction scene, samples E and B as
   separate families, reuses the source evaluator for B and the source glyph for
   geometry, and keeps the static source scene untouched. **Chosen.**

## 4. Physical model and assumptions

### 4.1 Symmetry and gauge

- One axisymmetric conductor aligned with a normalized axis.
- Quasistatic magnetoquasistatic limit: `∇×E = -∂B/∂t`, no displacement current,
  no radiation/retardation, no free charge (Coulomb gauge, no electrostatic
  scalar contribution).
- The source is a circular current loop or a finite continuous solenoid.
- The field is evaluated in the existing softened teaching model with the one
  scene-wide Plummer softening `a` already stored in `SourceScene.softening`.
- The normalized unit convention matches the source evaluator: the magnetic
  constant is absorbed so the existing `B` and the vector potential share units.
  Display units remain illustrative and SI-like only.

These assumptions make the reconstruction unique: the azimuthal vector potential
`A_phi` follows from the current distribution, and `E = -∂A/∂t` under the
axisymmetric Coulomb-gauge choice.

### 4.2 Current waveform

```
I(t) = amplitude * cos(2π * frequency * t)
```

- `amplitude` is a signed current in the normalized units of the source model.
- `frequency` is in cycles per simulated second, `>= 0`.
- A zero amplitude or zero frequency gives a static field and `dI/dt = 0`, hence
  exactly zero induced `E`.
- No phase control in this delivery; the initial state is `B` near maximum and
  `E` at zero, which makes the first play frame visibly change.

### 4.3 Magnetic field

`B(t)` is the existing softened magnetostatic field of the selected source with
current `I(t)`. The existing source evaluator is linear in current, so the app
prepares the unit-current `B` once per geometry and scales its value and
uncertainty by `I(t)`.

The existing family result contract remains authoritative for `B`: `TargetMet`,
`AccuracyNotMet` or `Unavailable`, with its diagnostics and target.

### 4.4 Induced field from the vector potential

For a loop of radius `R` at center `c` with local axis `z`, radial distance `r`
and axial offset `z`:

```
A_φ(r,z) = I * R * ∫₀²π cos φ / sqrt(r² + R² - 2rR cos φ + z² + a²) dφ
```

For a finite continuous solenoid of radius `R`, length `L`, `N` turns:

```
A_φ(r,z) = (N I / L) * R *
    ∫₀²π cos φ *
    [asinh((z + L/2)/D) - asinh((z - L/2)/D)] dφ

D = sqrt(r² + R² - 2rR cos φ + a²)
```

The loop is a line current; the solenoid is the same current distribution spread
as a continuous surface current with `N I / L` per unit length. The z integration
of the softened loop potential over a uniform axial current sheet is analytic and
yields the two `asinh` terms; only the azimuthal integral remains. The
implementation must verify `curl A = B_softened` numerically for both cases and
several softenings; matching a separate closed-form B alone is not sufficient.
`a` enters `D²` for both the line and surface distribution, so it regularizes the
softened B exactly as it does in the source evaluator. This formula was checked
numerically against the existing loop/solenoid magnetic evaluator before this
design was written; the plan still carries an independent fixture gate.

`E` is then:

```
E_phi = -dI/dt * A_phi(unit current)
E = E_phi * φ_hat
```

At `r = 0`, `φ_hat` is undefined and axisymmetry gives `E = 0`; the same exact
zero applies to a purely static field. `E` is finite under the softening model,
including on/near the conductor, though the softened model deliberately removes
the physical singular limit.

### 4.5 Accuracy and provenance

- `B` uses the source evaluator's existing family target, uncertainty and
  diagnostics.
- `E` uses a fixed-budget, deterministic azimuthal quadrature: eight equal
  panels on `[0, π]`, each evaluated with 32-point Gauss–Legendre nodes, then
  doubled (256 kernel samples per point). A coarse four-panel estimate (128
  samples) is also computed; the absolute uncertainty is
  `max(|fine - coarse|, 32 * f64::EPSILON * |fine|) * |dI/dt|`. No recursion,
  allocation per point, or time-varying work count.
- `E` target: `epsilon * e_scale + epsilon * |E|`, with the same
  `AccuracyRequest::epsilon` and `e_scale` as the source evaluator. `TargetMet`
  requires `uncertainty <= target`; otherwise the bounded `AccuracyNotMet`
  (roundoff) or `Unavailable` (numeric range) classification is used. The
  quadrature is smooth and periodic; there is no geometric split. A point whose
  integrand produces a non-finite value is `Unavailable(NumericRange)`, never a
  confident value.
- Each family reports `TargetMet`, `AccuracyNotMet(reason)` or `Unavailable`.
- The UI and exports report the two families separately. They are never added,
  averaged or presented as one vector field.
- Accuracy-not-met results are hidden by default, drawn only with a persisted
  default-off opt-in, and always visibly labelled **“accuracy not met”** in the
  viewport/export where applicable.
- The softened-model disclosure is not a numerical accuracy claim. It always
  says “quasistatic axisymmetric softened model” and shows `a`.
- The E/B uncertainty does not change the cached unit-current geometry; time-only
  changes rescale both value and uncertainty by `|I|`/`|dI/dt|` and reclassify
  against the current target.

## 5. Architecture

### 5.1 New pure model — `src/induction.rs`

```rust
pub enum InductionCase { CurrentLoop, Solenoid }

pub struct InductionScene {
    pub case: InductionCase,
    pub center: DVec3,
    pub axis: DVec3,          // normalized
    pub radius: f64,
    pub length: f64,          // solenoid only; 0 for loop
    pub turns: u32,           // solenoid only; 1 for loop
    pub amplitude: f64,
    pub frequency: f64,
    pub display: SourceDisplay, // Electric = E, Magnetic = B, Both
    pub allow_inaccurate: bool,
}

pub enum InductionFamilyResult {
    TargetMet(InductionEstimate),
    AccuracyNotMet(InductionEstimate, StopReason),
    Unavailable(StopReason),
}

pub struct InductionEstimate {
    pub value: DVec3,
    pub uncertainty: f64,
    pub target: f64,
}

pub struct InductionFamilyStatus {
    pub drawn: usize,
    pub target_met: usize,
    pub accuracy_not_met_total: usize,
    pub accuracy_not_met_drawn: usize,
    pub unavailable: usize,
}

pub struct InductionSampling {
    pub arrows: Vec<ArrowData>,
    pub electric: InductionFamilyStatus,
    pub magnetic: InductionFamilyStatus,
    pub current: f64,
    pub derivative: f64,
    pub derivative_source: Option<String>, // native CAS string; UI diagnostics only
    pub softening: f64,
    pub validation_error: Option<String>,
}

pub struct InductionPointSample {
    pub electric: InductionFamilyResult,
    pub magnetic: FamilyResult,
}

/// Immutable unit-current geometry and per-point kernel values.
pub struct InductionPrepared {
    pub key: InductionKey,
    points: Vec<InductionUnitPoint>,
}

pub struct InductionKey {
    pub case: InductionCase,
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
    pub length: f64,
    pub turns: u32,
    pub domain: (f32, f32, f32, f32, f32, f32),
    pub grid: (u32, u32, u32),
    pub plane: Option<(PlaneAxis, u32)>,
    pub softening: f64,
    pub accuracy: AccuracyRequest,
}

struct InductionUnitPoint {
    position: Vec3,       // local for a 2D slice, world in 3D
    world: DVec3,
    unit_e: DVec3,        // -A_phi times the unit azimuthal basis, zero on axis
    e_uncertainty: f64,
    b_value: DVec3,
    b_uncertainty: f64,
    b_stop: Option<StopReason>, // None = target met
}
```

`InductionScene` owns validation: finite center/axis, normalized nonzero axis,
`radius` and solenoid `length` in the documented finite ranges, `turns >= 1`
for solenoid, finite signed amplitude and finite non-negative frequency. Invalid
scenes are not sampled; the panel/status bar gets an actionable message and the
viewport shows no induction arrows.

Derived API:

```rust
impl InductionScene {
    pub fn current(&self, t: f32) -> f64;
    pub fn current_derivative(&self, t: f32) -> f64;
    pub fn is_animated(&self) -> bool;
    pub fn validate(&self) -> Result<(), String>;
    pub fn unit_source(&self) -> Source; // current 1.0, existing kind
}

pub fn induction_key(
    scene: &InductionScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
    softening: f64,
    accuracy: AccuracyRequest,
) -> InductionKey;

pub fn prepare_induction(
    scene: &InductionScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
    softening: f64,
    accuracy: AccuracyRequest,
) -> Result<InductionPrepared, String>;

pub fn sample_induction(
    prepared: &InductionPrepared,
    scene: &InductionScene,
    t: f32,
    view: &ViewOptions,
) -> InductionSampling;

pub fn evaluate_induction_point(
    scene: &InductionScene,
    point: DVec3,
    t: f32,
    softening: f64,
) -> InductionPointSample;
```

`prepare_induction` owns geometry/domain/grid/plane/softening/accuracy changes
and evaluates unit-current B and `A_phi` per point. `sample_induction` owns time
changes: it computes `I(t)`, `dI/dt`, rescales cached unit values and
uncertainties, reclassifies each point against the current E/B targets, filters
by display/opt-in, colours, and returns arrows. `State` stores the prepared
cache and its key; only a changed key calls `prepare_induction`. A per-point API
remains for physics tests and is not the frame path.

`InductionPrepared` stores `Vec3` positions (local on a 2D slice, world in 3D),
the world query point, the unit-current `E` vector (including the azimuthal
basis and exact zero on axis), its unit uncertainty, the existing evaluator's
unit-current `B` result, and its unit uncertainty. At 14,400 arrows it is a
small bounded vector of fixed-size records; the cache is replaced atomically on
a key change and dropped on leaving Induction/global reset.

### 5.2 Viewmodel and app

- Add `FieldMode::Induction` in `src/source_scene.rs`.
- Add `State::induction: InductionScene`,
  `State::induction_prepared: Option<Arc<InductionPrepared>>`, and
  `State::induction_sampling: Option<InductionSampling>`.
- `active_dimension()` returns `3` for Induction.
- `has_time_dependent_consumer()` returns `induction.is_animated()` in Induction;
  Formula and Sources behavior is unchanged.
- `particle_target()` returns `None` in Induction (arrows-only demo); existing
  formula particle state is preserved and paused by mode isolation.
- `set_field_mode` preserves Formula, Sources and Induction scenes; clears
  `induction_prepared`/`induction_sampling` and notices; clamps density to the
  induction caps; marks dirty.
- `snapshot_if_dirty` keys the prepared cache on exactly the fields in
  `InductionKey`. A changed key calls `prepare_induction`; a time-only change
  calls `sample_induction` without re-running per-point physics.
- Entering Induction with a persisted valid scene prepares on the first dirty
  rebuild. An invalid edit keeps the previous valid `InductionScene` and its
  cache, reporting the edit error; leaving Induction drops the prepared cache
  but preserves the durable scene, so returning re-prepares at the current time.
- `snapshot` in Induction returns the scaled arrows and empty lines/trails/dots.
  It appends the source glyph of the selected conductor through
  `source_glyph_lines` in the app composition path, exactly as Sources does.
- The app's `rebuild_scene` handles Induction like a vector scene: backdrop,
  source glyph, arrows; no streamlines, particles or click seeding.
- `sim_clock::has_clock_consumer` follows `State::has_time_dependent_consumer`;
  transport visibility follows it. No second clock.
- `set_time` marks dirty only when the induction waveform is animated; paused
  frames hold time.
- Switching to a scalar plot resumes the existing vector-consumer pause rule;
  switching back reuses the same clock time. No separate timer or time mirror.

### 5.3 CAS and web parity

`src/induction.rs` builds the canonical waveform source:

```
<amplitude>*cos(<2*pi*frequency>*t)
```

- Native: `cas::differentiate(source, Variable::T)` produces the waveform
  derivative; the result is parsed/evaluated through meval with `t` bound. The
  derivative string is retained as provenance/diagnostics.
- Wasm: the same derivative is evaluated by the fixed analytic form
  `-amplitude*2π*frequency*sin(2π*frequency*t)`. No CAS is required or ported.
- Native CAS result and analytic reference agree at sampled times with absolute
  tolerance `1e-12 * max(1, |analytic|)`; the test covers zero, positive and
  negative amplitude, zero frequency, and several times.
- If CAS differentiation fails, the native path falls back to the analytic
  derivative and reports a non-fatal diagnostic; the field is not silently
  invalid. A validation error in the amplitude/frequency itself still invalidates
  the scene.
- `derivative_source` (the CAS string) surfaces in the induction panel's
  Advanced diagnostics and never enters export metadata, because it is
  platform-dependent and not part of the physical result. Export metadata names
  the canonical analytic waveform/derivative instead.

### 5.4 Display semantics

`InductionScene.display` reuses `SourceDisplay`:

- `Electric`: E only, magnitude colormap.
- `Magnetic`: B only, magnitude colormap.
- `Both`: E amber `[255,176,64]`, B cyan `[64,196,255]`, separate arrow sets,
  never summed, never a shared colormap range.

E/B status badges and available counts are shown in the induction panel and
status bar. `Both` disables nothing; it is simply two families of arrows.

### 5.5 Sampling caps and performance

Initial engineering caps (subject to benchmark evidence before finalizing the
plan):

- `MAX_INDUCTION_DENSITY_3D = 20` (8,000 arrows)
- `MAX_INDUCTION_DENSITY_2D = 120` (14,400 arrows)
- one source only; no source list in Induction mode
- azimuthal quadrature bounded per point; no unbounded recursion or allocation
- geometry cache keyed by case/geometry/domain/grid/plane/display/softening/
  accuracy; time-only updates are scalar rescaling, not resampling.

The caps are provisional in this design and become frozen plan values only after
the Task 1 evaluator benchmark. Task 1 must measure both cases at both caps on
the recorded host and record the wall time; the plan may lower either cap before
Task 2 rather than hiding cost. Task 5 reruns the same benchmark through the
integrated app/export path. The 33 ms particle-performance context is not claimed.

## 6. UI

- Add an `Induction` choice to the vector-mode adapter selector in the formula
  bar, beside `Formula` and `Sources`. The formula editor is hidden in Induction
  mode and replaced by a compact summary + "Open induction" action.
- New `src/ui/induction_panel.rs`, rendered as a dedicated left rail/sheet in
  Induction mode (wide and compact). It owns:
  - case selector: Current loop / Finite solenoid;
  - geometry: radius (and length/turns for solenoid);
  - amplitude and frequency;
  - display: Electric / Magnetic / Both;
  - Advanced: softening display/edit (the shared source-scene softening),
    default-off accuracy opt-in, E/B status, assumptions note;
  - reset section.
- Editing softening here is an ordinary shared-state edit: it updates
  `SourceScene.softening`, clears both source and induction caches, marks the
  scene dirty, and affects Sources mode too. That one global value is deliberate.
- The panel shows the native CAS derivative string as diagnostic provenance when
  available; the field and export never depend on that string.
- The panel uses the shared `Controls` patterns where possible but keeps
  induction-specific fields out of the flat sidebar. Existing Display/Camera/
  Coloring/Domain sections remain available.
- Streamlines, particles, click seeding and the CAS panel are disabled in
  Induction with a short explanation; their durable values are preserved.
- Loading a classic preset returns to Formula and preserves the induction scene
  untouched, matching the Sources precedent.
- Global reset clears the induction scene, cache, notices and restores defaults.
- Help/README/GLOSSARY/CODEBASE explain:
  - the axisymmetric quasistatic assumptions;
  - why arbitrary `B` alone does not determine `E`;
  - the softened vector-potential reconstruction;
  - static-B/zero-frequency = zero induced E;
  - native CAS vs web analytic parity;
  - arrows-only scope and shared clock.

## 7. Clock, mode transitions, and particles

- Induction is a clock consumer only while `is_animated()`: exactly
  `amplitude != 0.0 && frequency > 0.0`. A signed amplitude with nonzero
  frequency animates; amplitude zero or frequency zero is static and hides the
  transport (unless another consumer exists). Play/pause/step/reset and speed are
  the existing `SimClock`.
- Pausing freezes the arrow field at the mirrored time; stepping advances exactly
  one clock step; reset-time returns the field to `t = 0`.
- Entering/leaving Induction clears only formula-owned transient geometry through
  the existing `sync_field_mode`. The source scene and induction scene are
  durable.
- Existing formula particles are not advanced while Induction is active, and
  their cloud/trails are preserved for the return. Induction does not spawn a
  cloud. This is the explicit particle scope for this delivery; the panel says
  so.
- Scalar-plot switching keeps the shared time; returning to Induction samples at
  the current time.
- Plane motion in 2D uses the existing `PlaneFrame`; the induction sampler
  evaluates world E/B and projects it onto the selected plane, matching the source
  mode. No #13 seeding/transition logic is duplicated.

## 8. Persistence and reset

`ViewRecord` gains:

```rust
pub field_mode: FieldMode,          // already exists; add Induction variant
pub induction_case: String,
pub induction_center: [f64; 3],
pub induction_axis: [f64; 3],
pub induction_radius: f64,
pub induction_length: f64,
pub induction_turns: u32,
pub induction_amplitude: f64,
pub induction_frequency: f64,
pub induction_display: SourceDisplay,
pub induction_allow_inaccurate: bool,
```

All new fields are serde-defaulted so old payloads restore as Formula with a
default valid induction scene and no notice. The default scene is:

```
case = CurrentLoop
center = (0, 0, 0), axis = (0, 0, 1)
radius = 1.0, length = 0.0 (solenoid default 2.0), turns = 1 (solenoid default 8)
amplitude = 1.0, frequency = 0.25 (period 4 s)
display = Both, allow_inaccurate = false
```

Invalid/out-of-range persisted induction fields fall back to this default and a
notice; they never make startup fail. “Empty” in this design means an unsampled
default scene, not an invalid zero-radius scene. The shared `source_softening`
field remains the one softening value. Global reset returns the induction scene
to this default, drops its cache, and clears notices. Camera state remains
unpersisted.

## 9. Export and warnings

Extend the existing `ExportProvenance` path so Induction requests capture a
warning-bearing provenance at request time, including when sampling is missing or
invalid.

Always-present model line:

```
Induction - quasistatic axisymmetric softened model - a = {softening}
```

Additional lines when relevant:

```
{display} - accuracy not met: {hidden} hidden, {drawn} drawn
Unavailable: {count}
Induction scene invalid: {error}
Induction sampling unavailable
```

Metadata keys:

```
vfe.model = quasistatic-axisymmetric-faraday
vfe.induction_case = current_loop | solenoid
vfe.waveform = amplitude*cos(2*pi*frequency*t)
vfe.waveform_derivative = -amplitude*2*pi*frequency*sin(2*pi*frequency*t)
vfe.amplitude
vfe.frequency
vfe.softening
vfe.display
vfe.accuracy_not_met_total
vfe.accuracy_not_met_drawn
vfe.unavailable
vfe.validation_error (when present)
```

PNG uses the existing baked banner + tEXt path; SVG uses the existing visible
text + metadata path. `allow_inaccurate` changes what is drawn, never whether the
warning exists or whether caps apply. Nonfinite values are never exported. This
is the same policy #26 approved for Sources; no new export backend.

## 10. Error, finite, and unavailable behavior

- Non-finite input/field values: scene validation error, no arrows, visible
  status/export warning.
- Quadrature work/roundoff limit: `Unavailable` or `AccuracyNotMet` with the
  existing stop reason; never a confident field.
- On/near the conductor with softening: finite softened values, with the model
  disclosure; no claim of the physical singular limit.
- Invalid edit: retain the previous valid induction scene and show the error;
  valid edits atomically replace it and clear the cache.
- Switching modes never corrupts Formula/Sources state or the shared clock.

## 11. Testing and evidence

### Physics (pure CPU)

- Loop and solenoid unit-current vector-potential fixtures from an independent
  high-precision generator: a committed JSON/CSV fixture file plus a reproducible
  Python `mpmath` generator and README, following the #24 source fixture pattern.
  No runtime or Rust dev-dependency is added. Rows cover off-axis, interior,
  exterior, near-conductor and on-axis points for both cases.
- `curl A` agrees with the existing source evaluator's `B` within the documented
  target over a grid, for both cases and several softenings.
- Faraday closed-contour check is a hard gate: `∮E·dl = -d/dt∫B·dA` for
  representative circles in both source types, with a 64-point contour and a
  tolerance of `max(1e-3 * |flux_rate|, 20 * combined_uncertainty)`. The fixture
  values use a pinned high-precision reference; the contour check is not
  informational.
- Sign/current reversal, amplitude scaling, frequency scaling, phase relation
  (`E` leads `B` by the expected quarter cycle), and exactly zero `E` for static
  current.
- Near-support/cancellation and over-budget behavior: finite or explicit
  `Unavailable`/`AccuracyNotMet`; no NaN/Inf vectors.
- E/B `TargetMet`, `AccuracyNotMet` and `Unavailable` classification and the
  default-off opt-in gate.

### CAS/web

- Native `cas::differentiate` of the canonical waveform matches the analytic
  derivative at sampled times and through the meval bridge.
- Wasm/analytic path yields the same supported teaching case and compiles without
  CAS.
- No new wasm test requiring CAS.

### Clock/app/UI

- `has_time_dependent_consumer` true only for an animated induction scene.
- Play advances arrows; pause freezes; step advances one coherent step; reset
  returns to `t=0`; speed and loop wrap follow the existing clock.
- Live geometry/amplitude/frequency edits rebuild or rescale correctly; invalid
  edits keep the last valid scene.
- Mode switch preserves Formula/Sources/Induction state; scalar-plot switch
  pauses and returns at the same time.
- E/B selection and `Both` fixed colours; no unlike sum.
- Actual egui panel controls at wide and compact sizes; rail/sheet reachable;
  reset/action paths; help text.
- Persistence round-trip/legacy/malformed; global reset.
- PNG decoded banner/tEXt and SVG text/metadata provenance; opt-in cannot remove
  warnings; nonfinite never exported.

### Performance and gates

- Benchmark loop and solenoid at both caps; record wall time and diagnostics.
- Full suite, build, clippy `-D warnings`, changed-file rustfmt, docs check,
  `git diff --check`, native smoke. Wasm build/check. Browser/manual visual gates
  remain owner-waived, not passed.
- Independent review of actual diffs; scoped fix loop; five-round ceiling.

## 12. Risks and review focus

- **Vector-potential/B consistency:** wrong normalization or missing softening
  would make the Faraday check pass superficially but disagree with the rendered
  B. Tests must compare against the actual source evaluator, not only a separate
  formula.
- **Axis/sign conventions:** E must follow the right-hand current convention and
  reverse with current direction; contour orientation tests must be explicit.
- **Cache invalidation:** time-only updates must rescale, geometry/domain/plane
  changes must resample, and stale arrows must never survive a mode/geometry
  change.
- **E/B presentation:** no summation, no shared magnitude range in `Both`, no
  “accuracy not met” label removed by opt-in.
- **CAS/web parity:** native CAS output must be equal to the analytic derivative,
  while wasm must not acquire a CAS dependency or silently diverge.
- **Scope discipline:** no general inverse-Faraday solver, no new source kinds,
  no particle/streamline advection in Induction, no CLI extension, no duplicate
  #13 plane logic.

## 13. Delivery shape

After owner approval, write the implementation plan and create linked sub-issues
under #27. Proposed tasks:

1. Induction scene/model and softened vector-potential E/B evaluator, with
   independent physics fixtures and a Task 1 cap benchmark that freezes the
   provisional caps before Task 2.
2. Shared-clock integration, E/B sampling/statuses, caching/limits.
3. Induction UI/rail/compact panel, mode selector, help/status.
4. Persistence/migration/reset and export provenance.
5. Docs, full gates and benchmark.

SDD execution: fresh implementer per task, independent review of the actual diff,
scoped fixes, whole-branch review, PR, preserving merge, post-merge gates, close
#27 and sub-issues.
