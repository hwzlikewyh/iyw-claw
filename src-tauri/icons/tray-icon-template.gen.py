"""Generate src-tauri/icons/tray-icon-template.png.

This is a macOS menu-bar template image: the system reads only the alpha
channel and tints the opaque pixels to match light/dark/highlighted menu
bar appearance. Re-run after editing the constants below:

    python3 src-tauri/icons/tray-icon-template.gen.py

Requires Pillow (pip install Pillow).
"""

from pathlib import Path
from PIL import Image, ImageDraw, ImageChops


W_LOGICAL, H_LOGICAL = 44, 44

# Render at 4x then downsample with Lanczos: PIL's drawing primitives
# are not anti-aliased, so supersampling is what produces smooth
# diagonals and round caps.
SCALE = 4
W, H = W_LOGICAL * SCALE, H_LOGICAL * SCALE

def _box(left, top, right, bottom):
    return tuple(value * SCALE for value in (left, top, right, bottom))


def main():
    alpha = Image.new("L", (W, H), 0)
    draw = ImageDraw.Draw(alpha)
    draw.arc(_box(4, 3, 40, 38), 185, 355, fill=255, width=3 * SCALE)
    draw.rounded_rectangle(_box(3, 17, 10, 31), 3 * SCALE, fill=255)
    draw.rounded_rectangle(_box(34, 17, 41, 31), 3 * SCALE, fill=255)
    draw.ellipse(_box(7, 8, 37, 39), fill=255)
    draw.polygon(
        [(15 * SCALE, 10 * SCALE), (18 * SCALE, 3 * SCALE), (22 * SCALE, 10 * SCALE)],
        fill=255,
    )
    draw.polygon(
        [(20 * SCALE, 10 * SCALE), (25 * SCALE, 5 * SCALE), (27 * SCALE, 13 * SCALE)],
        fill=255,
    )

    cut = Image.new("L", (W, H), 0)
    dc = ImageDraw.Draw(cut)
    dc.ellipse(_box(13, 17, 17, 22), fill=255)
    dc.ellipse(_box(27, 17, 31, 22), fill=255)
    dc.ellipse(_box(13, 24, 31, 33), outline=255, width=2 * SCALE)
    dc.line(_box(16, 28, 28, 28), fill=255, width=SCALE)

    alpha = ImageChops.subtract(alpha, cut)
    zero = Image.new("L", (W, H), 0)
    img = Image.merge("RGBA", (zero, zero, zero, alpha)).resize(
        (W_LOGICAL, H_LOGICAL), Image.LANCZOS
    )

    out = Path(__file__).parent / "tray-icon-template.png"
    img.save(out)
    print(f"wrote {out} {img.size}")


if __name__ == "__main__":
    main()
