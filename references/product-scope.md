# Product and release constraints

Consult when changing platform support, packaging, onboarding or release scope.
User-facing setup/browser details live in [README](../README.md); current work
authorization and temporary verification waivers live in [Workflow](workflow.md).

## v0.1.0 charter (historical acceptance baseline)

- Linux x86_64 single-file AppImage; macOS x86_64/arm64 unsigned `.app` zips.
  No signing, notarization, auto-update or telemetry.
- Real wasm at `https://ttgg1.github.io/vector-field-explorer/` with formula,
  field presets, 2D/3D, arrows/streamlines/particles, click-to-seed, colormaps,
  camera, browser persistence and screenshot download parity. WebGPU only,
  no WebGL2 fallback. Firefox-first support terms and enable instructions are
  documented in README; unsupported browsers receive guidance.
- README quickstart, three Start here presets and in-app help. Original manual
  five-minute native/web check: launch → preset → edit formula → 3D → seed.
- Performance floors: native 60 fps default / 30 fps densest preset at 1280×800
  integrated GPU; web 30 fps default; startup <2 s native / <5 s web.
- No v0.1.0 localization; keep user-facing strings in `ui/` modules.

## Release boundaries

SemVer, on-demand patch/minor releases, no fixed cadence/backports. The original
v0.1.0 tag required ticket 19 green; consult Git/release evidence rather than
redispatching that historical gate. Windows is a v0.2.0 feature, not a v0.1.0
patch/backport. Windows packaging dry-run debt and public-launch status belong
in the live queue/handoff; neither implies authority to publish a real release
or change visibility. GitHub Issues/PRs accepted; no CLA.

Future physics scope/phase prerequisites remain in accepted tickets and
[ADRs](../docs/adr/), not this charter or a queue estimate. The
[roadmap history](../docs/wiki/roadmap/post-v1-frontier.md) is provenance, not
an implementation order.
