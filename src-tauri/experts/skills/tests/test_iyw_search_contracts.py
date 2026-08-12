import sys
from copy import deepcopy
from pathlib import Path

import pytest

SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-image-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from iyw_image import IywError
from iyw_search_contracts import (
    SEARCH_CONTRACTS,
    example_payload,
    normalize_search_response,
    validate_search_payload,
)

ALIASES = {
    "image",
    "catalog",
    "dict-industry",
    "report-areas",
    "report-years",
    "report-list",
    "report-detail",
    "report-detail-tu",
    "report-recommendations",
    "report-images",
    "report-full",
    "trend-dict",
    "tool-config",
    "trend-list",
    "trend-detail",
    "ip-list",
    "ip-patterns",
}


def test_all_search_aliases_have_complete_contracts():
    assert set(SEARCH_CONTRACTS) == ALIASES
    for alias, contract in SEARCH_CONTRACTS.items():
        assert contract.base_url.startswith("https://")
        assert contract.prefix.startswith("/")
        assert contract.path and not contract.path.startswith("/")
        assert validate_search_payload(alias, example_payload(alias))


def test_example_payload_is_an_independent_copy():
    first = example_payload("image")
    first["classify"].append("999")

    assert "999" not in example_payload("image")["classify"]


def test_image_defaults_are_applied_to_partial_payload():
    payload = validate_search_payload("image", {"searchText": "西瓜"})

    assert payload == {
        "classify": ["52"],
        "searchText": "西瓜",
        "searchImage": "",
        "exceptClassify": [3],
        "page": 1,
        "pageSize": 50,
        "timeRange": None,
    }


def test_image_search_accepts_documented_market_filter():
    payload = validate_search_payload(
        "image",
        {"classify": ["51"], "searchText": "家居", "market": 2},
    )

    assert payload["market"] == 2


def test_image_search_accepts_string_excluded_classification_ids():
    payload = validate_search_payload(
        "image",
        {"searchText": "家居", "exceptClassify": ["3"]},
    )

    assert payload["exceptClassify"] == ["3"]


@pytest.mark.parametrize("value", [0, "0", "1"])
def test_report_detail_accepts_documented_type_representations(value):
    payload = validate_search_payload("report-detail", {"reportId": 416, "type": value})

    assert payload["type"] == value


@pytest.mark.parametrize(
    "alias,field",
    [
        ("report-detail", "reportId"),
        ("report-detail-tu", "reportId"),
        ("report-recommendations", "reportId"),
        ("report-images", "reportId"),
        ("report-full", "reportId"),
        ("trend-detail", "activityID"),
        ("ip-patterns", "ipId"),
    ],
)
def test_resource_identifiers_are_required(alias, field):
    with pytest.raises(IywError, match=field):
        validate_search_payload(alias, {})


@pytest.mark.parametrize(
    "alias,payload,message",
    [
        ("image", {"searchText": "", "searchImage": ""}, "text or image"),
        ("image", {"searchText": "x", "pageSize": 201}, "pageSize"),
        ("image", {"searchText": "x", "timeRange": {}}, "timeRange"),
        ("image", {"searchText": "x", "market": 1}, "classify 51"),
        ("image", {"searchText": "x", "exceptClassify": ["0"]}, "exceptClassify"),
        ("catalog", {"unknown": True}, "unknown field"),
        ("report-detail", {"reportId": 0, "type": 1}, "reportId"),
        ("trend-list", {"pageIndex": True}, "pageIndex"),
        ("ip-patterns", {"ipId": "1175", "isDesign": 0}, "isDesign"),
    ],
)
def test_search_contracts_reject_invalid_payloads(alias, payload, message):
    with pytest.raises(IywError, match=message):
        validate_search_payload(alias, payload)


@pytest.mark.parametrize(
    "field", ["token", "Cookie", "Authorization", "securityKey", "tokenInfo"]
)
def test_search_contracts_reject_sensitive_fields(field):
    with pytest.raises(IywError, match="sensitive field"):
        validate_search_payload("image", {"searchText": "西瓜", field: "secret"})


