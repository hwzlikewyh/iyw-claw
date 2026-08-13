"""Generate the macOS template from the current application icon."""

from pathlib import Path
from PIL import Image


W_LOGICAL, H_LOGICAL = 44, 44


def main():
    source = Image.open(Path(__file__).parent / "icon.png").convert("RGBA")
    source = source.resize((W_LOGICAL, H_LOGICAL), Image.Resampling.LANCZOS)
    red = source.getchannel("R")
    alpha = red.point(lambda value: max(0, min(255, (value - 80) * 255 // 175)))
    zero = Image.new("L", (W_LOGICAL, H_LOGICAL), 0)
    img = Image.merge("RGBA", (zero, zero, zero, alpha))

    out = Path(__file__).parent / "tray-icon-template.png"
    img.save(out)
    print(f"wrote {out} {img.size}")


if __name__ == "__main__":
    main()
