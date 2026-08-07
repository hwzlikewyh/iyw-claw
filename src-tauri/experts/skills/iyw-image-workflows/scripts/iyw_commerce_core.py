"""Core HTTP operations for the IYW commerce image CLI."""

from __future__ import annotations

import asyncio
import mimetypes
import re
import time
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qsl, urlsplit, urlunsplit
from urllib.request import Request, urlopen

from iyw_image import IywClient, IywError, PROCESS_STATUSES, _public_result


OPERATION_PATTERN = re.compile(r"^[A-Za-z][A-Za-z0-9_]*$")
SUPPORTED_SUFFIXES = frozenset({".png", ".jpg", ".jpeg", ".webp"})
DESTRUCTIVE_OPERATIONS = frozenset({"removeTaskOrImage"})
SENSITIVE_PAYLOAD_KEY = re.compile(
    r"(?:authorization|cookie|credential|password|secret|securitykey|signature|signed|token)",
    re.IGNORECASE,
)
SENSITIVE_URL_QUERY = re.compile(
    r"(?:accesskey|credential|expires?|policy|securitytoken|sign|signature|token)",
    re.IGNORECASE,
)
IMAGE_PAYLOAD_KEYS = frozenset(
    {
        "cover",
        "image",
        "images",
        "imageurl",
        "imageurls",
        "img",
        "mask",
        "reference",
        "styleimg",
    }
)


def _require_https(url: str, label: str) -> str:
    value = url.strip()
    if not value.startswith("https://"):
        raise IywError(f"{label} must use HTTPS", "invalid_input")
    return value


def _validate_payload_safety(
    value: Any, path: str = "payload", *, image_field: bool = False
) -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).replace("_", "").replace("-", "").lower()
            if SENSITIVE_PAYLOAD_KEY.search(normalized):
                raise IywError(
                    f"{path} contains a sensitive field: {key}", "invalid_input"
                )
            is_image = normalized in IMAGE_PAYLOAD_KEYS or normalized.endswith("imageurl")
            _validate_payload_safety(item, f"{path}.{key}", image_field=is_image)
        return
    if isinstance(value, list):
        for index, item in enumerate(value):
            _validate_payload_safety(
                item, f"{path}[{index}]", image_field=image_field
            )
        return
    if not isinstance(value, str):
        return
    if image_field:
        _require_https(value, f"{path} URL")
    if value.startswith("https://"):
        query = parse_qsl(urlsplit(value).query, keep_blank_values=True)
        if any(SENSITIVE_URL_QUERY.search(key) for key, _ in query):
            raise IywError(f"{path} must not contain a signed URL", "invalid_input")


def _make_object_key(file_path: Path) -> str:
    suffix = file_path.suffix.lower()
    if suffix not in SUPPORTED_SUFFIXES:
        allowed = ", ".join(sorted(SUPPORTED_SUFFIXES))
        raise IywError(f"image extension must be one of: {allowed}", "invalid_input")
    date = datetime.now().strftime("%y%m%d")
    return f"AI/img/{date}/{uuid.uuid4().hex}{suffix}"


def _validate_object_key(value: str) -> str:
    key = value.strip().replace("\\", "/")
    if not key.startswith("AI/img/") or ".." in key.split("/"):
        raise IywError("object key must stay under AI/img/", "invalid_input")
    if Path(key).suffix.lower() not in SUPPORTED_SUFFIXES:
        raise IywError(
            "object key must end with a supported image extension", "invalid_input"
        )
    return key


def _public_url(signed_url: str) -> str:
    parts = urlsplit(_require_https(signed_url, "signed upload URL"))
    return urlunsplit((parts.scheme, parts.netloc, parts.path, "", ""))


def _put_file_sync(signed_url: str, file_path: Path, timeout: float) -> None:
    content_type = mimetypes.guess_type(file_path.name)[0] or "application/octet-stream"
    request = Request(
        _require_https(signed_url, "signed upload URL"),
        data=file_path.read_bytes(),
        method="PUT",
        headers={"Content-Type": content_type},
    )
    try:
        with urlopen(request, timeout=timeout) as response:
            if not 200 <= response.status < 300:
                raise IywError("object storage rejected the upload", "upload_failed")
    except HTTPError as exc:
        raise IywError(
            f"image upload failed with HTTP {exc.code}", "upload_failed"
        ) from exc
    except (URLError, TimeoutError) as exc:
        raise IywError(
            f"image upload failed: {exc}", "upstream_unavailable", True
        ) from exc


async def _put_file(signed_url: str, file_path: Path, timeout: float) -> None:
    await asyncio.to_thread(_put_file_sync, signed_url, file_path, timeout)


async def check_image(
    client: IywClient, image_url: str, *, dry_run: bool = False
) -> dict[str, Any]:
    public_url = _require_https(image_url, "image URL")
    checked = await client.request(
        "api/microModel/checkImage",
        {"image": public_url},
        dry_run=dry_run,
    )
    return checked if dry_run else {"image_url": public_url, "checked": True}


