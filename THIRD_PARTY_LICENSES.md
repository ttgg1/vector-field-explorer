# Third-party licences

Vector Field Explorer itself is licensed under GPL-3.0-only; see
[LICENSE](LICENSE). This file covers the third-party components that are
compiled into or distributed with the app.

## Fonts

The app ships three font files under `assets/fonts/`. The provenance, version,
and SHA-256 of each file are recorded in
[`assets/fonts/README.md`](assets/fonts/README.md).

### DejaVu Sans and DejaVu Sans Mono

`DejaVuSans.ttf` and `DejaVuSansMono.ttf` are DejaVu Fonts 2.37. DejaVu changes
are in the public domain; the glyphs imported from Bitstream Vera and Arev keep
their own notices. The verbatim notice from the font name tables follows.

```text
Fonts are (c) Bitstream (see below). DejaVu changes are in public domain. Glyphs imported from Arev fonts are (c) Tavmjung Bah (see below)

Bitstream Vera Fonts Copyright
------------------------------

Copyright (c) 2003 by Bitstream, Inc. All Rights Reserved. Bitstream Vera is
a trademark of Bitstream, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of the fonts accompanying this license ("Fonts") and associated
documentation files (the "Font Software"), to reproduce and distribute the
Font Software, including without limitation the rights to use, copy, merge,
publish, distribute, and/or sell copies of the Font Software, and to permit
persons to whom the Font Software is furnished to do so, subject to the
following conditions:

The above copyright and trademark notices and this permission notice shall
be included in all copies of one or more of the Font Software typefaces.

The Font Software may be modified, altered, or added to, and in particular
the designs of glyphs or characters in the Fonts may be modified and
additional glyphs or characters may be added to the Fonts, only if the fonts
are renamed to names not containing either the words "Bitstream" or the word
"Vera".

This License becomes null and void to the extent applicable to Fonts or Font
Software that has been modified and is distributed under the "Bitstream
Vera" names.

The Font Software may be sold as part of a larger software package but no
copy of one or more of the Font Software typefaces may be sold by itself.

THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF COPYRIGHT, PATENT,
TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL BITSTREAM OR THE GNOME
FOUNDATION BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, INCLUDING
ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL DAMAGES,
WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF
THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER DEALINGS IN THE
FONT SOFTWARE.

Except as contained in this notice, the names of Gnome, the Gnome
Foundation, and Bitstream Inc., shall not be used in advertising or
otherwise to promote the sale, use or other dealings in this Font Software
without prior written authorization from the Gnome Foundation or Bitstream
Inc., respectively. For further information, contact: fonts at gnome dot
org. 

Arev Fonts Copyright
------------------------------

Copyright (c) 2006 by Tavmjong Bah. All Rights Reserved.

Permission is hereby granted, free of charge, to any person obtaining
a copy of the fonts accompanying this license ("Fonts") and
associated documentation files (the "Font Software"), to reproduce
and distribute the modifications to the Bitstream Vera Font Software,
including without limitation the rights to use, copy, merge, publish,
distribute, and/or sell copies of the Font Software, and to permit
persons to whom the Font Software is furnished to do so, subject to
the following conditions:

The above copyright and trademark notices and this permission notice
shall be included in all copies of one or more of the Font Software
typefaces.

The Font Software may be modified, altered, or added to, and in
particular the designs of glyphs or characters in the Fonts may be
modified and additional glyphs or characters may be added to the
Fonts, only if the fonts are renamed to names not containing either
the words "Tavmjong Bah" or the word "Arev".

This License becomes null and void to the extent applicable to Fonts
or Font Software that has been modified and is distributed under the 
"Tavmjong Bah Arev" names.

The Font Software may be sold as part of a larger software package but
no copy of one or more of the Font Software typefaces may be sold by
itself.

THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL
TAVMJONG BAH BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.

Except as contained in this notice, the name of Tavmjong Bah shall not
be used in advertising or otherwise to promote the sale, use or other
dealings in this Font Software without prior written authorization
from Tavmjong Bah. For further information, contact: tavmjong @ free
. fr.
```

The same notice is kept verbatim in
[`assets/fonts/DejaVu-LICENSE.txt`](assets/fonts/DejaVu-LICENSE.txt).

### egui-icons

`egui-icons.ttf` is the icon subset from `epaint_default_fonts` 0.37.0, from
John Slegers' Emoji Icon Font. It is MIT licensed.

