"""IYW 固定搜索接口的声明式定义。"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class SearchContract:
    base_url: str
    prefix: str
    path: str
    example: Any
    fields: dict[str, str] | None
    response: str


def _contract(
    host: str,
    prefix: str,
    path: str,
    example: Any,
    fields: dict[str, str] | None,
    response: str,
) -> SearchContract:
    return SearchContract(host, prefix, path, example, fields, response)


TU = "https://tu.iyw.cn"
WWW = "https://www.iyw.cn"
GATEWAY = "https://gateway.iyw.cn"
SIGNED_QUERY_KEYS = frozenset(
    {
        "accesskey",
        "accesskeyid",
        "awsaccesskeyid",
        "credential",
        "expires",
        "expiry",
        "ossaccesskeyid",
        "policy",
        "qak",
        "qheaderlist",
        "qkeytime",
        "qurlparamlist",
        "securitytoken",
        "sig",
        "sign",
        "signature",
        "token",
        "xcossecuritytoken",
        "xossadditionalheaders",
        "xosscredential",
        "xossdate",
        "xossexpires",
        "xosssecuritytoken",
        "xosssignature",
        "xosssignatureversion",
    }
)
IMAGE_TIME_RANGES = frozenset(
    {"one_year", "half_year", "three_months", "one_month", "all"}
)
SEARCH_CONTRACTS = {
    "image": _contract(
        TU,
        "/sapi",
        "ai-chat/api/imageSearch/search",
        {
            "classify": ["52"],
            "searchText": "西瓜",
            "searchImage": "",
            "exceptClassify": [3],
            "page": 1,
            "pageSize": 50,
            "timeRange": None,
        },
        {
            "classify": "ids",
            "searchText": "str",
            "searchImage": "url?",
            "exceptClassify": "ids",
            "page": "page",
            "pageSize": "size",
            "timeRange": "str?",
            "market": "nint?",
        },
        "array-page",
    ),
    "catalog": _contract(
        WWW,
        "/gateway",
        "ai-chat/api/procurementCatalog/list",
        {"name": "", "page": 1, "pageSize": 24, "timeRange": "all"},
        {"name": "str", "page": "page", "pageSize": "size", "timeRange": "str"},
        "array-page",
    ),
    "dict-industry": _contract(
        WWW,
        "/gateway",
        "account-search/basic/dict/getByKeys",
        ["industry"],
        None,
        "values",
    ),
    "report-areas": _contract(
        WWW,
        "/gateway",
        "exhibition/report/getAreaList",
        {"publish": 1},
        {"publish": "int"},
        "array",
    ),
    "report-years": _contract(
        WWW,
        "/gateway",
        "exhibition/report/getPublishYear",
        {"publish": 1},
        {"publish": "int"},
        "array",
    ),
    "report-list": _contract(
        WWW,
        "/gateway",
        "exhibition/report/queryList",
        {
            "page": 1,
            "size": 4,
            "status": None,
            "areaIds": [],
            "publishYears": [],
            "industryList": [],
            "type": 1,
            "title": "",
        },
        {
            "page": "page",
            "size": "size",
            "status": "int?",
            "areaIds": "ids",
            "publishYears": "ints",
            "industryList": "ids",
            "type": "int",
            "title": "str",
        },
        "records",
    ),
    "report-detail": _contract(
        WWW,
        "/gateway",
        "exhibition/report/detail",
        {"reportId": 416, "type": 1},
        {"reportId": "id", "type": "int-like"},
        "item",
    ),
    "report-detail-tu": _contract(
        TU,
        "/sapi",
        "exhibition/report/detail",
        {"reportId": "159"},
        {"reportId": "id"},
        "item",
    ),
    "report-recommendations": _contract(
        WWW,
        "/gateway",
        "exhibition/report/recommendationReport",
        {"reportId": 416},
        {"reportId": "id"},
        "list",
    ),
    "report-images": _contract(
        WWW,
        "/gateway",
        "exhibition/report/getReportImg",
        {"reportId": 416, "imgType": 1, "pageNum": 1, "pageSize": 20},
        {"reportId": "id", "imgType": "int", "pageNum": "page", "pageSize": "size"},
        "report-images",
    ),
    "report-full": _contract(
        WWW,
        "/gateway",
        "exhibition/report/getFullReport",
        {"reportId": 416},
        {"reportId": "id"},
        "item",
    ),
    "trend-dict": _contract(
        TU,
        "/sapi",
        "platform/basic/dict/getByKeys",
        {"keys": ["vector_search_merge_category"]},
        {"keys": "strings+"},
        "values",
    ),
    "tool-config": _contract(
        GATEWAY,
        "/platform",
        "basic/dict/getByKeys",
        {
            "nameSpace": "COMMON",
            "keys": [
                "ai_clothing_type",
                "model_options",
                "ai_video",
                "ai_agent_tool_config",
                "ai_imitation_prompt",
                "vector_search_merge_category",
                "ai_gpt_tool_channel",
                "optimize_write_prompt",
                "ai_agent_page",
            ],
        },
        {"nameSpace": "str+", "keys": "strings+"},
        "tool-config",
    ),
    "trend-list": _contract(
        GATEWAY,
        "/theme-activity",
        "api/Trend/GetTrendList",
        {
            "keywords": "",
            "orderBy": 0,
            "market": -1,
            "pageIndex": 1,
            "pageSize": 99,
            "categoryType": 0,
        },
        {
            "keywords": "str",
            "orderBy": "int",
            "market": "int",
            "pageIndex": "page",
            "pageSize": "size",
            "categoryType": "int",
        },
        "items",
    ),
    "trend-detail": _contract(
        GATEWAY,
        "/theme-activity",
        "api/Trend/GetTrendDetail",
        {"activityID": "361"},
        {"activityID": "id"},
        "item",
    ),
    "ip-list": _contract(
        GATEWAY,
        "/tu-zp",
        "api/Ip/GetList",
        {"keywords": "", "recommend": 1, "hasCase": 1, "pageSize": 20, "pageIndex": 1},
        {
            "keywords": "str",
            "recommend": "int",
            "hasCase": "int",
            "pageSize": "size",
            "pageIndex": "page",
        },
        "list-page",
    ),
    "ip-patterns": _contract(
        GATEWAY,
        "/tu-zp",
        "api/ip/GetDesignPatternList",
        {
            "pageSize": 16,
            "pageIndex": 1,
            "ipId": "1175",
            "seriesId": 0,
            "isDesign": False,
        },
        {
            "pageSize": "size",
            "pageIndex": "page",
            "ipId": "id",
            "seriesId": "nint",
            "isDesign": "bool",
        },
        "array-page",
    ),
}

REQUIRED_FIELDS = {
    "report-detail": {"reportId"},
    "report-detail-tu": {"reportId"},
    "report-recommendations": {"reportId"},
    "report-images": {"reportId"},
    "report-full": {"reportId"},
    "trend-detail": {"activityID"},
    "ip-patterns": {"ipId"},
}
