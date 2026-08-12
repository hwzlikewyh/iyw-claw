"""IYW 固定搜索接口的请求与响应合同。"""

from __future__ import annotations

import re
from copy import deepcopy
from typing import Any
from urllib.parse import parse_qsl, urlsplit

from iyw_image import IywError
from iyw_search_specs import (
    IMAGE_TIME_RANGES,
    REQUIRED_FIELDS,
    SEARCH_CONTRACTS,
    SIGNED_QUERY_KEYS,
    SearchContract,
)

MAX_PAGE_SIZE = 200
SENSITIVE_KEY = re.compile(
    r"(?:authorization|cookie|credential|password|secret|securitykey|signature|signed|tokeninfo|token)",
    re.IGNORECASE,
)


def example_payload(alias: str) -> Any:
    return deepcopy(_get_contract(alias).example)


def _get_contract(alias: str) -> SearchContract:
    try:
        return SEARCH_CONTRACTS[alias]
    except KeyError as exc:
        raise IywError(f"unsupported search: {alias}", "invalid_input") from exc


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def is_sensitive_query_key(name: str) -> bool:
    normalized = re.sub(r"[^a-z0-9]", "", name.lower())
    signed_prefixes = ("qsign", "xamz", "xgoog")
    return normalized in SIGNED_QUERY_KEYS or normalized.startswith(signed_prefixes)


def _validate_url(value: Any, key: str, *, optional: bool) -> None:
    if optional and value == "":
        return
    if not isinstance(value, str):
        raise IywError(f"{key} must use HTTPS", "invalid_input")
    parts = urlsplit(value)
    if parts.scheme != "https" or not parts.netloc:
        raise IywError(f"{key} must use HTTPS", "invalid_input")
    if parts.username is not None or parts.password is not None:
        raise IywError(f"{key} must not contain credentials", "invalid_input")
    if any(is_sensitive_query_key(name) for name, _ in parse_qsl(parts.query)):
        raise IywError(f"{key} must not be a signed URL", "invalid_input")


def _validate_list(
    value: Any, key: str, item_rule: str, *, required: bool = False
) -> None:
    if not isinstance(value, list) or (required and not value):
        raise IywError(
            f"{key} must be {'a non-empty' if required else 'an'} array",
            "invalid_input",
        )
    for item in value:
        _validate_field(item, key, item_rule)


def _validate_field(value: Any, key: str, rule: str) -> None:
    optional = rule.endswith("?")
    rule = rule.rstrip("?")
    if optional and value is None:
        return
    if rule in {"str", "str+"} and (
        not isinstance(value, str) or (rule == "str+" and not value.strip())
    ):
        raise IywError(
            f"{key} must be a{' non-empty' if rule == 'str+' else ''} string",
            "invalid_input",
        )
    if rule in {"int", "nint", "page", "size"}:
        if not _is_int(value):
            raise IywError(f"{key} must be an integer", "invalid_input")
        minimum = 1 if rule in {"page", "size"} else (0 if rule == "nint" else None)
        if minimum is not None and value < minimum:
            raise IywError(f"{key} is outside the supported range", "invalid_input")
        if rule == "size" and value > MAX_PAGE_SIZE:
            raise IywError(f"{key} must not exceed {MAX_PAGE_SIZE}", "invalid_input")
    elif rule == "bool" and not isinstance(value, bool):
        raise IywError(f"{key} must be a boolean", "invalid_input")
    elif rule == "int-like" and not (
        _is_int(value) or isinstance(value, str) and re.fullmatch(r"-?\d+", value)
    ):
        raise IywError(f"{key} must be an integer or numeric string", "invalid_input")
    elif rule == "id" and not (
        _is_int(value)
        and value > 0
        or isinstance(value, str)
        and value.isdigit()
        and int(value) > 0
    ):
        raise IywError(f"{key} must be a positive ID", "invalid_input")
    elif rule == "url":
        _validate_url(value, key, optional=optional)
    elif rule in {"ids", "ints", "nints", "strings+"}:
        item_rule = {"ids": "id", "ints": "int", "nints": "nint", "strings+": "str+"}[
            rule
        ]
        _validate_list(value, key, item_rule, required=rule == "strings+")


