from __future__ import annotations

import hashlib
import struct
import tempfile
import zlib
from collections.abc import Callable
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen

from iyw_sales_layout import sanitize_path_component
from iyw_sales_validation import ValidationError

MAX_IMAGE_BYTES = 25 * 1024 * 1024


class ImageDownloadError(OSError):
    pass


def _validated_url(value: object) -> str:
    text = str(value or "").strip()
    parsed = urlparse(text)
    if parsed.scheme != "https" or not parsed.hostname:
        raise ImageDownloadError("产品图片只允许 HTTPS 地址")
    return text


def _fetch_image(url: str) -> tuple[bytes, str, str]:
    source = _validated_url(url)
    try:
        request = Request(source, headers={"Accept": "image/*"})
        with urlopen(request, timeout=60) as response:
            final_url = _validated_url(response.geturl())
            content_type = response.headers.get_content_type().lower()
            length = response.headers.get("Content-Length")
            if length and int(length) > MAX_IMAGE_BYTES:
                raise ImageDownloadError("产品图片超过 25 MB 限制")
            data = response.read(MAX_IMAGE_BYTES + 1)
    except ImageDownloadError:
        raise
    except (HTTPError, URLError, TimeoutError, ValueError) as error:
        raise ImageDownloadError("产品图片下载失败") from error
    if len(data) > MAX_IMAGE_BYTES:
        raise ImageDownloadError("产品图片超过 25 MB 限制")
    return data, content_type, final_url


def _png_is_complete(data: bytes) -> bool:
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        return False
    position = 8
    header: bytes | None = None
    compressed = bytearray()
    found_end = False
    while position + 12 <= len(data):
        length = int.from_bytes(data[position : position + 4], "big")
        end = position + 12 + length
        if end > len(data):
            return False
        kind = data[position + 4 : position + 8]
        payload = data[position + 8 : position + 8 + length]
        expected_crc = int.from_bytes(data[end - 4 : end], "big")
        if zlib.crc32(kind + payload) & 0xFFFFFFFF != expected_crc:
            return False
        if kind == b"IHDR":
            header = payload
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            found_end = True
            break
        position = end
    if header is None or len(header) != 13 or not compressed or not found_end:
        return False
    width, height, depth, color, compression, filtering, interlace = struct.unpack(
        ">IIBBBBB", header
    )
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(color)
    if (
        not width
        or not height
        or channels is None
        or compression
        or filtering
        or interlace not in {0, 1}
    ):
        return False
    try:
        pixels = zlib.decompress(compressed)
    except zlib.error:
        return False
    row_bytes = (width * channels * depth + 7) // 8
    return bool(pixels) and (interlace == 1 or len(pixels) == height * (row_bytes + 1))


def _jpeg_is_complete(data: bytes) -> bool:
    if (
        len(data) < 12
        or not data.startswith(b"\xff\xd8")
        or not data.endswith(b"\xff\xd9")
    ):
        return False
    position = 2
    has_frame = False
    while position + 4 <= len(data) - 2:
        if data[position] != 0xFF:
            return False
        while position < len(data) and data[position] == 0xFF:
            position += 1
        if position >= len(data) - 2:
            return False
        marker = data[position]
        position += 1
        if marker in {0x01, *range(0xD0, 0xD8)}:
            continue
        if position + 2 > len(data) - 2:
            return False
        length = int.from_bytes(data[position : position + 2], "big")
        if length < 2 or position + length > len(data) - 2:
            return False
        if marker == 0xDA:
            return has_frame and position + length < len(data) - 2
        frame_markers = (
            set(range(0xC0, 0xC4))
            | set(range(0xC5, 0xC8))
            | set(range(0xC9, 0xCC))
            | set(range(0xCD, 0xD0))
        )
        if marker in frame_markers:
            has_frame = length >= 8 and any(
                data[position + offset] for offset in (3, 5)
            )
        position += length
    return False


def _gif_is_complete(data: bytes) -> bool:
    if len(data) < 14 or not data.startswith((b"GIF87a", b"GIF89a")):
        return False
    width, height = struct.unpack("<HH", data[6:10])
    return bool(width and height and data.rstrip(b"\x00").endswith(b";"))


