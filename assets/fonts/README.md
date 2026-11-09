# Vendored fonts

These files are compiled into both the native and WebAssembly binaries. They
replace epaint's bundled Ubuntu-Light/Hack faces, whose Ubuntu Font Licence is
not GPL-compatible for a combined work (ticket 67).

All font files are **unmodified** upstream releases. Do not subset, re-hint, or
otherwise rewrite them; regenerate this directory only when moving to a new
upstream release, and update the hashes below in the same commit.

| File | Version | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `DejaVuSans.ttf` | DejaVu 2.37 | 759,720 | `6038a160b491e121c1f12c7bccb4a9c8730296e3adc1086a059404ed84b7451c` |
| `DejaVuSansMono.ttf` | DejaVu 2.37 | 343,140 | `b5babc084554ebdd142fe099d8b7c573ff161738bca913723d3e84f5c3df1c78` |
| `egui-icons.ttf` | epaint_default_fonts 0.37.0 | 3,484 | `122806555c73df7cef55aa1d945289ae27fbc4dbbd31abbb4204ee0cda829cc8` |

## Sources

- `DejaVuSans.ttf` and `DejaVuSansMono.ttf`: DejaVu Fonts 2.37,
  <https://dejavu-fonts.github.io/> (release
  <https://github.com/dejavu-fonts/dejavu-fonts/releases/tag/version_2_37>).
- `egui-icons.ttf`: the icon subset shipped by the `epaint_default_fonts`
  0.37.0 crate, from the Emoji Icon Font by John Slegers,
  <https://github.com/jslegers/emoji-icon-font>.

The install seam is `src/fonts.rs`, which uses `FontDefinitions::empty()` and
explicitly lists the three faces:

- `Proportional`: `DejaVu Sans`, `egui-icons`
- `Monospace`: `DejaVu Sans Mono`, `DejaVu Sans`, `egui-icons`

## Licences

- `DejaVu-LICENSE.txt`: verbatim name-table ID 13 notice from
  `DejaVuSans.ttf` — Bitstream Vera Fonts Copyright, Arev Fonts Copyright
  (Tavmjong Bah), and the public-domain DejaVu statement.
- `egui-icons-LICENSE.txt`: verbatim MIT licence from
  `emoji-icon-font-mit-license.txt`, Copyright (c) 2014 John Slegers.

`THIRD_PARTY_LICENSES.md` is owned by ticket 19; these files are the raw
sources for its DejaVu and egui-icons notices.
