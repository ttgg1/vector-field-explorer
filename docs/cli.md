# Native command-line plot export

Run `vector-field-explorer` with **no arguments** to open the normal GUI.
With arguments, it runs once without a window, dialogs, saved settings or camera
presets. It writes only the explicitly selected output file (overwriting an
existing file); parent directories must already exist. GPU drivers may maintain
their own caches; these are not GUI settings or camera persistence. `--help` and `--version`
exit successfully without a display or GPU. Errors go to stderr with a nonzero
exit status. This CLI is native-only; the browser startup is unchanged.

```sh
# Actual vector geometry, CPU-only: no GPU or display needed.
vector-field-explorer --formula '(-y,x)' --density 24 -o vortex.svg

# Displayless GPU PNG, with a time-dependent field and seeded particles.
vector-field-explorer --mode 3d --formula '(-y,x,sin(t))' \
  --particles true --particle-count 200 --trail-len 16 \
  --start-time 1 --duration 0.5 --seed 0 --width 1024 --height 768 -o vortex.png

# Override individual config members, including explicit false and auto range.
vector-field-explorer --config plot.json --arrows false --color-range auto -o flow.svg
```

Example `plot.json`:

```json
{
  "output": "flow.svg",
  "formula": "(-y,x)",
  "streamlines": true,
  "width": 900,
  "height": 600,
  "color_range": null,
  "references": { "grid_and_domain_box": false, "world_axes": true },
  "camera": { "target": [0,0,0], "ortho_zoom": 1 },
  "background": [27,27,27,255]
}
```

## Precedence and format

Settings resolve as **mode-specific defaults → JSON config → explicit flags**.
`--config PATH` reads strict JSON, not the GUI persistence format. Unknown keys
at every nesting level fail. Every supplied config value is checked before
overlay: an invalid value cannot be hidden by a flag. Nested objects merge one
member at a time. Omission inherits; `color_range: null` or `--color-range auto`
selects automatic range; no other member accepts null. Explicit false/zero are
values, not omission. Enums use the spellings below; booleans require `true` or
`false`. CLI tuples are comma-separated without spaces; JSON tuples are arrays.
Quote formulas and shell-sensitive values.

An output path is required after merging. Its case-insensitive `.svg`/`.png`
extension selects the backend. SVG is pure 2D vector geometry, never a raster
wrapper; 3D SVG fails. PNG supports 2D/3D and requires a wgpu GPU adapter
(a compatible software Vulkan adapter also works). It needs no display server,
but still needs installed drivers. No adapter is an actionable error, **not a
CPU-rendering fallback**. Native paths are preserved without lossy UTF-8 conversion;
JSON itself is UTF-8. Relative config output paths are relative to the process's
working directory, not the config's directory.

## Complete JSON / flag reference

All numbers must be finite. Bounds below are inclusive unless stated otherwise.
Dot-separated keys denote nested JSON objects, not literal dotted keys.
Unspecified values use the existing app's mode-specific defaults.