def _webp_is_complete(data: bytes) -> bool:
    if len(data) < 20 or data[:4] != b"RIFF" or data[8:12] != b"WEBP":
        return False
    declared_size = int.from_bytes(data[4:8], "little") + 8
    return declared_size == len(data) and data[12:16] in {b"VP8 ", b"VP8L", b"VP8X"}


def _detected_suffix(data: bytes) -> str:
    signatures = (
        (_png_is_complete(data), ".png"),
        (_jpeg_is_complete(data), ".jpg"),
        (_gif_is_complete(data), ".gif"),
        (_webp_is_complete(data), ".webp"),
    )
    for matched, suffix in signatures:
        if matched:
            return suffix
    raise ImageDownloadError("下载结果不是支持的产品图片格式")


def is_supported_image(path: str | Path) -> bool:
    candidate = Path(path)
    if not candidate.is_file():
        return False
    try:
        with candidate.open("rb") as stream:
            data = stream.read(MAX_IMAGE_BYTES + 1)
        if len(data) > MAX_IMAGE_BYTES:
            return False
        _detected_suffix(data)
    except (ImageDownloadError, OSError):
        return False
    return True


def _target_path(
    output: Path, index: int, product: dict[str, Any], suffix: str
) -> Path:
    name = sanitize_path_component(str(product.get("name") or "产品"), "产品")
    return output / f"{index:02d}-{name}{suffix}"


def _write_image(target: Path, data: bytes, *, force: bool) -> None:
    if target.exists() and not force:
        raise ImageDownloadError("目标产品图片已存在")
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(delete=False, dir=target.parent) as stream:
            stream.write(data)
            temporary = Path(stream.name)
        temporary.replace(target)
    finally:
        if temporary:
            temporary.unlink(missing_ok=True)


def _receipt(source: str, resolved: str, data: bytes) -> dict[str, str]:
    return {
        "source_url": _validated_url(source),
        "resolved_url": _validated_url(resolved),
        "sha256": hashlib.sha256(data).hexdigest(),
    }


def _existing_receipt_valid(product: dict[str, Any], path: Path) -> bool:
    receipt = product.get("download_receipt")
    source = str(product.get("image_url") or "").strip()
    if not product.get("local_path") or not is_supported_image(path):
        return False
    if not isinstance(receipt, dict) or receipt.get("source_url") != source:
        return False
    try:
        rebuilt = _receipt(
            source, str(receipt.get("resolved_url") or ""), path.read_bytes()
        )
    except (ImageDownloadError, OSError):
        return False
    return rebuilt == receipt


def _validate_download_input(products: object, limit: int) -> list[dict[str, Any]]:
    if not isinstance(products, list) or any(
        not isinstance(item, dict) for item in products
    ):
        raise ValidationError("products must be an array of objects")
    if limit < 1:
        raise ValidationError("limit must be positive")
    return [dict(item) for item in products]


def download_product_images(
    products: object,
    output_dir: str | Path,
    *,
    limit: int = 10,
    force: bool = False,
    fetcher: Callable[[str], tuple[bytes, str, str]] = _fetch_image,
) -> dict[str, object]:
    updated = _validate_download_input(products, limit)
    output = Path(output_dir)
    output.mkdir(parents=True, exist_ok=True)
    saved: list[str] = []
    errors: list[dict[str, object]] = []
    download_index = 0
    seen_urls: set[str] = set()
    for item_index, product in enumerate(updated, 1):
        existing = Path(str(product.get("local_path") or ""))
        if _existing_receipt_valid(product, existing):
            saved.append(str(existing))
            continue
        image_url = str(product.get("image_url") or "").strip()
        if not image_url or image_url in seen_urls or download_index >= limit:
            continue
        seen_urls.add(image_url)
        download_index += 1
        try:
            source = _validated_url(image_url)
            data, _, final_url = fetcher(source)
            target = _target_path(
                output, download_index, product, _detected_suffix(data)
            )
            _write_image(target, data, force=force)
            product["local_path"] = str(target)
            product["download_receipt"] = _receipt(source, final_url, data)
            saved.append(str(target))
        except ImageDownloadError as error:
            errors.append(
                {
                    "index": item_index,
                    "name": product.get("name"),
                    "message": str(error),
                }
            )
    return {"products": updated, "saved_paths": saved, "errors": errors}
