# Static scalar plotting — approved design

**Status: APPROVED by owner on 2026-11-15 in conversation.**

Date: 2026-11-15. Owning requirement: [GitHub #12](https://github.com/ttgg1/vector-field-explorer/issues/12).
Inspected baseline: local main `fb7d290`.

Approval provenance: the owner said **“go ahead”** in response to the written
design approval request on 2026-11-15, after the full design and independent
builder check. This worktree copy preserves the accepted design below; the main
checkout's original proposed artifact is unchanged. “Propose”, “proposed”, and
“designer defaults” in the preserved body record the historical design process,
not outstanding approval gates: the owner approved the full written design,
including the five decisions listed at the end. Implementation still requires
the separate plan's approval and Workflow's dispatch/review/evidence gates.

**Historical proposal preface (superseded by the approval recorded above):**

The owner selected **lit mesh**, rather than the issue's permitted wireframe
fallback. That selects an approach, **not approval of this document**. Unless
explicitly attributed to the issue or owner below, concrete defaults, interaction
rules and implementation boundaries are designer proposals for approval. This is
a design, not an implementation plan or authorization to start production work.

## Intent and success

Let someone explore a static scalar function directly, without pretending that
one number is a two- or three-component vector field. Curves show `y = f(x)`;
surfaces show `z = f(x, y)`. Discoverable physics examples should make both useful
without requiring prior knowledge of the formula syntax.

Success means an editable curve and an opaque, lit, correctly occluded surface
compose with reference geometry, camera navigation, colormaps and PNG capture.
Invalid values leave honest gaps. Returning to vector mode recovers the user's
vector expression and controls, not a scalar masquerading as a vector. Old saved
settings and existing vector/CLI rendering continue to work.

### Goals

- Separate scalar parsing and bounded, pure sampling from GPU resources.
- Explicitly distinguish vector fields, curves and surfaces in state and UI.
- Preserve separate vector, curve and surface expressions across transitions.
- Supply all eight named examples in #12 through the ordinary edit/sample path.
- Preserve native and WebAssembly product scope, with existing platform seams.
- Make plot sizing, depth, screenshots and persistence testable contracts.

### Non-goals and anti-goals

- No scalar animation, `t`, transport additions, or work from follow-on #16.
- No CLI flags, scalar CLI export modes, new preset framework, dependency changes,
  contours, implicit surfaces, volumes, transparency or two-sided mesh option.
- No selected-plane vector sampling/seeding/advection changes from #13.
- No CAS scalar algebra/apply feature, global renderer rewrite, or change to the
  vector path's painter-order/opacity policy.
- Never clamp function heights to the domain Z range, bridge a known invalid
  sample, invent zeroes at holes, or export a vector-only image labelled a plot.
- No claim that finite sampling proves mathematical continuity everywhere.

## Authority, coordination and current evidence

[Workflow](../../../references/workflow.md) is the policy authority. The controller
acts as chief of staff: maintain scope/claims and evidence, secure design approval,
coordinate shared seams, arrange implementation and independent review, and
serialize integration rather than writing production code during that loop.

The current #12 comment explicitly claims just-in-time **design**, not product
implementation. Its body contains stale dependency text: #15 is closed and PR
#31 is merged (merge commit `b51b1b0868f41036525f6145b6c89468b27730f2`); #14 is
closed. #13 is open, unassigned, with no comments/explicit claims when inspected.
That is a point-in-time coordination fact, not permission to take its scope or
ignore future ownership. Recheck GitHub before implementation dispatch, especially
for `src/viewmodel.rs`, renderer/app seams and `tests/suite.rs`.

The ignored historical `.superpowers/sdd/2026-11-10-native-cas/progress.md` is
absent here. No historical ruling or completion evidence is inferred from it.
The untracked `.codegraph/` is user work and is not part of this change.

### Verified architectural constraints

Source was explored through CodeGraph before code-file reads:

- `FieldExpr` in `src/field.rs` deliberately accepts two/three components and
  supports vector time dependence. It must not acquire a scalar interpretation.
- `State::set_formula` preserves the last parsed vector field on an invalid
  edit. `mode_3d` controls projection; `State::is_3d` also depends on vector arity.
  Neither boolean alone can select a scalar scene or its references.
- `src/ui/formula_bar.rs` currently owns one text mirror and projection toggles.
  Plot drafts must not be accidentally overwritten by that vector-only mirror.
- `SceneGPU`, `DrawState::new/record`, and `submit_offscreen` in
  `src/renderer.rs` currently have **no depth attachment/state**. Vector 3D uses
  painter ordering. Turning on depth in a pipeline alone is not a solution.
- `frame_viewport` in `src/app/viewport.rs` installs resources with eframe's
  actual `target_format`. `src/window.rs` uses default renderer configuration;
  there is no application-configured depth target to borrow.
- Installed `egui-wgpu 0.37.0` `CallbackTrait` explicitly runs `prepare` before
  egui's render pass and permits encoding into its supplied command encoder.
  `paint` receives an already-open `RenderPass<'static>` and cannot replace its
  attachments or begin a nested depth pass. Egui establishes the callback
  viewport and clip/scissor context before calling it.
- `src/shaders.line.wgsl` expands segment instances into world-width,
  camera-facing ribbons. Its entry points can be reused with depth-enabled
  pipeline state; a second curve shader is unnecessary.
- `capture_size` already handles logical points, DPI and proportional size caps;
  screenshots already distinguish synchronous native and asynchronous wasm
  readback. `ViewRecord` is serde-defaulted inside one RON `APP_KEY` envelope.
- Existing orbit camera distance is bounded to 20 and its perspective far plane
  is 100. Reusing those limits unquestioningly would make "Fit plot" dishonest
  for finite heights outside the domain.

These are source/contract observations, not proof that the proposed GPU path has
run successfully. Device/runtime evidence remains an implementation gate.

## Alternatives and selected approach

1. **Lit mesh in an isolated plot pass — selected approach.** Add a small opaque
   mesh pipeline and a plot-owned color/depth target; composite its color into
   egui. This delivers the owner's selected result and isolates depth changes
   from existing vector opacity/order, at the cost of an extra target and blit.
2. **Wireframe using existing lines.** Smaller shading/mesh-rendering change and
   explicitly permitted by #12, but does not deliver the owner's lit-mesh choice.
   It still needs depth handling; it is not a zero-cost substitute. Do not silently
   fall back to it if mesh integration fails.
3. **Enable global eframe depth and migrate all scene draws.** Broadens pipeline
   compatibility, transparent vector behavior and screenshot changes. It is not
   needed for #12 and makes regression risk unnecessarily large. Drawing the
   mesh directly in the current callback without a depth attachment is infeasible.

## Formula and state model

### Pure scalar expression

Introduce `ScalarExpr`, using existing `meval`, with an explicit curve/surface
parse context and pure evaluation. The scalar source is the **right-hand side**,
not an assignment: the UI supplies `y =` or `z =` as a non-editable label.

- Curve variables: `x` only. Surface variables: `x`, `y` only.
- Constants and the existing meval built-in functions/constants are allowed.
- Reject unknown identifiers/functions and malformed input at parse/validation
  time with a useful error naming the unexpected symbol and allowed variables.
  Reject free `y` for curves and free `z`/`t` for both; they have no meaningful
  static independent-variable role here.
- Validate bindings without requiring one arbitrary test point to evaluate
  finitely: `sqrt(x)` is a valid expression with a partly invalid domain.
- Evaluate in f64, then accept a point only if coordinates and their f32
  representation are finite. Evaluation errors, NaN, infinity, or unrepresentable
  GPU coordinates produce absent samples, never replacement zeroes. Explain
  representability loss in diagnostics; it is not a hidden height clamp.

### Explicit kind and preserved drafts

Propose `PlotKind::{VectorField, Curve2D, Surface3D}`, default `VectorField`.
Keep the existing vector formula/parsed `FieldExpr` separate from curve source/
parsed `ScalarExpr` and surface source/parsed `ScalarExpr`. Each scalar context
has its own draft, parse error, last-valid parsed value and cached sample result.
Do not infer kind from comma count, vector arity, or camera projection.

Typing a valid scalar replaces only that context's accepted expression and
resamples. An invalid **edit within the active context** retains that context's
last valid plot and shows "Invalid expression — displaying last valid plot".
No accepted plot means empty geometry. Switching kind clears active geometry
before selecting/resampling the destination's expression; if that destination
draft is invalid, show its error and no plot, not another kind's cached geometry.
Returning to vector mode never renders stale mesh/curve resources.

Proposed initial scalar drafts are `0.5*x^2` and `exp(-(x^2+y^2))`. These are
designer defaults, not additional owner requirements. Selection of a built-in
example replaces only its corresponding scalar draft and invokes the same setter
and rebuild path as typing. No parallel example-specific renderer or selection
state that can disagree with edited text.

## Sampling contracts

Both samplers are pure, deterministic and independent of GPU, clock and camera.
Sampling changes on accepted expression, relevant domain, resolution or kind,
not on every orbit/pan frame. Colors may rebuild without reevaluating expressions.
All allocation/work caps apply at the seam, not just the slider.

### Curve

`sample_curve(expr, domain, samples) -> Vec<Option<Vec3>>` samples the inclusive
X interval and emits `(x, f(x), 0)`. Domain Y/Z do not clip its output. Proposed
default count: **513**, bounded to **2..=4097** for normal requests. A zero/one
request at the pure seam returns zero samples/one midpoint sample respectively;
larger requests cap at 4097 before allocation. Count means returned grid slots,
including `None`, not number of successfully evaluated points.

Only adjacent present samples may form segments. Never connect the ends of
separate finite runs. Skip zero-length/non-finite segment geometry. Additional
bounded midpoint probes may reject an edge, but never alter the returned slot
count. Keep edge-validity/segment construction as a separate pure operation.

Finite samples on opposite sides of a pole are possible even without a NaN at a
grid point. Proposed conservative edge policy: probe each candidate midpoint;
reject non-finite probes; recursively inspect edges whose endpoint/probe value
spread exceeds eight times `max(domain Y span, 1)` for at most six subdivision
levels, and omit an edge still unresolved at the limit. The original adjacent
edge is either accepted or omitted, not bridged with its bad probe. Bound total
probe work by that depth and the sample cap. This deliberately permits false
gaps for extremely steep functions and still cannot discover every discontinuity;
document it as a display heuristic, not a continuity theorem. Owner approval of
this proposal is important (see open decisions).

### Surface

`sample_surface(expr, domain, resolution) -> SurfaceMesh` samples a rectangular
inclusive XY grid, height `z = f(x,y)`, regardless of domain Z. Proposed UI default
**64×64**, maximum **96×96 vertices** (not cells): at most 9,216 grid slots and
18,050 triangles / 54,150 indices. Use checked arithmetic and clamp before
allocation. Each axis with zero samples yields an empty mesh; one sample lies at
its midpoint and produces no cells along that axis. Zero XY extent produces no
degenerate triangles. Reversed/non-finite domains are rejected at this pure seam;
ordinary UI domains have already passed existing sanitation.

Keep an optional grid-to-compact-vertex map so invalid samples are not encoded as
fake origin vertices. Emit a cell only when **all four corners** are present;
otherwise omit both its triangles. Thus no triangle spans a known missing corner
and holes have conservative cell-sized margins. Also omit a cell with a
non-finite center probe; this adds one bounded probe per candidate cell and catches
some between-vertex singularities, without claiming all poles are detected.

For increasing X/Y corners `a=(i,j)`, `b=(i+1,j)`, `c=(i,j+1)`, `d=(i+1,j+1)`,
use `(a,b,c)` and `(b,d,c)`: geometric normals point toward +Z on a flat graph.
Front face is CCW and backface culling is enabled. Accumulate area-weighted face
normals from **surviving triangles only**, normalize safely, and remove unused
vertices (or give them finite +Z fallback normals if retained internally).
Compute differences/cross-products robustly in f64 to avoid f32 overflow;
degenerate triangles are omitted. Hole boundaries do not borrow values or normals
across a missing cell. No bottom, side walls, hole filling, or geometric caps:
this is an open graph, not a closed solid. Viewing its back can legitimately make
it disappear; state this in help rather than disabling required culling.

For an entirely finite, nondegenerate `nx × ny` grid, counts are `nx*ny` vertices
and `6*(nx-1)*(ny-1)` indices. With holes/degeneracy, compacted counts decrease.
Empty/all-invalid samples yield empty geometry and a status message, not a crash.
Bounds and automatic height range use vertices actually participating in visible
segments/triangles; isolated unrendered points do not drive framing.

## Renderer architecture and resource ownership

### Scene boundary

Keep vector scene construction and its current `SceneGPU` draw plan unchanged in
behavior. Introduce a discriminated scene payload at the app/renderer seam:
existing vector scene or a plot scene carrying immutable mesh/curve geometry,
separate reference lines, plot style and generation identity. This may be an
outer enum rather than adding optional mesh fields to every vector caller.
`Arc` geometry can retain the current dirty/reupload optimization pattern.

Plot scenes include **only** scalar geometry and ordinary grid/axes/domain
references. Arrows, streamlines, particles, click seeding and their transport
consumer checks are inactive while plotting; preserve their vector settings and
pause their evolution rather than advancing an invisible simulation. The scalar
is never a clock consumer. Restore vector behavior on return. Reference geometry
dimension follows explicit curve/surface kind, never `FieldExpr::dimension`.

### Offscreen pass in prepare; composite in paint

Propose a `PlotDrawState` stored in egui callback resources alongside existing
`DrawState`. It owns plot pipelines, uniform/geometry buffers, color/depth
textures and views, and the composite texture bind group. The plot callback
carries the exact logical viewport rect, immutable scene generation, camera
matrices and background. There is one plot viewport/callback per app frame.

1. In `prepare`, derive a physical target extent from the callback rect and
   `ScreenDescriptor.pixels_per_point`, using the same rounded pixel-rectangle
   convention as `PaintCallbackInfo::viewport_in_pixels`. Do not size from the
   whole window or the clip rect. Skip a zero-area viewport. Enforce device
   texture limits with proportional downscaling, not independent axis stretching;
   retain the original logical aspect for camera projection.
2. Allocate/reuse a **single-sample** color texture in the eframe target format,
   with `RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC`, and matching
   single-sample `Depth32Float` with `RENDER_ATTACHMENT`. Validate required format
   usage against the device; report unsupported configurations without panicking
   or silently reverting to an unoccluded mesh. Reallocate on size/format/device
   change. Clear color to the same opaque viewport background used by capture,
   supplied in linear space; clear depth to 1.0.
3. Upload only changed geometry; update this frame's uniforms. Encode the plot
   color/depth render pass directly into the **supplied egui encoder**, before
   the main egui render pass. Return no extra command buffers. This uses the
   verified prepare contract and avoids independent `queue.submit` ordering.
4. In `paint`, read the prepared resources and draw a texture-sampling full-rect
   triangle/quad into the **incoming pass**. The composite pipeline has no depth
   state, outputs opaque color (replace blend), and matches the incoming target
   format/sample count (current default is one). Respect egui's viewport/scissor;
   map the full target UV range correctly, without vertical inversion. Do not
   end/replace the incoming pass or attempt to attach plot depth there.

The composite is a separate tiny shader, not a replacement line shader. sRGB
targets must decode on texture sampling and encode on output exactly once;
do not manually gamma-correct both stages. Linear targets must stay linear.
Opaque clearing ensures grid/mesh alpha is composited once against the right
background, rather than leaking whatever UI happened to paint underneath.

Keep textures/views/bind groups/pipelines in callback resources through `paint`;
do not return references to locals from `prepare`. Replace resource sets as a
unit on resize, leaving submitted commands' handles alive through wgpu ownership.
Identify prepared generation/extent so a missing or mismatched preparation draws
no stale plot. Device recreation reinstalls resources; release obsolete large
targets when leaving plotting. If multi-viewport rendering is introduced later,
key resources per viewport rather than allowing prepare calls to overwrite one
shared uniform/texture set. It is not added by this feature.

### Depth and shading policy

- **Mesh:** triangle list, CCW front, back cull, opaque/no blending,
  `Depth32Float`, compare `LessEqual`, depth write enabled. Use interpolated
  normalized world-space normals, ambient plus directional diffuse lighting
  using the existing light direction. No specular/material settings. Base color
  comes from the existing colormap; illumination must not make holes opaque.
- **Plot reference lines:** reuse the exact existing line WGSL with a new
  depth-enabled pipeline, compare `LessEqual`, depth write disabled, existing
  line blend. Draw after mesh so foreground references are visible and references
  behind the opaque surface fail depth. They are not always-on-top decoration.
- **Curve:** same line WGSL, depth-enabled curve variant with `LessEqual` and
  depth write enabled. Draw references first, then the curve, so a curve in the
  same plane overlays its grid at equal depth. No geometry-displacing epsilon
  and no mesh pipeline accidentally bound in egui's no-depth pass.

Do not use the legacy no-depth line pipeline in the plot depth pass: attachment
compatibility must match for **all** its draws. Keep the original no-depth
pipeline for the unchanged vector path. Filter degenerate line instances before
upload; document the existing world-unit width convention in the plot control.

### PNG and shared geometry

Both live plots and PNG use one pure plot scene and one plot-pass recording
function, including the same references, accepted expression, normals, lighting,
height colors, camera and background. Capture must not reconstruct a vector
snapshot or approximate the mesh with lines. Screenshot size uses existing
`capture_size` and DPI/proportional cap rules; rerender the same plot pass into a
capture-owned color/depth pair at that size, then use existing padded-row/channel
conversion and native/wasm saving seams. The output remains the viewport image,
not the surrounding panels, matching current capture scope.

Capture resources/uniforms must be independent from live plot buffers and targets:
a screenshot occurs before callback prepare today and must not overwrite data
that a queued live draw will consume. Retain readback/capture handles until the
asynchronous completion unmaps them; all failure paths clear the existing pending
capture state and produce an error toast. No browser main-thread blocking poll.
Camera aspect/framing is from the source viewport even if capped capture rounding
differs by a pixel. A screenshot during an invalid draft captures the clearly
identified last-valid plot currently displayed, not the invalid draft.

## UI and transitions

Proposed wide layout (same controls reflow in compact layout):

```text
[Vector field | Curve y=f(x) | Surface z=f(x,y)]
[2D view | 3D view]    y = [ 0.5*x^2                         ]

Plot
  Example       [Custom / Harmonic oscillator potential ...]
  Samples       [513]          Line width (world units) [0.01]
  Color         [Viridis]      Range [Auto height | Manual]
  Manual min/max [...] [...]   [Fit plot]
  Advanced: resolution for surfaces; section reset
```

Only relevant controls appear: curves have sample count/line width, surfaces a
single square-grid resolution (not separate X/Y sliders). Proposed line default
is 0.01 world units, sanitized to 0.001..=0.2; surface default is 64 per axis.
Put detailed controls in the owning Plot panel, not a new global settings page.
Keep grid/axes and camera controls available; hide or disable vector-only controls
with an explanation, preserving their values. Placeholder/help says `x` only for
curves, `x and y` for surfaces, versus comma-separated vector components.

### Projection versus kind

The concepts remain separate in the model even though scalar UX pairs them:

| Action | Proposed behavior |
| --- | --- |
| Select Curve | Activate saved curve draft, set orthographic 2D, resample curve. |
| Select Surface | Activate saved surface draft, set perspective 3D, resample surface. |
| 3D view while in Curve | Explicitly select Surface and its separate draft; never reinterpret curve text. |
| 2D view while in Surface | Explicitly select Curve and its separate draft; never flatten/slice surface. |
| View toggle in Vector | Existing vector projection/sampling behavior, unchanged by #12. |
| Return to Vector | Restore preserved vector expression/settings and last vector projection. |

Show a toggle tooltip/help notice that in plot mode changing 2D/3D selects the
corresponding saved scalar plot. This is a proposed resolution of #12's ambiguous
"2D/3D switching" wording; camera-only surface projections and 3D curve views
are not introduced. An invalid destination draft shows its own error and empty
plot until corrected. Kind/example changes release pointer capture and leave fly
mode safely; no hidden captured-pointer seeding. Retain vector camera pose in
transient memory while navigating plots; scalar camera changes do not unexpectedly
move it on return. Cameras are still not persisted on launch.

Vector presets and CAS apply must not write a vector formula into the active
scalar editor. Proposed behavior: choosing a vector preset explicitly returns to
Vector; CAS remains a separate rail but vector-only Apply is disabled in plot
mode with explanatory text. Do not expand #14 into scalar CAS support.

### Built-in examples (exact #12 names and expressions)

| Kind | Name | Right-hand-side expression |
| --- | --- | --- |
| Curve2D | Harmonic oscillator potential | `0.5*x^2` |
| Curve2D | Gaussian wave packet | `exp(-x^2)*cos(8*x)` |
| Curve2D | Damped oscillator | `exp(-0.2*abs(x))*sin(3*x)` |
| Surface3D | Point-charge potential | `1/(sqrt(x^2+y^2)+0.1)` |
| Surface3D | Two like charges | `1/(sqrt((x-0.6)^2+y^2)+0.1) + 1/(sqrt((x+0.6)^2+y^2)+0.1)` |
| Surface3D | Quantum oscillator ground state | `exp(-(x^2+y^2))` |
| Surface3D | Standing wave | `sin(3*x)*cos(3*y)` |
| Surface3D | Radial outgoing wave | `sin(6*sqrt(x^2+y^2))/(1+sqrt(x^2+y^2))` |

Use a small static catalogue with stable kind/name/source entries, no new preset
storage framework. Selecting any entry sets kind/projection and its draft, then
uses the ordinary parse/rebuild path. All must parse with current `meval` and
have finite grid samples on `Domain::default()`. Explain that the charge examples
include epsilon **0.1** to regularize singularities; these are illustrative
dimensionless formulas, not a newly approved physical solver or unit system.

### Colormap, domain and camera fitting

Reuse Viridis/Turbo/Mono and existing color mapping; plot color is **signed
height**, not absolute magnitude. Curves may color each segment by mean endpoint
height; surfaces use per-vertex height color interpolated by the mesh shader.
Automatic range uses finite rendered geometry, not missing samples. Manual
finite min/max clamp colors only, never heights. Reversed/equal/non-finite input
must be sanitized deterministically (swap reversed endpoints, use existing gray
constant-range behavior for equal endpoints, defaults for non-finite values).
Label the legend "Height y" or "Height z". Keep plot manual range/auto mode
separate from vector magnitude range so returning does not change vector colors;
the palette selector itself may remain shared.

`F` / **Fit domain** continues to mean the existing domain-framing command, not
fit scalar extrema. Add **Fit plot** as a labelled button and help-table entry,
without a new keyboard shortcut. It frames finite rendered bounds, including
values outside the domain: X/Y for a curve and XYZ for a surface. Empty geometry
disables it with explanation; flat/single-axis bounds receive a small finite
padding. Proposed margin: 10%, with aspect-aware bounds fitting. No automatic
fit on each keystroke. Domain edits resample, but do not clamp heights.

Implement fit through a **plot-only camera/framing seam**, not by silently
changing vector orbit limits or calling its distance-clamped Fit domain helper.
Use f64 bounds/fit arithmetic; allow a plot camera distance and orthographic
extent large enough for its bounds, and derive plot near/far planes from those
bounds and eye position with finite padding. Fit plot exits fly mode. Reuse
existing orbit/pan gestures and world-axis conventions but supply plot-specific
projection parameters; use these exact matrices for live and PNG. If bounds
cannot produce finite, usable GPU matrices, fail Fit plot visibly and retain the
previous view rather than claiming a successful fit or clipping data to fit.
Ordinary offscreen/frustum clipping remains a camera property, not domain-Z
clamping. Domain references may remain outside a plot-only fit; its label promises
to frame the plot, not the union of every reference line and the domain.

### Persistence and reset

Extend the existing serde-defaulted `ViewRecord` inside the same UI envelope:
plot kind, curve and surface source strings, resolution/count, line width,
plot range/auto settings, and last vector projection. Keep the current `formula`
field as the vector expression for compatibility; do not repurpose it to the
currently visible scalar. Missing plot fields load **VectorField** and proposed
scalar defaults. Preserve old `mode_3d` behavior when the new remembered-vector
projection field is absent. Restore all sources/controls through their setters,
then select kind and reconcile scalar projection; contradictory scalar kind/view
records resolve to that kind's required projection. Corrupt new fields follow
existing envelope fallback/sanitation conventions, not unchecked allocation.

Persist drafts, not GPU meshes or last-valid caches. Invalid scalar text restored
after restart shows its error and no plot until corrected; do not persist a
hidden substitute expression. Test valid round trips and this explicit invalid
draft behavior. Global Reset returns to vector mode, the existing default vector
formula and controls/clock, default scalar drafts and plot controls, and no stale
plot/errors/capture state. Plot-section reset restores only plot settings and its
default draft, without erasing the saved vector formula.

### Honest export support

PNG supports both plot kinds through the depth-equipped plot pass described
above. Preserve current vector PNG and CLI behavior. No scalar command-line flags
or output modes are added; CLI scene construction remains explicitly vector-only
and must not start rendering persisted UI plot kinds by accident.

Scalar SVG export is outside #12's required scope. Disable it for **both scalar
kinds**, with "Scalar plots require PNG; SVG export supports vector mode" in
menu/button/help, and guard again at execution. Do not let a plot request fall
through to vector arrows. No fake raster-as-vector, wireframe substitution, or
successful empty SVG. Preserve existing vector SVG support and its element
cap/error behavior.

This is the controller's scope-minimizing proposal following independent builder
review: the GUI SVG exporter treats line width as a pixel stroke with a minimum,
whereas live lines use world units. Correct curve SVG would require an additional
projection/width contract and exporter work not requested by #12. It is not
silently treated as equivalent to the existing GUI export path.

## Acceptance and evidence gates

Tests belong in `tests/<module>_test.rs`, registered in `tests/suite.rs`; no inline
source test modules or ignored tests. Test the production seams, not duplicate
predicates. Implementation must provide evidence for:

- Scalar constants/functions and `y=f(x)`/`z=f(x,y)` semantics; errors for unknown
  variables/functions, wrong context, assignment syntax and malformed expressions;
  valid expressions with partly undefined domains are accepted by parsing.
- Curve counts/endpoints, zero/one/capped requests, non-finite/f32-overflow gaps,
  segment splitting, bounded midpoint policy and pole-between-grid-point cases.
- Surface exact finite-grid counts, documented CCW winding (+Z for a plane),
  finite unit normals, sloped-plane normals, invalid-corner/center holes, no indices
  referencing invalid vertices, degeneracy, empty data and allocation/work caps.
- Every exact example's unique name, correct kind, parse and finite default-domain
  samples; selection exercises the real kind/setter/rebuild path.
- Typed-edit last-valid behavior versus invalid destination-kind clearing; no
  cross-kind stale scene; separate source preservation, projection transitions,
  reset, old RON fixtures, round trips, and corrupt plot controls.
- Scalar mode suppresses vector geometry, seeding, particle advancement and clock
  consumers without altering vector controls. Returning restores vector behavior.
  Existing vector parsing, sampling, presets, CAS rails, selected-plane behavior,
  alpha/order, persistence and CLI export regressions stay covered.
- Height/manual range and palette behavior, all-invalid/constant data, aspect-aware
  Fit plot outside domain Z, fixed vector Fit domain semantics and plot camera
  matrix finiteness/near-far behavior.
- Shared plot scene/pass selection for viewport and PNG, capture extent/DPI/caps,
   row/channel conversion, failure lifecycle and SVG rejection for both scalar
   kinds. Automated UI tests cover compact/wide control layout and help text.

Pure tests cannot validate a wgpu shader or attachment compatibility. Runtime
evidence must exercise an actual device, constructing both new depth-enabled
line variants, the lit mesh pipeline and no-depth composite pipeline; render a
curve and a surface with reference lines, perform a resize and PNG readback,
and return to the vector path without validation errors. Add a deterministic
device-backed test/probe where practical for overlapping near/far geometry and
an invalid-sample hole (readback assertions, not just successful submission).
Do not call ordinary vector-only startup smoke proof that an unvisited plot
pipeline works. Record backend/device and exactly which paths ran; device
unavailability is missing evidence, not a pass.

Required implementation gates per current Workflow: `cargo build`,
`scripts/test.sh` (full `cargo test --all-targets`),
`cargo clippy --all-targets -- -D warnings`, changed-file formatting,
`python3 scripts/check-docs.py`, `git diff --check`, and
`scripts/smoke-run.sh 15` for renderer/shader/input changes. Follow Workflow's
explicit Rustup toolchain, Cargo concurrency/target limits and initial independent
review requirements. Controller records exact evidence and any finding closure
before integration.

Dedicated web/wasm compatibility and browser/manual visual, screenshot, and
separate painted-ink checks are **currently owner-waived, not passed**, under
Workflow. Preserve automated functional/layout tests and wasm code paths anyway;
this waiver does not remove browser product support or native runtime smoke.
Optional native/browser screenshots can illustrate the proposed result when
available, but are not evidence that unperformed checks passed.

For this design-only artifact, applicable checks are docs/link/policy consistency
and diff checks, not Cargo or renderer runtime tests. No implementation or visual
result is claimed by this document.

## Approval frontier and remaining discretion

**Historical approval frontier:** items 1–5 below were approved with the full
design on 2026-11-15; the proposal wording is preserved for provenance. The
escalation boundary in the final paragraph remains applicable.

The selected lit-mesh approach is settled. The following are **designer proposals
requiring approval as part of the full design**, not already accepted mandates:

1. Plot-mode 2D/3D toggles select the corresponding separate curve/surface draft;
   they do not expose camera-only projections of the same scalar object.
2. Invalid edits retain the active last-valid plot, but entering a context with
   an invalid draft clears geometry; invalid restored drafts have no hidden cache.
3. Curve midpoint/jump guarding is conservative and bounded, with a documented
   false-gap tradeoff; finite sampling cannot guarantee all asymptotes are found.
4. Initial drafts, sample/resolution/width limits, one-sided open mesh semantics,
   10% Fit plot padding, and separate plot range settings are proposed defaults.
5. PNG supports both scalar kinds; SVG explicitly rejects both. Scalar SVG, CAS
   apply and CLI scalar exports are not part of this issue.

The builder may choose concrete module/type names, buffer packing and cache
mechanics within these contracts. A discovery requiring global eframe depth,
weaker occlusion, a wireframe fallback, changed gap semantics, altered vector
behavior or additional dependencies returns to the controller/owner for design
resolution rather than being hidden as implementation discretion. After owner
approval and independent design review, a separate implementation plan may be
commissioned; none is included or implied here.
