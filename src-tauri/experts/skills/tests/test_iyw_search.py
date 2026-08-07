import asyncio
import json
import sys
from urllib.request import Request
from pathlib import Path
from types import SimpleNamespace

import pytest


SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-image-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from iyw_search import SEARCH_SPECS, build_parser, normalize_tool_config, redact_search_result, run_search  # noqa: E402
from iyw_image import IywClient  # noqa: E402


def _args(alias: str, input_file: str) -> SimpleNamespace:
    return SimpleNamespace(
        alias=alias,
        input_file=input_file,
        base_url="https://gateway.iyw.cn",
        token=None,
        timeout=10.0,
        dry_run=True,
    )


def test_search_specs_use_fixed_hosts_and_paths():
    expected = {
        "image", "catalog", "dict-industry", "report-areas", "report-years",
        "report-list", "report-detail", "report-detail-tu", "report-recommendations",
        "report-images", "report-full", "trend-dict", "tool-config", "trend-list",
        "trend-detail", "ip-list", "ip-patterns",
    }
    assert set(SEARCH_SPECS) == expected
    assert SEARCH_SPECS["image"] == ("https://tu.iyw.cn", "/sapi", "ai-chat/api/imageSearch/search")
    assert SEARCH_SPECS["trend-list"] == ("https://gateway.iyw.cn", "/theme-activity", "api/Trend/GetTrendList")
    assert SEARCH_SPECS["ip-list"] == ("https://gateway.iyw.cn", "/tu-zp", "api/Ip/GetList")
    assert SEARCH_SPECS["tool-config"] == ("https://gateway.iyw.cn", "/platform", "basic/dict/getByKeys")


def test_search_dry_run_does_not_require_token(tmp_path, monkeypatch):
    monkeypatch.delenv("IYW_API_BASE_URL", raising=False)
    payload_file = tmp_path / "payload.json"
    payload_file.write_text(json.dumps({"searchText": "西瓜"}), encoding="utf-8")

    result = asyncio.run(run_search(_args("image", str(payload_file))))

    assert result["method"] == "POST"
    assert result["url"] == "https://tu.iyw.cn/sapi/ai-chat/api/imageSearch/search"
    assert result["body"] == {"searchText": "西瓜"}


def test_search_dry_run_preserves_empty_array_payload(tmp_path):
    payload_file = tmp_path / "payload.json"
    payload_file.write_text("[]", encoding="utf-8")

    result = asyncio.run(run_search(_args("dict-industry", str(payload_file))))

    assert result["body"] == []


def test_search_parser_rejects_base_url_override():
    with pytest.raises(SystemExit):
        build_parser().parse_args(
            ["search", "image", "--input-file", "payload.json", "--base-url", "https://evil.invalid"]
        )


def test_search_result_redacts_sensitive_fields_and_signed_queries():
    result = redact_search_result(
        {
            "title": "结果",
            "token": "secret",
            "image_url": "https://cdn.example.com/a.png?Expires=1&x=2",
            "nested": {"authorization": "secret", "value": 1},
        }
    )

    assert "token" not in result
    assert "authorization" not in result["nested"]
    assert result["image_url"] == "https://cdn.example.com/a.png?x=2"


def test_tool_config_only_exposes_capabilities():
    result = normalize_tool_config({"model_options": "secret", "ai_agent_tool_config": "[]"})

    assert result == {"available": True, "capabilities": ["ai_agent_tool_config"]}


def test_client_request_uses_only_token_auth_header():
    client = IywClient("https://gateway.iyw.cn", "token-value", "/api", 10)
    request = Request("https://gateway.iyw.cn/api/test", headers={"token": client.token})

    assert request.get_header("Token") == "token-value"
    assert request.get_header("Authorization") is None
    assert request.get_header("Securitykey") is None