| JSON key | Flag | Values / default |
| --- | --- | --- |
| `output` | `-o`, `--output` | path; required |
| `width`, `height` | `--width`, `--height` | integers 1–4096 each; defaults 1280, 800 independently; product ≤16,777,216 |
| `mode` | `--mode` | `2d`, `3d`; default `2d` |
| `formula` | `--formula` | meval component list; default `(x, y)` / `(x, y, z)` |
| `domain.min`, `domain.max` | `--domain-min`, `--domain-max` | XYZ triples, each −100–100, min < max on every axis; defaults `[-1,-1,-1]`, `[1,1,1]` |
| `density` | `--density` | integer 4–200; effective volumetric sampling max 40; defaults 24 / 16 |
| `arrows`, `streamlines`, `particles` | `--arrows`, `--streamlines`, `--particles` | bool; defaults true, false, false |
| `arrow_scale` | `--arrow-scale` | 0.1–4; default 1 |
| `arrow_opacity` | `--arrow-opacity` | 0.05–1; default 1 |
| `plane_dim_opacity` | `--plane-dim-opacity` | 0–1; default 0.2 |
| `arrow_length_mode` | `--arrow-length-mode` | `proportional`, `uniform`, `magnitude`; default `proportional` |
| `magnitude_scale` | `--magnitude-scale` | 0.01–10; default 1 |
| `proportional_exponent` | `--proportional-exponent` | 0.25–4; default 1 |
| `colormap` | `--colormap` | `viridis`, `turbo`, `mono`; default `viridis` |
| `color_range` | `--color-range` | JSON null or `[MIN,MAX]`; flag `auto` or `MIN,MAX`; min < max; default automatic |
| `arrow_style_3d` | `--arrow-style-3d` | `cone`, `cylinder`, `flat`, `streamline`; default `cone` |
| `arrow_style_2d` | `--arrow-style-2d` | `flat`, `streamline`; default `flat` |
| `arrow_params.head_length` | `--head-length` | 0.1–0.5; default 0.3 |
| `arrow_params.head_width` | `--head-width` | 0.02–0.3; default 0.12 |
| `arrow_params.shaft_radius` | `--shaft-radius` | 0.01–0.12; default 0.055 |
| `arrow_params.sides` | `--arrow-sides` | integer 4–16; default 12 |
| `streamline_seed_mode` | `--streamline-seed-mode` | `auto`, `click`; default `auto` |
| `plane.enabled` | `--plane-enabled` | bool; default false |
| `plane.axis` | `--plane-axis` | `x`, `y`, `z`; default `z` |
| `plane.index` | `--plane-index` | integer 0 through final grid-axis length minus 1; default middle of selected axis |
| `particle_count` | `--particle-count` | integer 0–4000; default 0 = Auto (density ×16, capped 4000) |
| `particle_size` | `--particle-size` | 0.25–3; default 1 |
| `particle_appearance.opacity` | `--particle-opacity` | 0.05–1; default 1 |
| `particle_appearance.color_mode` | `--particle-color` | `magnitude`, `speed`, `fixed`; default `magnitude` |
| `particle_appearance.fixed_color` | `--particle-color-rgb` | RGB bytes 0–255; default `[0,150,136]`; opaque RGB, opacity controlled separately |
| `particle_appearance.trail_len` | `--trail-len` | integer 0–32 requested points; default 0 |
| `particle_appearance.trail_fade` | `--trail-fade` | 0–1; default 0.65 |
| `particle_appearance.trail_width` | `--trail-width` | 0.05–2; default 1 |
| `particle_appearance.loop_on_stagnation` | `--loop-on-stagnation` | bool; default true |
| `particle_appearance.stagnation_seconds` | `--stagnation-seconds` | 0.1–5 seconds; default 0.5 |
| `camera.target` | `--camera-target` | XYZ triple; default domain center |
| `camera.kind` | `--camera-kind` | `orbit`, `fly`; default `orbit`; fly requires 3D |
| `camera.position` | `--camera-position` | XYZ triple; fly only; default pose derived from default orbit |
| `camera.yaw` | `--camera-yaw` | radians, periodic; default orbit 0.8 / derived fly yaw |
| `camera.pitch` | `--camera-pitch` | radians −1.55–1.55; default orbit 0.9 / derived fly pitch |
| `camera.distance` | `--camera-distance` | 0.5–20 world units; 3D orbit only; default 3.2 |
| `camera.fov_y_degrees` | `--camera-fov-deg` | strictly between 0 and 180 degrees; 3D orbit only; default 55 |
| `camera.ortho_zoom` | `--camera-ortho-zoom` | 0.05–50; 2D only; default 1 |
| `references.grid_and_domain_box` | `--references-grid-and-box` | bool; default true; grid and 3D domain box are one coupled choice |
| `references.world_axes` | `--references-world-axes` | bool; default true; independent world axes |
| `background` | `--background-rgba` | straight RGBA bytes 0–255; default `[27,27,27,255]` |
| `seed` | `--seed` | unsigned 64-bit integer, zero valid; default `1311768467463790320` (`0x123456789ABCDEF0`) |
| `start_time_seconds` | `--start-time` | 0–3600 simulated seconds; default 0 |
| `duration_seconds` | `--duration` | 0–3600 simulated seconds; default 0; start + duration ≤3600 |
| `fixed_step_seconds` | `--fixed-step` | 0.0001–0.25 simulated seconds; default 1/60 |

## Sampling, simulation and appearance

Formulas bind existing meval names `x,y,z,t`. Unknown variables/functions fail;
valid zero/singular fields are allowed, with nonfinite samples skipped. Mode is
resolved before its defaults. A two-component formula in 3D mode is allowed:
sampling is flat at domain mid-z, with density up to 200; three-component 3D
sampling caps density at 40. Particles require 3D mode (also for flat formulas).
`click` streamline seeding produces no automatic seeds: a one-shot has no click
history. Plane index is checked against the final effective grid; 2D plane
controls are inert. No GUI interaction, transport, fly movement or CAS/physics
controls are exported.

Time inputs retain f64 precision for scheduling; the existing field/RK4 math is
f32. Initial particles spawn at start time; each step integrates at its start
and intermediate RK4 times. Indexed boundaries avoid cumulative clock drift;
the last step is shortened to the endpoint. The final field/streamline/color
snapshot uses start + duration even without particles. Zero duration samples
at start with no advection. More than **100,000 steps**, nonfinite schedules,
or f32 boundaries/intermediate stages that cannot advance are rejected before
simulation work. This is a work bound, not an accuracy guarantee. The seed
controls only existing particle spawn/respawn randomness. Trails obey the
existing 100,000-total-point budget: requesting 4000 particles ×32 points is
valid but yields 25 effective points per particle.

Equivalent resolved inputs, seed and duration reproduce the existing deterministic
field/particle state. This does not promise byte-identical PNGs across GPU
drivers/platforms; SVG serialization is deterministic for the same prepared scene.

The 2D camera is front orthographic, centered at domain mid-z, fitting the XY
box to output aspect with a 1.35 margin. Only target and ortho zoom apply. 3D
orbit uses the app's default pose around domain center, not an automatic fit;
large boxes may crop. Fly starts from that orbit's derived pose, accepts only
position/yaw/pitch and uses fixed 55° vertical FOV. Explicit mode-inapplicable
camera members fail rather than being silently ignored. Both backends use one
shared output frame, existing clipping planes and final scene ordering.

SVG projects actual Flat arrow shafts/heads or headless Streamline shafts,
preserving geometry parameters, opacity, world-space widths and clipping.
Instance colors are linear and converted to display sRGB in SVG. PNG uses an
sRGB render target and linear clear/blend math; readback is display-encoded.
Both preserve configured background alpha, including transparency; the default
is the GUI dark-panel gray, not an invented export theme. SVG is not promised
to be pixel-identical to GPU lighting/antialiasing.
