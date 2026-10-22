#!/usr/bin/env python3
"""Render assets/smolcast.svg into assets/smolcast.ico and assets/smolcast.png.

Maintainer-only: the app never runs this, and the generated files are
committed. build.rs embeds the .ico in the exe (Explorer, Properties, the
taskbar and the tray all read it from there). Needs cairosvg and Pillow.

The SVG is the source of truth: "s." on a dark squircle (DESIGN.md "App
icon"). At 24 px and below the hairline rim is dropped and the mark is drawn
larger, so the tray and small Explorer views stay legible.

Usage: python3 scripts/generate_icon.py [--assets assets]
"""

import argparse
import io
import pathlib
import re

import cairosvg
from PIL import Image

# Windows picks from these: 16-40 are the small (tray) icon at 100-250% scale
# in 25% steps, 32/48 shell icons, 64 and 256 large views and Properties.
SIZES = [16, 20, 24, 28, 32, 36, 40, 48, 64, 256]
SMALL = 24
SMALL_MARK_SCALE = 1.22


def small_variant(svg):
    """The SVG without its rim and with the mark scaled about the centre."""
    svg, rims = re.subn(r'\s*<path id="rim"[^>]*/>', "", svg)
    k = SMALL_MARK_SCALE
    mark = '<g id="mark">'
    if rims != 1 or svg.count(mark) != 1:
        raise SystemExit('smolcast.svg needs one <path id="rim" .../> and one <g id="mark">')
    return svg.replace(
        mark,
        f'<g id="mark" transform="translate(128 128) scale({k}) translate(-128 -128)">',
    )


def render(svg, size):
    png = cairosvg.svg2png(
        bytestring=svg.encode(), output_width=size, output_height=size
    )
    return Image.open(io.BytesIO(png)).convert("RGBA")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--assets", default="assets", type=pathlib.Path)
    args = parser.parse_args()

    svg = (args.assets / "smolcast.svg").read_text()
    small = small_variant(svg)
    frames = [render(small if size <= SMALL else svg, size) for size in SIZES]

    largest = frames[-1]
    largest.save(
        args.assets / "smolcast.ico",
        format="ICO",
        sizes=[(f.width, f.height) for f in frames],
        append_images=frames[:-1],
    )
    largest.save(args.assets / "smolcast.png", optimize=True)

    # Older Pillow ignores append_images for ICO and resizes the largest
    # frame instead, which would lose the small variant.
    with Image.open(args.assets / "smolcast.ico") as ico:
        ico.size = (SMALL, SMALL)
        if ico.convert("RGBA").tobytes() != frames[SIZES.index(SMALL)].tobytes():
            raise SystemExit("this Pillow wrote resized frames; upgrade Pillow")


if __name__ == "__main__":
    main()
