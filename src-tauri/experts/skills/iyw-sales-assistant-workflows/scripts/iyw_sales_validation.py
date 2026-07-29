from __future__ import annotations

import re
from datetime import datetime
from typing import Any

ALLOWED_TOP_LEVEL = {
    "company",
    "run",
    "activities",
    "crm",
    "products",
    "contacts",
    "materials",
    "outreach",
    "pending_actions",
    "errors",
}
ARRAY_FIELDS = {
    "activities",
    "products",
    "contacts",
    "materials",
    "pending_actions",
    "errors",
}
SENSITIVE_KEYS = {
    "apikey",
    "authorization",
    "captcha",
    "challenge",
    "jwt",
    "requestverificationtoken",
    "saasrefreshtoken",
    "seccode",
    "validate",
}
SENSITIVE_SUFFIXES = ("password", "cookie", "token")


class ValidationError(ValueError):
    pass


def parse_datetime(value: str) -> datetime:
    normalized = value.strip().replace("Z", "+00:00")
    try:
        result = datetime.fromisoformat(normalized)
    except (TypeError, ValueError) as error:
        raise ValidationError(f"invalid ISO 8601 datetime: {value!r}") from error
    if result.tzinfo is None:
        raise ValidationError("datetime must include a timezone")
    return result


def _require_mapping(record: dict[str, Any], key: str) -> dict[str, Any]:
    value = record.get(key)
    if not isinstance(value, dict):
        raise ValidationError(f"{key} must be an object")
    return value


def _require_text(mapping: dict[str, Any], path: str, key: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValidationError(f"{path}.{key} must be a non-empty string")
    return value.strip()


def _is_sensitive_key(key: object) -> bool:
    normalized = re.sub(r"[^a-z0-9]", "", str(key).lower())
    return normalized in SENSITIVE_KEYS or normalized.endswith(SENSITIVE_SUFFIXES)


def _reject_sensitive_keys(value: object, path: str = "record") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            if _is_sensitive_key(key):
                raise ValidationError(f"sensitive field is not allowed: {path}.{key}")
            _reject_sensitive_keys(item, f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_sensitive_keys(item, f"{path}[{index}]")


def _validate_arrays(record: dict[str, Any]) -> None:
    for field in ARRAY_FIELDS:
        value = record.get(field, [])
        if not isinstance(value, list):
            raise ValidationError(f"{field} must be an array")
        if any(not isinstance(item, dict) for item in value):
            raise ValidationError(f"{field} items must be objects")
    for action in record.get("pending_actions", []):
        if action.get("status") not in (None, "pending"):
            raise ValidationError("input pending actions cannot be completed")


def validate_record(record: object) -> dict[str, Any]:
    if not isinstance(record, dict):
        raise ValidationError("record must be an object")
    _reject_sensitive_keys(record)
    unknown = set(record) - ALLOWED_TOP_LEVEL
    if unknown:
        raise ValidationError(f"unknown top-level fields: {', '.join(sorted(unknown))}")
    company = _require_mapping(record, "company")
    run = _require_mapping(record, "run")
    _require_mapping(record, "crm")
    _require_text(company, "company", "name")
    market = _require_text(run, "run", "market")
    _require_text(run, "run", "salesperson")
    parse_datetime(_require_text(run, "run", "as_of"))
    if market not in {"export", "domestic"}:
        raise ValidationError("run.market must be export or domestic")
    if "outreach" in record and not isinstance(record["outreach"], dict):
        raise ValidationError("outreach must be an object")
    _validate_arrays(record)
    return record
