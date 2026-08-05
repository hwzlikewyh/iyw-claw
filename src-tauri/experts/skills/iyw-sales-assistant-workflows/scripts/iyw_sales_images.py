from __future__ import annotations

import tempfile
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


def _detected_suffix(data: bytes) -> str:
    signatures = (
        (data.startswith(b"\x89PNG\r\n\x1a\n"), ".png"),
        (data.startswith(b"\xff\xd8\xff"), ".jpg"),
        (data.startswith((b"GIF87a", b"GIF89a")), ".gif"),
        (len(data) >= 12 and data[:4] == b"RIFF" and data[8:12] == b"WEBP", ".webp"),
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
            _detected_suffix(stream.read(12))
    except (ImageDownloadError, OSError):
        return False
    return True


def _target_path(output: Path, index: int, product: dict[str, Any], suffix: str) -> Path:
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


def download_product_images(
    products: object,
    output_dir: str | Path,
    *,
    limit: int = 10,
    force: bool = False,
    fetcher: Callable[[str], tuple[bytes, str, str]] = _fetch_image,
) -> dict[str, object]:
    if not isinstance(products, list) or any(not isinstance(item, dict) for item in products):
        raise ValidationError("products must be an array of objects")
    if limit < 1:
        raise ValidationError("limit must be positive")
    output = Path(output_dir)
    output.mkdir(parents=True, exist_ok=True)
    updated = [dict(item) for item in products]
    saved: list[str] = []
    errors: list[dict[str, object]] = []
    download_index = 0
    seen_urls: set[str] = set()
    for item_index, product in enumerate(updated, 1):
        existing = Path(str(product.get("local_path") or ""))
        if product.get("local_path") and is_supported_image(existing):
            saved.append(str(existing))
            continue
        image_url = str(product.get("image_url") or "").strip()
        if not image_url or image_url in seen_urls or download_index >= limit:
            continue
        seen_urls.add(image_url)
        download_index += 1
        try:
            source = _validated_url(image_url)
            data, _, _ = fetcher(source)
            target = _target_path(output, download_index, product, _detected_suffix(data))
            _write_image(target, data, force=force)
            product["local_path"] = str(target)
            saved.append(str(target))
        except ImageDownloadError as error:
            errors.append({"index": item_index, "name": product.get("name"), "message": str(error)})
    return {"products": updated, "saved_paths": saved, "errors": errors}
