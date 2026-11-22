# Vendored fonts

These files are compiled into both the native and WebAssembly binaries. They
replace epaint's bundled Ubuntu-Light/Hack faces, whose Ubuntu Font Licence is
not GPL-compatible for a combined work (ticket 67).

All font files are **unmodified** copies from the sources below. Do not subset,
re-hint, or otherwise rewrite them; regenerate this directory only when moving
to a new source version, and update the hashes below in the same commit.

| File | Version | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `DejaVuSans.ttf` | DejaVu 2.37 | 759,720 | `6038a160b491e121c1f12c7bccb4a9c8730296e3adc1086a059404ed84b7451c` |
| `DejaVuSansMono.ttf` | DejaVu 2.37 | 343,140 | `b5babc084554ebdd142fe099d8b7c573ff161738bca913723d3e84f5c3df1c78` |
| `STIXTwoMath-Regular.otf` | STIX Two Math 2.13 b171 | 838,652 | `3a5f3f26f40d5698b3c62dd085d48d6663696a3f80825aab8b553d5097518e8c` |
| `egui-icons.ttf` | epaint_default_fonts 0.37.0 | 3,484 | `122806555c73df7cef55aa1d945289ae27fbc4dbbd31abbb4204ee0cda829cc8` |

## Sources

- `DejaVuSans.ttf` and `DejaVuSansMono.ttf`: the DejaVu Fonts 2.37 files
  installed by the host distribution at `/usr/share/fonts/TTF/`, copied
  byte-for-byte. Their name tables report version 2.37. They are **not** the
  assets attached to the upstream
  <https://github.com/dejavu-fonts/dejavu-fonts/releases/tag/version_2_37>
  release: the distro copies are 759,720 / 343,140 bytes, while the GitHub
  release assets are 757,076 / 340,712 bytes, and their SHA-256 values differ.
  The hashes above therefore identify the bytes actually vendored here; they do
  not match the upstream release assets.
- `STIXTwoMath-Regular.otf`: STIX Two Math 2.13 b171 by the STIX Fonts Project
  Authors, <https://github.com/stipub/stixfonts>, from the host distribution's
  `/usr/share/fonts/stix/`, copied byte-for-byte. It supplies the Mathematical
  Alphanumeric Symbols (U+1D400–U+1D7FF) that DejaVu lacks. Its verbatim OFL
  notice is from `/usr/share/licenses/otf-stix/LICENSE`.
- `egui-icons.ttf`: the icon subset shipped by the `epaint_default_fonts`
  0.37.0 crate (copied from the local cargo registry), from the Emoji Icon Font
  by John Slegers, <https://github.com/jslegers/emoji-icon-font>.

The install seam is `src/fonts.rs`, which uses `FontDefinitions::empty()` and
explicitly lists the four faces. `install_startup_fonts` adds the KaTeX typeset
faces on top before the first frame:

- `Proportional`: `DejaVu Sans`, `STIX Two Math`, `egui-icons`
- `Monospace`: `DejaVu Sans Mono`, `DejaVu Sans`, `STIX Two Math`, `egui-icons`

## Licences

- `DejaVu-LICENSE.txt`: verbatim name-table ID 13 notice from
  `DejaVuSans.ttf` — Bitstream Vera Fonts Copyright, Arev Fonts Copyright
  (Tavmjong Bah), and the public-domain DejaVu statement.
- `STIXTwoMath-LICENSE.txt`: verbatim SIL OFL 1.1 licence from
  `/usr/share/licenses/otf-stix/LICENSE`, Copyright 2001-2021 The STIX Fonts
  Project Authors, with Reserved Font Name "TM Math".
- `egui-icons-LICENSE.txt`: verbatim MIT licence from
  `emoji-icon-font-mit-license.txt`, Copyright (c) 2014 John Slegers.

[`THIRD_PARTY_LICENSES.md`](../../THIRD_PARTY_LICENSES.md) repeats these
notices; these files are the raw sources for its DejaVu, STIX, and egui-icons
sections.
