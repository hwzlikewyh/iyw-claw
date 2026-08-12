"""Compose checked local images into a deterministic grid layout."""

from __future__ import annotations

import re
import uuid
from pathlib import Path
from typing import Any

from PIL import Image, ImageOps, UnidentifiedImageError

from iyw_image import IywError


SUPPORTED_INPUT_SUFFIXES = frozenset({".png", ".jpg", ".jpeg", ".webp"})
OUTPUT_FORMATS = {
    ".png": "PNG",
    ".jpg": "JPEG",
    ".jpeg": "JPEG",
    ".webp": "WEBP",
}
COLOR_PATTERN = re.compile(r"^#[0-9A-Fa-f]{6}$")


def _validate_layout(
    images: list[Path], rows: int, columns: int, gap: int
) -> list[Path]:
    if rows <= 0 or columns <= 0:
        raise IywError("rows and columns must be positive", "invalid_input")
    if gap < 0:
        raise IywError("gap must be nonnegative", "invalid_input")
    expected = rows * columns
    if len(images) != expected:
        raise IywError(f"layout requires exactly {expected} images", "invalid_input")
    paths = [Path(path).resolve() for path in images]
    for path in paths:
        if not path.is_file():
            raise IywError(f"input image not found: {path}", "invalid_input")
        if path.suffix.lower() not in SUPPORTED_INPUT_SUFFIXES:
            raise IywError(f"unsupported input image extension: {path.suffix}", "invalid_input")
    return paths


def _validate_output(out: Path, background: str, force: bool) -> tuple[Path, str]:
    path = Path(out).resolve()
    image_format = OUTPUT_FORMATS.get(path.suffix.lower())
    if image_format is None:
        raise IywError("unsupported output extension", "invalid_input")
    if not COLOR_PATTERN.fullmatch(background):
        raise IywError("background must use #RRGGBB", "invalid_input")
    if not path.parent.is_dir():
        raise IywError("output directory does not exist", "invalid_input")
    if path.exists() and not force:
        raise IywError("output file already exists; use --force", "invalid_input")
    return path, image_format


def _load_images(paths: list[Path]) -> list[Image.Image]:
    loaded = []
    try:
        for path in paths:
            with Image.open(path) as source:
                source.load()
                loaded.append(ImageOps.exif_transpose(source).convert("RGBA"))
    except (OSError, ValueError, UnidentifiedImageError) as exc:
        for image in loaded:
            image.close()
        raise IywError(f"could not decode input image: {path}", "invalid_input") from exc
    return loaded


def _hex_color(value: str) -> tuple[int, int, int]:
    return tuple(int(value[index : index + 2], 16) for index in (1, 3, 5))


def _compose_canvas(
    images: list[Image.Image], rows: int, columns: int, gap: int, background: str
) -> Image.Image:
    cell_width = max(image.width for image in images)
    cell_height = max(image.height for image in images)
    width = columns * cell_width + (columns - 1) * gap
    height = rows * cell_height + (rows - 1) * gap
    color = _hex_color(background)
    canvas = Image.new("RGB", (width, height), color)
    for index, image in enumerate(images):
        fitted = ImageOps.contain(
            image, (cell_width, cell_height), Image.Resampling.LANCZOS
        )
        row, column = divmod(index, columns)
        x = column * (cell_width + gap) + (cell_width - fitted.width) // 2
        y = row * (cell_height + gap) + (cell_height - fitted.height) // 2
        cell = Image.new("RGB", fitted.size, color)
        cell.paste(fitted, mask=fitted.getchannel("A"))
        canvas.paste(cell, (x, y))
        fitted.close()
        cell.close()
    return canvas


def _save_atomic(canvas: Image.Image, out: Path, image_format: str) -> None:
    temp = out.with_name(f".{out.stem}.{uuid.uuid4().hex}{out.suffix}")
    try:
        save_options = {"quality": 95} if image_format in {"JPEG", "WEBP"} else {}
        canvas.save(temp, format=image_format, **save_options)
        temp.replace(out)
    except Exception as exc:
        if temp.exists():
            temp.unlink()
        raise IywError(f"could not write output image: {out}", "invalid_input") from exc


def compose_layout(
    images: list[Path],
    rows: int,
    columns: int,
    out: Path,
    *,
    gap: int = 0,
    background: str = "#FFFFFF",
    force: bool = False,
) -> dict[str, Any]:
    paths = _validate_layout(images, rows, columns, gap)
    output, image_format = _validate_output(out, background, force)
    loaded = _load_images(paths)
    canvas = None
    try:
        canvas = _compose_canvas(loaded, rows, columns, gap, background)
        _save_atomic(canvas, output, image_format)
        width, height = canvas.size
    finally:
        if canvas is not None:
            canvas.close()
        for image in loaded:
            image.close()
    return {
        "out": str(output),
        "width": width,
        "height": height,
        "rows": rows,
        "columns": columns,
        "count": len(paths),
    }
