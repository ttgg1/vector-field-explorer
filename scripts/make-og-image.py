#!/usr/bin/env python3
"""Compose the 1280x640 Open Graph / Twitter card from the project mark.

The card is committed as web/og-image.png; CI never regenerates it. It uses the
same mark, wordmark, tagline and brand chrome as the app, plus the shipped
DejaVu Sans face (assets/fonts/DejaVuSans.ttf) for text.

Run scripts/gen-icons.sh first (it produces the 256 px mark), then this script.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
MARK = ROOT / "packaging" / "icons" / "vector-field-explorer-256.png"
FONT = ROOT / "assets" / "fonts" / "DejaVuSans.ttf"
OUT = ROOT / "web" / "og-image.png"

WIDTH, HEIGHT = 1280, 640
NAME = "Vector Field Explorer"
TAGLINE = "A formula-driven vector-field explorer for 2D and 3D fields."
SITE = "ttgg1.github.io/vector-field-explorer"

BG = "#101a26"
BORDER = "#22313f"
INK = "#f4f7fb"
MUTED = "#a9b7c6"
ACCENT = "#4cc9f0"


def face(size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(FONT), size)


def fit(draw: ImageDraw.ImageDraw, text: str, max_width: float, start: int, floor: int) -> ImageFont.FreeTypeFont:
    for size in range(start, floor - 1, -1):
        font = face(size)
        if draw.textlength(text, font=font) <= max_width:
            return font
    return face(floor)


def wrap(draw: ImageDraw.ImageDraw, text: str, font: ImageFont.FreeTypeFont, max_width: float) -> list[str]:
    lines: list[str] = []
    line = ""
    for word in text.split():
        candidate = word if not line else f"{line} {word}"
        if draw.textlength(candidate, font=font) <= max_width:
            line = candidate
        else:
            if line:
                lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines


def main() -> None:
    if not MARK.is_file():
        raise SystemExit(f"make-og-image: missing {MARK}; run scripts/gen-icons.sh first")
    if not FONT.is_file():
        raise SystemExit(f"make-og-image: missing font {FONT}")

    image = Image.new("RGB", (WIDTH, HEIGHT), BG)
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle(
        [24, 24, WIDTH - 25, HEIGHT - 25],
        radius=28,
        outline=BORDER,
        width=2,
    )

    mark = Image.open(MARK).convert("RGBA").resize((320, 320), Image.Resampling.LANCZOS)
    image.paste(mark, (96, (HEIGHT - 320) // 2), mark)

    text_x = 480
    text_width = WIDTH - text_x - 80
    name_font = fit(draw, NAME, text_width, start=68, floor=44)
    draw.text((text_x, 196), NAME, font=name_font, fill=INK)

    rule_y = 196 + name_font.size + 22
    draw.rounded_rectangle([text_x, rule_y, text_x + 96, rule_y + 5], radius=2, fill=ACCENT)

    tagline_font = face(32)
    y = rule_y + 30
    for line in wrap(draw, TAGLINE, tagline_font, text_width):
        draw.text((text_x, y), line, font=tagline_font, fill=MUTED)
        y += tagline_font.size + 10

    draw.text((text_x, y + 22), SITE, font=face(24), fill=ACCENT)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    image.save(OUT, "PNG", optimize=True)
    print(f"make-og-image: wrote {OUT} ({WIDTH}x{HEIGHT})")


if __name__ == "__main__":
    main()
