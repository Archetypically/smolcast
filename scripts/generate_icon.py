#!/usr/bin/env python3
"""Render assets/smolcast.svg into assets/smolcast.ico and assets/smolcast.png.

Maintainer-only: the app never runs this, and the generated files are
committed. build.rs embeds the .ico in the exe (Explorer, Properties, the
taskbar and the tray all read it from there). Needs cairosvg and Pillow.

The SVG is the source of truth: "s." on a dark squircle (DESIGN.md "App
icon"). At 24 px and below the hairline rim is dropped and the mark is drawn
larger, so the tray and small Explorer views stay legible.

It also renders the MSIX package's logos into packaging/msix/Assets (plan
§15), in the scale and target-size variants makepri indexes: the app list
and taskbar icon (Square44x44Logo, also unplated), the Start tile
(Square150x150Logo, the squircle at two thirds of the tile) and StoreLogo.

Usage: python3 scripts/generate_icon.py [--assets assets] [--msix packaging/msix/Assets]
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

# MSIX logos: base size at 100% and the scales Windows asks for.
SCALES = [100, 125, 150, 200, 400]
MSIX_LOGOS = {"Square44x44Logo": 44, "Square150x150Logo": 150, "StoreLogo": 50}
# Square44x44Logo at exact pixel sizes, for the taskbar, Start's app list
# and Explorer; the unplated copies are drawn without a tile behind them.
TARGET_SIZES = [16, 24, 32, 48, 256]
# How much of the Start tile the squircle covers.
TILE_FILL = 2 / 3


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


def icon_at(svg, small, size):
    """The icon drawn at `size`: the small variant at tray sizes."""
    return render(small if size <= SMALL else svg, size)


def padded(image, size, fill):
    """`image` scaled to `fill` of a transparent `size` square, centred."""
    inner = round(size * fill)
    canvas = Image.new("RGBA", (size, size))
    offset = (size - inner) // 2
    canvas.alpha_composite(image.resize((inner, inner), Image.LANCZOS), (offset, offset))
    return canvas


def msix_logos(svg, small, out):
    """The package logos, named as makepri expects its qualifiers."""
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("*.png"):
        old.unlink()
    for name, base in MSIX_LOGOS.items():
        for scale in SCALES:
            size = base * scale // 100
            if name == "Square150x150Logo":
                image = padded(render(svg, size * 4), size, TILE_FILL)
            else:
                image = icon_at(svg, small, size)
            image.save(out / f"{name}.scale-{scale}.png", optimize=True)
    for size in TARGET_SIZES:
        image = icon_at(svg, small, size)
        for suffix in ("", "_altform-unplated"):
            image.save(out / f"Square44x44Logo.targetsize-{size}{suffix}.png", optimize=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--assets", default="assets", type=pathlib.Path)
    parser.add_argument("--msix", default="packaging/msix/Assets", type=pathlib.Path)
    args = parser.parse_args()

    svg = (args.assets / "smolcast.svg").read_text()
    small = small_variant(svg)
    msix_logos(svg, small, args.msix)
    frames = [icon_at(svg, small, size) for size in SIZES]

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
