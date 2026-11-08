# 16: GitHub Pages release site

**What to build:** a static project/release site published with GitHub Pages:
what the app is, screenshots, feature list, and download buttons pointing at
the latest GitHub Release assets. This is a landing/download page, **not a
browser build** — the design spec lists a browser build as a v1 non-goal, and
the app depends on native GPU/window/dialog APIs. A playable wasm build would
be a separate ticket with a large porting effort.

**Blocked by:** 17 (release assets must exist; the page can be authored before
the first tag, but its download links are verified after `v0.1.0`).

**Prereq to verify first:** GitHub Pages on a **private** repository requires a
plan that supports it (Pro/Team/Enterprise; not Free). If Pages cannot be
enabled, stop and report the blocker with the plan/visibility options — do not
fake the deployment. Fallbacks: make the repo public at release time, or host
the same static files elsewhere.

**Status:** ready-for-agent

## Deliverables

- `site/index.html`, `site/style.css` (no framework, no build step):
  - hero: name, one-line pitch, primary "Download latest release" button;
  - 1–2 real screenshots (reuse ticket 08 capture or a user-supplied image);
  - features: typed formulas, 2D/3D, streamlines, particles, click-to-seed,
    camera presets, screenshot export, themes;
  - install: direct links to `releases/latest` assets for Linux x86_64 and
    macOS (x86_64 + Apple silicon), plus a "build from source" snippet;
  - quick controls summary (mouse gestures per mode, top menus);
  - footer: GPL-3.0-only, repo link, issue link.
- `.github/workflows/pages.yml`: on push to `main` touching `site/**` and
  `workflow_dispatch`; permissions `pages: write`, `id-token: write`;
  `actions/upload-pages-artifact` (path `site`) + `actions/deploy-pages`.
- `scripts/check-site.sh`: fails on missing local files/links or missing
  required strings (title, license, download URL); run in CI.
- README link to the published URL (ticket 09 owns the README; add the link
  there).

## Acceptance

- [ ] Pages URL is live and serves the site (or a documented plan blocker).
- [ ] Every download link resolves to the latest release assets.
- [ ] No claim that the app runs in the browser.
- [ ] Mobile-width layout is readable; no console errors.
- [ ] `scripts/check-site.sh` passes locally and in CI.
- [ ] Commit `docs: GitHub Pages release site`.

## Non-goals

Wasm/browser build, custom domain/DNS, analytics, version auto-detection JS,
release-note rendering beyond a link.

## Scheduling

After ticket 17 (release pipeline), before ticket 09's final review. CI-only
work; it may run in parallel with ticket 13 only if no files overlap
(`.github/workflows/` and `site/` vs `src/`/`tests/`), but the
sequential-implementers rule still applies.
