#!/usr/bin/env python3
"""Rebuild the tintable inset masks (requires Pillow only when regenerating).

The 64px source has 28px corners and an 8px stretchable center, sampled at 4x.
InsetShadowLayer slices at 112 source pixels to preserve the corner curves.
"""
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

SCALE = 4
SIZE = 64 * SCALE
RADIUS = 28 * SCALE
PADDING = 16 * SCALE
DESTINATION = Path(__file__).resolve().parents[1] / "assets" / "effects"


def mask(offset: int, blur: int) -> Image.Image:
    size = SIZE + 2 * PADDING
    shape = Image.new("L", (size, size), 0)
    ImageDraw.Draw(shape).rounded_rectangle(
        (PADDING, PADDING, PADDING + SIZE - 1, PADDING + SIZE - 1),
        radius=RADIUS,
        fill=255,
    )
    outside = ImageChops.invert(shape)
    cast = Image.new("L", outside.size, 255)
    cast.paste(outside, (offset * SCALE, offset * SCALE))
    cast = cast.filter(ImageFilter.GaussianBlur(blur * SCALE))
    alpha = ImageChops.multiply(cast, shape)
    rgba = Image.new("RGBA", shape.size, (255, 255, 255, 0))
    rgba.putalpha(alpha)
    return rgba.crop((PADDING, PADDING, PADDING + SIZE, PADDING + SIZE))


def main() -> None:
    DESTINATION.mkdir(parents=True, exist_ok=True)
    mask(2, 3).save(DESTINATION / "inset-shade.png", optimize=True)
    mask(-2, 2).save(DESTINATION / "inset-highlight.png", optimize=True)


if __name__ == "__main__":
    main()
