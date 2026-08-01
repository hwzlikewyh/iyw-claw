from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from lixiao_commands import CommandError


CAPTURED_PLATFORM_CODES = {
    "亚马逊": "9",
    "阿里巴巴国际站": "6",
    "速卖通": "12",
    "中国制造国际站": "7",
    "环球资源网": "8",
    "1688": "4",
    "ebay": "13",
    "京东": "1",
    "天猫": "5",
    "Temu": "19",
    "TikTok": "20",
    "虾皮": "17",
    "美客多": "18",
    "Shein": "16",
    "wildberries": "21",
    "TradeKey": "22",
    "Wish": "15",
    "敦煌网": "14",
    "苏宁易购": "2",
    "沃尔玛": "23",
    "来赞达": "24",
    "tokopedia": "25",
    "乐天": "26",
    "家得宝": "27",
    "ozon": "28",
    "Wayfair": "29",
}
CAPTURED_PRODUCT_FILTER = {
    "field": "eProductNameV2",
    "operator": "IN",
    "length_max": 100,
}
ALIASES = {
    "amazon": "亚马逊",
    "alibaba": "阿里巴巴国际站",
    "made in china": "中国制造国际站",
    "global sources": "环球资源网",
    "shein": "Shein",
    "jd": "京东",
    "tmall": "天猫",
}
DEDICATED_SCENES = {
    "亚马逊": "searchEcommercePlatformEnterpriseAmazon",
    "阿里巴巴国际站": "searchEcommercePlatformEnterpriseAlibaba",
    "中国制造国际站": "searchEcommercePlatformEnterpriseMadeInChina",
    "环球资源网": "searchEcommercePlatformEnterpriseGlobalSources",
}


@dataclass(frozen=True)
class PlatformSelection:
    label: str
    code: str
    scene_name: str
    scene_label: str
    product_field: str
    product_operator: str
    product_length_max: int | None


def _config_data(config: dict[str, Any]) -> dict[str, Any]:
    data = config.get("data") if isinstance(config, dict) else None
    if not isinstance(data, dict):
        raise CommandError("Lixiao search condition config has no data object")
    return data


def _platform_options(config: dict[str, Any] | None) -> dict[str, str]:
    if config is None:
        return dict(CAPTURED_PLATFORM_CODES)
    definition = _config_data(config).get("relateEcomShopPlatformV2")
    cv = definition.get("cv") if isinstance(definition, dict) else None
    options = cv.get("options") if isinstance(cv, dict) else None
    if not isinstance(options, list):
        raise CommandError("Lixiao search condition config has no platform options")
    result = {
        str(item["label"]): str(item["value"])
        for item in options
        if isinstance(item, dict) and item.get("label") and item.get("value") != "0"
    }
    if not result:
        raise CommandError("Lixiao search condition config returned no platforms")
    return result


def _product_filter(config: dict[str, Any] | None) -> tuple[str, str, int | None]:
    if config is None:
        return (
            str(CAPTURED_PRODUCT_FILTER["field"]),
            str(CAPTURED_PRODUCT_FILTER["operator"]),
            int(CAPTURED_PRODUCT_FILTER["length_max"]),
        )
    definition = _config_data(config).get("ecomProductName")
    if not isinstance(definition, dict):
        raise CommandError("Lixiao search condition config has no product-name filter")
    relation = definition.get("cr")
    value = definition.get("cv")
    constraint = value.get("constraint") if isinstance(value, dict) else None
    field = definition.get("esFieldName")
    operator = relation.get("defaultValue") if isinstance(relation, dict) else None
    length_max = constraint.get("lengthMax") if isinstance(constraint, dict) else None
    if not field or not operator:
        raise CommandError("Lixiao product-name filter is incomplete")
    if length_max is not None and (not isinstance(length_max, int) or length_max <= 0):
        raise CommandError("Lixiao product-name length constraint is invalid")
    return str(field), str(operator), length_max


def resolve_platform(
    config: dict[str, Any] | None, requested: str
) -> PlatformSelection:
    value = requested.strip()
    label = ALIASES.get(value.casefold(), value)
    options = _platform_options(config)
    matched = next(
        (item for item in options if item.casefold() == label.casefold()), None
    )
    if matched is None:
        raise CommandError(f"unsupported Lixiao ecommerce platform: {requested}")
    scene_name = DEDICATED_SCENES.get(
        matched, "searchEcommercePlatformEnterprise"
    )
    scene_label = matched if matched in DEDICATED_SCENES else "更多电商平台"
    field, operator, length_max = _product_filter(config)
    return PlatformSelection(
        matched,
        options[matched],
        scene_name,
        scene_label,
        field,
        operator,
        length_max,
    )


def build_search_body(
    selection: PlatformSelection,
    keyword: str,
    *,
    page: int,
    page_size: int,
) -> dict[str, Any]:
    term = keyword.strip()
    if not term:
        raise CommandError("ecommerce search keyword must not be empty")
    if selection.product_length_max and len(term) > selection.product_length_max:
        raise CommandError(
            "ecommerce search keyword exceeds the current Lixiao filter limit"
        )
    return {
        "condition": {
            "cn": "composite",
            "cr": "MUST",
            "cv": [
                {
                    "cn": selection.product_field,
                    "cr": selection.product_operator,
                    "cv": [term],
                }
            ],
        },
        "hasUnfolded": 0,
        "hasSyncClue": 0,
        "hasSyncRobot": 0,
        "hasSyncDx": 0,
        "hasSyncIsys": 0,
        "sortBy": 0,
        "syncRobotRangeDate": [],
        "syncDxRangeDate": [],
        "syncIsysRangeDate": [],
        "syncCrmRangeDate": [],
        "matchType": "most_fields",
        "sceneSearchParam": {
            "label": selection.scene_label,
            "name": selection.scene_name,
        },
        "page": page,
        "pagesize": page_size,
        "syncRobotRangeDateRelation": 0,
        "syncDxRangeDateRelation": 0,
        "syncCrmRangeDateRelation": 0,
        "ecommercePlatformFilter": {
            "platform": [selection.code],
        },
    }


def response_items(response: Any) -> tuple[list[dict[str, Any]], int | None]:
    data = response.get("data") if isinstance(response, dict) else None
    items = data.get("items") if isinstance(data, dict) else None
    if not isinstance(items, list):
        raise CommandError("Lixiao ecommerce search response has no data.items")
    candidates = [item for item in items if isinstance(item, dict)]
    total = data.get("total") if isinstance(data.get("total"), int) else None
    return candidates, total
