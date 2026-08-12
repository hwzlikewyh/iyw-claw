"""Commerce 请求体和图片 URL 的安全校验。"""

from __future__ import annotations

import re
from typing import Any
from urllib.parse import parse_qsl, urlsplit

from iyw_image import IywError

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


def require_https(url: str, label: str) -> str:
    value = url.strip()
    if not value.startswith("https://"):
        raise IywError(f"{label} must use HTTPS", "invalid_input")
    return value


def validate_payload_safety(
    value: Any, path: str = "payload", *, image_field: bool = False
) -> None:
    if isinstance(value, dict):
        _validate_dict(value, path)
        return
    if isinstance(value, list):
        for index, item in enumerate(value):
            validate_payload_safety(item, f"{path}[{index}]", image_field=image_field)
        return
    if not isinstance(value, str):
        return
    normalized_value = value.strip()
    if image_field:
        require_https(normalized_value, f"{path} URL")
    if normalized_value.startswith("https://"):
        query = parse_qsl(urlsplit(normalized_value).query, keep_blank_values=True)
        if any(SENSITIVE_URL_QUERY.search(key) for key, _ in query):
            raise IywError(f"{path} must not contain a signed URL", "invalid_input")


def _validate_dict(value: dict[Any, Any], path: str) -> None:
    for key, item in value.items():
        normalized = str(key).replace("_", "").replace("-", "").lower()
        if SENSITIVE_PAYLOAD_KEY.search(normalized):
            raise IywError(f"{path} contains a sensitive field: {key}", "invalid_input")
        is_image = normalized in IMAGE_PAYLOAD_KEYS or normalized.endswith("imageurl")
        validate_payload_safety(item, f"{path}.{key}", image_field=is_image)
