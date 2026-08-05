from __future__ import annotations

from typing import Any

from lixiao_commands import CommandError


DEFAULT_PAGE_SIZE = 10
MAX_PAGE_SIZE = 100
SYNC_FLAG_OFF = 0


def empty_condition() -> dict[str, Any]:
    return {"cn": "composite", "cr": "MUST", "cv": []}


def validate_condition(condition: Any) -> dict[str, Any]:
    if not isinstance(condition, dict):
        raise CommandError("search condition must be a JSON object")
    required = ("cn", "cr", "cv")
    if any(key not in condition for key in required):
        raise CommandError("search condition must contain cn, cr, and cv")
    if not isinstance(condition["cn"], str) or not condition["cn"]:
        raise CommandError("search condition cn must be a non-empty string")
    if not isinstance(condition["cr"], str) or not condition["cr"]:
        raise CommandError("search condition cr must be a non-empty string")
    if not isinstance(condition["cv"], list):
        raise CommandError("search condition cv must be a list")
    return condition


def validate_limits(limit: int, page_size: int) -> None:
    if limit <= 0:
        raise CommandError("limit must be positive")
    if not 1 <= page_size <= MAX_PAGE_SIZE:
        raise CommandError(f"page size must be between 1 and {MAX_PAGE_SIZE}")


def _validate_page(page: int) -> None:
    if page <= 0:
        raise CommandError("page must be positive")


def _sync_fields(*, include_isys: bool, include_relations: bool) -> dict[str, Any]:
    fields: dict[str, Any] = {
        "hasSyncClue": SYNC_FLAG_OFF,
        "hasSyncRobot": SYNC_FLAG_OFF,
        "hasSyncDx": SYNC_FLAG_OFF,
        "syncRobotRangeDate": [],
        "syncDxRangeDate": [],
        "syncCrmRangeDate": [],
    }
    if include_isys:
        fields.update({"hasSyncIsys": SYNC_FLAG_OFF, "syncIsysRangeDate": []})
    if include_relations:
        fields.update(
            {
                "syncRobotRangeDateRelation": SYNC_FLAG_OFF,
                "syncDxRangeDateRelation": SYNC_FLAG_OFF,
                "syncCrmRangeDateRelation": SYNC_FLAG_OFF,
            }
        )
        if include_isys:
            fields["syncIsysRangeDateRelation"] = SYNC_FLAG_OFF
    return fields


def build_advanced_search_body(
    condition: dict[str, Any], *, page: int, page_size: int
) -> dict[str, Any]:
    validate_condition(condition)
    _validate_page(page)
    validate_limits(page, page_size)
    return {
        "condition": condition,
        "hasUnfolded": SYNC_FLAG_OFF,
        "sortBy": SYNC_FLAG_OFF,
        **_sync_fields(include_isys=True, include_relations=True),
        "page": page,
        "pagesize": page_size,
        "userClick": 1,
        "templateType": SYNC_FLAG_OFF,
        "templateName": "",
        "templateUuid": "",
    }


def build_tender_search_body(
    condition: dict[str, Any], keyword: str | None, *, page: int, page_size: int
) -> dict[str, Any]:
    validate_condition(condition)
    _validate_page(page)
    validate_limits(page, page_size)
    return {
        "condition": condition,
        "keyword": keyword,
        "hasUnfolded": SYNC_FLAG_OFF,
        "sortBy": SYNC_FLAG_OFF,
        **_sync_fields(include_isys=False, include_relations=False),
        "matchField": ["title", "subjectMatter"],
        "matchType": "most_fields",
        "sceneSearchParam": {
            "label": "招投标项目查询",
            "name": "tenderProjectSearch",
        },
        "page": page,
        "pagesize": page_size,
    }


def build_channel_search_body(
    condition: dict[str, Any], keyword: str | None, *, page: int, page_size: int
) -> dict[str, Any]:
    validate_condition(condition)
    _validate_page(page)
    validate_limits(page, page_size)
    return {
        "condition": condition,
        "keyword": keyword or "",
        "hasUnfolded": SYNC_FLAG_OFF,
        "sortBy": SYNC_FLAG_OFF,
        **_sync_fields(include_isys=False, include_relations=True),
        "matchField": [],
        "sceneSearchParam": {"label": "搜渠道", "name": "searchChannel"},
        "page": page,
        "pagesize": page_size,
    }


def response_items(response: Any) -> tuple[list[dict[str, Any]], int | None]:
    data = response.get("data") if isinstance(response, dict) else None
    if not isinstance(data, dict):
        raise CommandError("Lixiao search response has no data object")
    items = data.get("items")
    if not isinstance(items, list):
        raise CommandError("Lixiao search response has no data.items list")
    total = next(
        (
            value
            for key in ("total", "realTotal", "count")
            if isinstance(value := data.get(key), int) and not isinstance(value, bool)
        ),
        None,
    )
    return [item for item in items if isinstance(item, dict)], total