```text
MIT License

Copyright (c) 2014 John Slegers

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

The same notice is kept verbatim in
[`assets/fonts/egui-icons-LICENSE.txt`](assets/fonts/egui-icons-LICENSE.txt).

### No Ubuntu Font Licence or Hack bytes

Earlier builds used eframe/epaint's `default_fonts` bundle, which contained
Ubuntu-Light and Hack. That bundle is disabled (`default_fonts` is off), and the
vendored DejaVu and egui-icons files above replace it. No Ubuntu Font Licence
(UFL-1.0), OFL, Ubuntu, or Hack bytes are compiled into the app or shipped in
the archives (ticket 67, commit `3c88e82`). The UFL-1.0/GPL-3.0 compatibility
question is therefore moot for this release.

## Rust crates

The full dependency graph is pinned by `Cargo.lock`. Direct dependencies, in
the licence groups used by the project:

| Licence | Crates |
| --- | --- |
| MIT OR Apache-2.0 (or `MIT/Apache-2.0`) | `eframe`, `egui`, `epaint`, `glam`, `serde`, `serde_json`, `image`, `log`, `rand`, `dirs-next`, `env_logger`, `getrandom`, `wasm-bindgen`, `wasm-bindgen-futures`, `js-sys`, `web-sys`, `web-time`, `console_log`, and the dev-dependency `tempfile` |
| Apache-2.0 | `colorous` |
| Zlib OR Apache-2.0 OR MIT | `bytemuck` |
| Unlicense OR MIT | `meval` |
| MIT | `rfd` |

`eframe`, `egui`, and `epaint` are the UI, window, and text stack. `colorous`
supplies the Viridis and Turbo colour ramps. `meval` is the formula evaluator.
`rfd` is the native screenshot save dialog. `image` encodes PNGs. `wasm-bindgen`,
`js-sys`, `web-sys`, `web-time`, `console_log`, and `getrandom` are the browser
platform seam; `dirs-next` and `env_logger` are native-only.

The remaining crates resolved by `Cargo.lock` are transitive dependencies and
are under permissive licences: MIT, Apache-2.0, Apache-2.0 WITH
LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, BSL-1.0, Unicode-3.0,
CC0-1.0, 0BSD, Unlicense, NCSA, or a dual/triple alternative containing one of
these. The only GPL-3.0-only package in the graph is this project; a small
number of crates offer LGPL-2.1-or-later as an alternative to MIT/Apache-2.0,
and the permissive option is used.

Notable transitive licence families are NCSA for `libfuzzer-sys` (whose
expression is `(MIT OR Apache-2.0) AND NCSA`), Apache-2.0 WITH LLVM-exception
for `linux-raw-sys`, `rustix`, `wasi`, `wasip2`, and `wit-bindgen` (each also
offers Apache-2.0 and/or MIT), and `Apache-2.0 OR GPL-2.0-only` for `self_cell`
(this project uses the Apache-2.0 option). Each crate's exact copyright line
and licence text are in its source distribution named by `Cargo.lock`; this
file does not reproduce those copyright lines.

The standard MIT permission text follows.

```text
MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The Apache License 2.0 text is available at
<https://www.apache.org/licenses/LICENSE-2.0>. Where a crate is dual licensed
MIT OR Apache-2.0, this project uses the MIT option unless the crate's own
notice says otherwise.

## System libraries and the release archives

- **Linux AppImage:** linuxdeploy bundles the non-graphics shared libraries that
  the release build links against (for example xkbcommon and other desktop
  dependencies). The packaging workflow derives the bundled list from the
  AppDir, resolves each library's Debian/Ubuntu package, and copies that
  package's copyright notice into
  `usr/share/doc/vector-field-explorer/system-libraries/` inside the AppImage,
  with a `SOURCES.txt` mapping each library to its package. The package step
  fails if a bundled library has no notice. The workflow deliberately excludes
  `libvulkan*`, `libGL*`, `libEGL*`, `libOpenGL*`, `libdrm*`, and `libgbm*`, so
  the host GPU driver and its licences stay on the host.
- **macOS:** the app links against the system frameworks supplied by macOS; no
  Apple framework binaries are redistributed in the `.app.zip`.
- The release archives include this file, `README.md`, and `LICENSE` alongside
  the application.