async def upload_and_check(
    client: IywClient,
    file_path: Path,
    *,
    object_key: str | None = None,
    timeout: float = 300,
    dry_run: bool = False,
) -> dict[str, Any]:
    path = file_path.resolve()
    if not path.is_file():
        raise IywError(f"image file not found: {file_path}", "invalid_input")
    key = _validate_object_key(object_key) if object_key else _make_object_key(path)
    presigned = await client.request(
        "api/microModel/PreSignedUrl",
        {"objectKey": key},
        dry_run=dry_run,
    )
    if dry_run:
        return {
            "object_key": key,
            "presign_request": presigned,
            "next_steps": ["PUT returned signed URL", "check uploaded public URL"],
        }
    signed_url = str(presigned.get("value") or presigned.get("url") or "")
    if not signed_url:
        raise IywError(
            "presign response did not include an upload URL", "upload_failed"
        )
    await _put_file(signed_url, path, timeout)
    public_url = _public_url(signed_url)
    await check_image(client, public_url)
    return {"image_url": public_url, "object_key": key, "checked": True}


def _validate_generate_payload(payload: dict[str, Any]) -> None:
    tool_name = payload.get("toolName")
    image_urls = payload.get("imageUrls")
    if tool_name == "mix":
        if not isinstance(image_urls, list) or not 2 <= len(image_urls) <= 10:
            raise IywError("mix requires 2 to 10 image URLs", "invalid_input")
        for image_url in image_urls:
            _require_https(str(image_url), "mix image URL")
    elif tool_name in {"variation", "extend"}:
        if not isinstance(image_urls, str):
            raise IywError(f"{tool_name} requires one image URL", "invalid_input")
        _require_https(image_urls, f"{tool_name} image URL")
    elif tool_name in {"iyw_tu", "iyw_ip", "user_product", "seperate_layers", "extract_pattern", "return_leftright", "color_transfer"}:
        if isinstance(image_urls, str):
            _require_https(image_urls, "image URL")
        elif isinstance(image_urls, list):
            for image_url in image_urls:
                _require_https(str(image_url), "image URL")
        else:
            raise IywError(f"{tool_name} requires image URLs", "invalid_input")
    else:
        raise IywError("unsupported g_tools_generate_image toolName", "invalid_input")


def _validate_upscale_payload(payload: dict[str, Any]) -> None:
    scale = payload.get("scale")
    if isinstance(scale, bool) or not isinstance(scale, int) or not 1 <= scale <= 8:
        raise IywError(
            "upscaleImage scale must be an integer from 1 to 8", "invalid_input"
        )
    _require_https(str(payload.get("image") or ""), "upscale image URL")
    if payload.get("providerId") not in {None, 0}:
        if not all(
            isinstance(payload.get(key), int) and payload[key] > 0
            for key in ("width", "height")
        ):
            raise IywError(
                "nonzero providerId requires positive width and height", "invalid_input"
            )


def _validate_known_payload(operation: str, payload: dict[str, Any]) -> None:
    if operation == "g_tools_generate_image":
        _validate_generate_payload(payload)
    elif operation == "upscaleImage":
        _validate_upscale_payload(payload)


async def invoke_operation(
    client: IywClient,
    operation: str,
    payload: dict[str, Any],
    *,
    confirm_destructive: bool = False,
    dry_run: bool = False,
) -> dict[str, Any]:
    if not OPERATION_PATTERN.fullmatch(operation):
        raise IywError("commerce operation name is invalid", "invalid_input")
    if operation in DESTRUCTIVE_OPERATIONS and not confirm_destructive:
        raise IywError(
            "destructive operation requires --confirm-destructive", "invalid_input"
        )
    _validate_payload_safety(payload)
    _validate_known_payload(operation, payload)
    result = await client.request(f"api/commerce/{operation}", payload, dry_run=dry_run)
    return result if dry_run else _public_result(result)


def normalize_commerce_task(
    data: dict[str, Any], fallback_id: str = ""
) -> dict[str, Any]:
    nested = data.get("data") if isinstance(data.get("data"), dict) else data
    task_id = nested.get("taskId") or nested.get("task_id") or fallback_id
    status = nested.get("status") or PROCESS_STATUSES.get(
        nested.get("process"), "running"
    )
    images = []
    for image in nested.get("images") or []:
        if isinstance(image, str):
            url = image
        elif isinstance(image, dict):
            url = image.get("image") or image.get("cover") or image.get("url")
        else:
            url = None
        if isinstance(url, str) and url.startswith("https://"):
            images.append({"url": url})
    return {"task_id": str(task_id), "status": status, "images": images}


async def get_commerce_task(
    client: IywClient, task_id: str, *, dry_run: bool = False
) -> dict[str, Any]:
    data = await client.request(
        "api/commerce/getCommerceTaskDetail",
        {"taskId": task_id},
        dry_run=dry_run,
    )
    return data if dry_run else normalize_commerce_task(data, task_id)


async def wait_for_commerce_task(
    client: IywClient,
    task_id: str,
    wait_seconds: int,
    poll_interval: float,
    *,
    dry_run: bool = False,
) -> dict[str, Any]:
    if dry_run:
        return await get_commerce_task(client, task_id, dry_run=True)
    deadline = time.monotonic() + wait_seconds
    while True:
        task = await get_commerce_task(client, task_id)
        if task["status"] in {"succeeded", "failed", "canceled"}:
            return task
        if time.monotonic() >= deadline:
            task["next_poll_seconds"] = max(1, int(poll_interval))
            return task
        await asyncio.sleep(poll_interval)
