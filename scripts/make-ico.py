#!/usr/bin/env python3
"""Pack the committed PNG icons into a Windows icon (.ico).

Mirrors scripts/make-icns.py for the Windows target. The output is committed;
CI never regenerates it. Generate with:

    scripts/gen-icons.sh && scripts/make-ico.py

Verify with `file packaging/windows/vector-field-explorer.ico`.
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "packaging" / "icons"
OUT = ROOT / "packaging" / "windows" / "vector-field-explorer.ico"

# Standard Windows icon sizes. 256 is stored as a PNG inside the .ico container.
SIZES = [16, 24, 32, 48, 64, 128, 256]


def png_path(size: int) -> Path:
    # 24 is not in the committed set; fall back to 32 and let Pillow rescale.
    source = size if size in (16, 32, 48, 64, 128, 256) else 32
    path = ICONS / f"vector-field-explorer-{source}.png"
    if not path.is_file():
        sys.exit(f"make-ico: missing {path}; run scripts/gen-icons.sh first")
    return path


def main() -> None:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    # Pillow generates every requested size from the largest source image, so
    # feed it the 256 px render and ask for the full size set.
    with Image.open(png_path(256)) as src:
        src = src.convert("RGBA")
        src.save(OUT, format="ICO", sizes=[(s, s) for s in SIZES])
    print(f"make-ico: wrote {OUT} ({OUT.stat().st_size} bytes, {len(SIZES)} sizes)")


if __name__ == "__main__":
    main()