def _reject_sensitive(value: Any, path: str = "payload") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            if SENSITIVE_KEY.search(str(key).replace("_", "").replace("-", "")):
                raise IywError(
                    f"{path} contains a sensitive field: {key}", "invalid_input"
                )
            _reject_sensitive(item, f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_sensitive(item, f"{path}[{index}]")


def validate_search_payload(alias: str, payload: Any) -> Any:
    contract = _get_contract(alias)
    _reject_sensitive(payload)
    if contract.fields is None:
        _validate_list(payload, alias, "str+", required=True)
        return deepcopy(payload)
    if not isinstance(payload, dict):
        raise IywError(f"{alias} payload must be a JSON object", "invalid_input")
    unknown = sorted(set(payload) - set(contract.fields))
    if unknown:
        raise IywError(
            f"{alias} payload contains unknown field: {unknown[0]}", "invalid_input"
        )
    missing = sorted(REQUIRED_FIELDS.get(alias, set()) - set(payload))
    if missing:
        raise IywError(f"{alias} payload requires {missing[0]}", "invalid_input")
    merged = deepcopy(contract.example)
    merged.update(payload)
    for key, rule in contract.fields.items():
        _validate_field(merged.get(key), key, rule)
    if alias == "image":
        if not merged["searchText"].strip() and not merged["searchImage"]:
            raise IywError("image search requires text or image", "invalid_input")
        _validate_url(merged["searchImage"], "searchImage", optional=True)
        if merged["timeRange"] not in IMAGE_TIME_RANGES | {None}:
            raise IywError("timeRange is not supported", "invalid_input")
        if merged.get("market") not in {None, 0, 1, 2}:
            raise IywError("market is not supported", "invalid_input")
        if merged.get("market") is not None and not any(
            int(item) == 51 for item in merged["classify"]
        ):
            raise IywError("market requires trend classify 51", "invalid_input")
    return merged


def _require_dict(value: Any, alias: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise IywError(f"{alias} expected an object response", "invalid_response")
    return value


def _require_list(value: Any, alias: str) -> list[Any]:
    if not isinstance(value, list):
        raise IywError(f"{alias} expected an array response", "invalid_response")
    return value


def _require_object_items(items: list[Any], alias: str) -> list[dict[str, Any]]:
    if any(not isinstance(item, dict) for item in items):
        raise IywError(f"{alias} returned an invalid result item", "invalid_response")
    return items


def _total(value: Any, alias: str) -> int:
    if isinstance(value, str) and value.isdigit():
        value = int(value)
    if not _is_int(value) or value < 0:
        raise IywError(f"{alias} returned an invalid total", "invalid_response")
    return value


def _positive_int(value: Any, alias: str, name: str) -> int:
    if isinstance(value, str) and value.isdigit():
        value = int(value)
    if not _is_int(value) or value <= 0:
        raise IywError(f"{alias} returned an invalid {name}", "invalid_response")
    return value


def _validate_item_response(alias: str, values: dict[str, Any]) -> None:
    if alias == "report-full":
        items = _require_list(values.get("reportImgList"), alias)
        if any(not isinstance(item, str) or not item for item in items):
            raise IywError(f"{alias} returned an invalid image URL", "invalid_response")
    elif alias == "trend-detail":
        _require_dict(values.get("detailInfo"), alias)


def _normalize_array(alias: str, data: Any) -> dict[str, Any]:
    items = _require_list(data, alias)
    if alias == "report-areas":
        _require_object_items(items, alias)
    elif alias == "report-years" and any(not _is_int(item) for item in items):
        raise IywError(f"{alias} returned an invalid year", "invalid_response")
    return {"items": items, "total": len(items)}


def _normalize_array_page(alias: str, data: Any, payload: Any) -> dict[str, Any]:
    items = _require_object_items(_require_list(data, alias), alias)
    page_key = "pageIndex" if "pageIndex" in payload else "page"
    return {
        "items": items,
        "total": len(items),
        "page": payload[page_key],
        "page_size": payload["pageSize"],
    }


def _normalize_object_list(
    alias: str, kind: str, data: Any, payload: Any
) -> dict[str, Any]:
    values = _require_dict(data, alias)
    item_key = {
        "records": "records",
        "items": "items",
        "list": "list",
        "list-page": "list",
        "report-images": "imgBrandList",
    }[kind]
    items = _require_object_items(_require_list(values.get(item_key), alias), alias)
    if kind == "list":
        return {"items": items, "total": len(items)}
    total_key = "total" if kind == "records" else "totalCount"
    result = {
        "items": items,
        "total": _total(values.get(total_key), alias),
        "page": _positive_int(
            values.get("current", payload.get("pageNum", payload.get("pageIndex"))),
            alias,
            "page",
        ),
        "page_size": _positive_int(
            values.get("size", payload.get("pageSize")), alias, "page size"
        ),
    }
    if kind == "report-images":
        result["meta"] = {
            key: item
            for key, item in values.items()
            if key not in {item_key, total_key}
        }
    return result


def normalize_search_response(alias: str, data: Any, payload: Any) -> dict[str, Any]:
    kind = _get_contract(alias).response
    if kind == "tool-config":
        values = _require_dict(data, alias)
        allowed = {
            "ai_clothing_type",
            "ai_video",
            "ai_agent_tool_config",
            "ai_imitation_prompt",
            "vector_search_merge_category",
            "optimize_write_prompt",
            "ai_agent_page",
        }
        return {"available": True, "capabilities": sorted(allowed & set(values))}
    if kind == "values":
        return {"values": _require_dict(data, alias)}
    if kind == "item":
        values = _require_dict(data, alias)
        _validate_item_response(alias, values)
        return {"item": values}
    if kind == "array":
        return _normalize_array(alias, data)
    if kind == "array-page":
        return _normalize_array_page(alias, data, payload)
    return _normalize_object_list(alias, kind, data, payload)
