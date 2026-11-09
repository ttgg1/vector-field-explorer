#!/usr/bin/env python3
"""Pack the committed PNG icons into an Apple Icon Image (.icns).

Linux packaging has neither `iconutil` nor `png2icns`, so this script writes the
container directly. Each element is ``type + big-endian length + payload``;
modern macOS accepts PNG payloads in the ``icp4``/``icp5``/``icp6`` and
``ic07``-``ic14`` slots. We emit the 1x slots plus the retina aliases so Finder,
the Dock and the app switcher all find a native size.

Run `scripts/gen-icons.sh` first, then this script. The output is committed; CI
never regenerates it. Verify with `file packaging/icons/vector-field-explorer.icns`
and, on macOS, `sips -g pixelWidth packaging/icons/vector-field-explorer.icns`.
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "packaging" / "icons"
OUT = ICONS / "vector-field-explorer.icns"

# (element type, source size, role). icp4/5/6 are the legacy small slots;
# ic07-ic09 are the modern 128/256/512 PNG slots; ic11-ic14 are their retina
# aliases (16@2x, 32@2x, 128@2x, 256@2x).
ELEMENTS: list[tuple[bytes, int, str]] = [
    (b"icp4", 16, "16x16"),
    (b"icp5", 32, "32x32"),
    (b"icp6", 64, "64x64"),
    (b"ic07", 128, "128x128"),
    (b"ic08", 256, "256x256"),
    (b"ic09", 512, "512x512"),
    (b"ic11", 32, "16x16@2x"),
    (b"ic12", 64, "32x32@2x"),
    (b"ic13", 256, "128x128@2x"),
    (b"ic14", 512, "256x256@2x"),
]


def png_bytes(size: int) -> bytes:
    path = ICONS / f"vector-field-explorer-{size}.png"
    if not path.is_file():
        sys.exit(f"make-icns: missing {path}; run scripts/gen-icons.sh first")
    data = path.read_bytes()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        sys.exit(f"make-icns: {path} is not a PNG")
    return data


def main() -> None:
    chunks: list[bytes] = []
    for kind, size, role in ELEMENTS:
        payload = png_bytes(size)
        length = 8 + len(payload)
        if length > 0xFFFFFFFF:
            sys.exit(f"make-icns: {kind!r} ({role}) is too large for an ICNS element")
        chunks.append(kind + struct.pack(">I", length) + payload)

    body = b"".join(chunks)
    total = 8 + len(body)
    if total > 0xFFFFFFFF:
        sys.exit("make-icns: the icon set exceeds the ICNS 32-bit length field")

    OUT.write_bytes(b"icns" + struct.pack(">I", total) + body)
    print(f"make-icns: wrote {OUT} ({total} bytes, {len(ELEMENTS)} elements)")
    for kind, _, role in ELEMENTS:
        print(f"  {kind.decode('ascii')}  {role}")


if __name__ == "__main__":
    main()