@pytest.mark.parametrize(
    "url",
    [
        "http://example.com/image.png",
        "https://example.com/image.png?Expires=1&Signature=secret",
        "https://example.com/image.png?X-Oss-Signature=secret",
        "https://example.com/image.png?X-Cos-Security-Token=secret",
        "https://example.com/image.png?X-Amz-Signature=secret",
        "https://example.com/image.png?X-Goog-Signature=secret",
        "https://user:password@example.com/image.png",
    ],
)
def test_image_search_rejects_unsafe_urls(url):
    with pytest.raises(IywError, match="HTTPS|signed URL|credentials"):
        validate_search_payload("image", {"searchImage": url})


def test_image_search_accepts_non_sensitive_query_names():
    result = validate_search_payload(
        "image",
        {"searchImage": "https://example.com/image.png?design=floral"},
    )

    assert result["searchImage"].endswith("?design=floral")


RESPONSE_CASES = {
    "image": ([{"data_id": "1"}], {"items", "total", "page", "page_size"}),
    "catalog": ([{"pdfId": 1}], {"items", "total", "page", "page_size"}),
    "dict-industry": ({"industry": []}, {"values"}),
    "report-areas": ([{"areaId": 1}], {"items", "total"}),
    "report-years": ([2026], {"items", "total"}),
    "report-list": (
        {"records": [], "total": "15", "current": 1, "size": 4},
        {"items", "total", "page", "page_size"},
    ),
    "report-detail": ({"reportId": 416}, {"item"}),
    "report-detail-tu": ({"reportId": "159"}, {"item"}),
    "report-recommendations": ({"list": []}, {"items", "total"}),
    "report-images": (
        {"imgBrandList": [], "totalCount": 0, "permissionFlag": 1},
        {"items", "total", "page", "page_size", "meta"},
    ),
    "report-full": (
        {"reportImgList": ["https://example.com/report.jpg"]},
        {"item"},
    ),
    "trend-dict": ({"vector_search_merge_category": "[]"}, {"values"}),
    "tool-config": (
        {"model_options": "secret", "ai_agent_tool_config": "[]"},
        {"available", "capabilities"},
    ),
    "trend-list": (
        {"items": [], "totalCount": 0},
        {"items", "total", "page", "page_size"},
    ),
    "trend-detail": ({"detailInfo": {}}, {"item"}),
    "ip-list": ({"list": [], "totalCount": 0}, {"items", "total", "page", "page_size"}),
    "ip-patterns": ([{"id": "1"}], {"items", "total", "page", "page_size"}),
}


@pytest.mark.parametrize("alias", sorted(ALIASES))
def test_all_search_responses_are_validated_and_normalized(alias):
    response, expected_keys = RESPONSE_CASES[alias]

    result = normalize_search_response(
        alias,
        deepcopy(response),
        validate_search_payload(alias, example_payload(alias)),
    )

    assert set(result) == expected_keys


def test_array_response_from_shared_client_can_be_unwrapped():
    from iyw_search import _unwrap_client_data

    assert _unwrap_client_data({"value": [{"data_id": "1"}]}) == [{"data_id": "1"}]
    assert _unwrap_client_data({"value": [], "other": True}) == {
        "value": [],
        "other": True,
    }


@pytest.mark.parametrize(
    "alias,response",
    [
        ("image", {}),
        ("report-list", {"records": "bad", "total": 1}),
        ("report-detail", []),
        ("trend-list", {"items": [], "totalCount": "bad"}),
        ("tool-config", []),
        ("report-years", ["2026"]),
    ],
)
def test_search_response_shape_mismatches_fail(alias, response):
    with pytest.raises(IywError, match="invalid|expected"):
        normalize_search_response(alias, response, example_payload(alias))


def test_response_pagination_is_normalized_to_integers():
    result = normalize_search_response(
        "report-list",
        {"records": [], "total": "15", "current": "1", "size": "4"},
        example_payload("report-list"),
    )

    assert result["page"] == 1
    assert result["page_size"] == 4


@pytest.mark.parametrize(
    "alias,response",
    [
        ("image", ["bad"]),
        ("catalog", [1]),
        ("report-list", {"records": [None], "total": 1}),
        ("report-full", {"reportImgList": [1]}),
        ("trend-detail", {"detailInfo": []}),
        ("ip-patterns", [{"id": "1"}, "bad"]),
    ],
)
def test_nested_response_shapes_reject_invalid_items(alias, response):
    with pytest.raises(IywError, match="result item|expected|image URL"):
        normalize_search_response(alias, response, example_payload(alias))
