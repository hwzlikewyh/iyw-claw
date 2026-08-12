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
    "material_workflow",
    "track_results",
    "pending_actions",
    "errors",
}
ARRAY_FIELDS = {
    "activities",
    "products",
    "contacts",
    "materials",
    "track_results",
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
TRACK_NAMES = {
    "business_contacts",
    "product_images",
    "activity_evidence",
    "image_materials_ppt",
}
COMPLETE_TRACK_STATUSES = {"ok", "complete", "completed"}


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


def _validate_material_workflow(record: dict[str, Any]) -> None:
    workflow = record.get("material_workflow")
    if workflow is None:
        return
    if not isinstance(workflow, dict) or not isinstance(
        workflow.get("attempts", []), list
    ):
        raise ValidationError("material_workflow.attempts must be an array")
    if any(not isinstance(item, dict) for item in workflow.get("attempts", [])):
        raise ValidationError("material_workflow.attempts items must be objects")


def _validate_track_results(record: dict[str, Any], company: dict[str, Any]) -> None:
    expected_key = str(company.get("lixiao_id") or company.get("name") or "").strip()
    seen: set[str] = set()
    for item in record.get("track_results", []):
        track = item.get("track")
        if track not in TRACK_NAMES:
            raise ValidationError("track_results[].track is invalid")
        if track in seen:
            raise ValidationError("track_results[] contains a duplicate track")
        seen.add(track)
        if str(item.get("company_key") or "").strip() != expected_key:
            raise ValidationError("track_results[].company_key does not match company")
        if not str(item.get("status") or "").strip():
            raise ValidationError("track_results[].status is required")
        if not isinstance(item.get("missing"), list):
            raise ValidationError("track_results[].missing must be an array")


def track_results_complete(record: dict[str, Any]) -> bool:
    tracks = record.get("track_results", [])
    if not isinstance(tracks, list):
        return False
    for name in TRACK_NAMES:
        matching = [item for item in tracks if isinstance(item, dict) and item.get("track") == name]
        if len(matching) != 1:
            return False
        if str(matching[0].get("status") or "").casefold() not in COMPLETE_TRACK_STATUSES:
            return False
    return True


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
    _validate_material_workflow(record)
    _validate_track_results(record, company)
    return record
