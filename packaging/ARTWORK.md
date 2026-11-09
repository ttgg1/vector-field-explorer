# Artwork and icon provenance

The only artwork in this repository is the Vector Field Explorer mark.

- **Source:** `packaging/linux/vector-field-explorer.svg` (SVG, 256×256 viewBox).
- **Author:** The Vector Field Explorer authors.
- **Licence:** GPL-3.0-only, the same licence as the rest of the project. See
  `LICENSE`.
- **Generated on:** 2026-11-09.
- **Third-party assets:** none. The mark, wordmark, and Open Graph card are
  original geometric work by the project authors. The OG card uses the shipped
  DejaVu Sans face only for text; its licence is in
  `assets/fonts/DejaVu-LICENSE.txt`.

There is no icon font, stock illustration, or traced third-party logo in the
project. The mark may be reused under GPL-3.0-only.

## Generated files

The generators are maintainer tools, not build steps. Every output below is
committed, so CI and a release build never need an SVG renderer, Python, or
Pillow.

| Path | Size / format |
| --- | --- |
| `packaging/icons/vector-field-explorer-16.png` | 16×16 PNG |
| `packaging/icons/vector-field-explorer-32.png` | 32×32 PNG |
| `packaging/icons/vector-field-explorer-48.png` | 48×48 PNG |
| `packaging/icons/vector-field-explorer-64.png` | 64×64 PNG |
| `packaging/icons/vector-field-explorer-128.png` | 128×128 PNG |
| `packaging/icons/vector-field-explorer-256.png` | 256×256 PNG |
| `packaging/icons/vector-field-explorer-512.png` | 512×512 PNG |
| `packaging/icons/vector-field-explorer.icns` | Apple Icon Image, 10 elements |
| `packaging/linux/vector-field-explorer-256.png` | 256×256 PNG (stable linuxdeploy path) |
| `web/icons/favicon-16.png` | 16×16 PNG |
| `web/icons/favicon-32.png` | 32×32 PNG |
| `web/icons/apple-touch-icon.png` | 180×180 PNG |
| `web/icons/icon-192.png` | 192×192 PNG |
| `web/icons/icon-512.png` | 512×512 PNG |
| `web/og-image.png` | 1280×640 PNG (Open Graph / Twitter card) |

The `packaging/linux/vector-field-explorer-256.png` file is intentionally a
byte-identical copy of the 256 px ladder image. The packaging workflow passes
this file to linuxdeploy's `--icon-file`, so the copy keeps that path stable
if the ladder layout under `packaging/icons/` changes.

## Exact commands

From the repository root, with `rsvg-convert` (or `inkscape`) installed:

```sh
scripts/gen-icons.sh
python3 scripts/make-icns.py
python3 scripts/make-og-image.py
```

`scripts/gen-icons.sh` prefers `rsvg-convert` and falls back to `inkscape`. It
renders the seven packaging sizes, keeps the stable linuxdeploy 256 px path in
sync, and renders/copies the five browser sizes.

`scripts/make-icns.py` writes the ICNS container directly because Linux has
neither `iconutil` nor `png2icns`. It emits `icp4`/`icp5`/`icp6` plus
`ic07`–`ic09` and the retina aliases `ic11`–`ic14`.

Verification performed on Linux:

```sh
file packaging/icons/vector-field-explorer.icns
# Mac OS X icon, ... "icp4" type
```

The script's own check verifies the `icns` magic, the declared length, every
element length, and that each PNG slot starts with a PNG signature. `sips` is
not available on Linux; on macOS the same file can be checked with:

```sh
sips -g pixelWidth -g pixelHeight packaging/icons/vector-field-explorer.icns
```

The packaging workflow additionally validates the `.icns` presence in
`Contents/Resources/` and lints `Info.plist` with `plutil` on the macOS runner.

`scripts/make-og-image.py` composes the 1280×640 card from
`packaging/icons/vector-field-explorer-256.png`, the wordmark, the tagline, and
`assets/fonts/DejaVuSans.ttf`.

## Regeneration policy

Regenerate only when the canonical SVG or the brand copy changes, then commit
the outputs together with the source change. Do not generate icons during CI:
release artifacts must be reproducible from the committed tree.
