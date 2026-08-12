import asyncio
import json
import sys
from pathlib import Path
from types import SimpleNamespace
from urllib.request import Request

import pytest

SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-image-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import iyw_search
from iyw_image import IywClient
from iyw_search import (
    SEARCH_SPECS,
    build_parser,
    main,
    normalize_tool_config,
    redact_search_result,
    run_command,
    run_search,
)
from iyw_search_contracts import example_payload


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
    assert set(SEARCH_SPECS) == expected
    assert SEARCH_SPECS["image"] == (
        "https://tu.iyw.cn",
        "/sapi",
        "ai-chat/api/imageSearch/search",
    )
    assert SEARCH_SPECS["trend-list"] == (
        "https://gateway.iyw.cn",
        "/theme-activity",
        "api/Trend/GetTrendList",
    )
    assert SEARCH_SPECS["ip-list"] == (
        "https://gateway.iyw.cn",
        "/tu-zp",
        "api/Ip/GetList",
    )
    assert SEARCH_SPECS["tool-config"] == (
        "https://gateway.iyw.cn",
        "/platform",
        "basic/dict/getByKeys",
    )


@pytest.mark.parametrize("alias", sorted(SEARCH_SPECS))
def test_every_search_example_completes_cli_dry_run(alias, tmp_path):
    payload_file = tmp_path / f"{alias}.json"
    payload_file.write_text(
        json.dumps(example_payload(alias), ensure_ascii=False), encoding="utf-8"
    )

    result = asyncio.run(run_search(_args(alias, str(payload_file))))

    base_url, prefix, path = SEARCH_SPECS[alias]
    assert result["method"] == "POST"
    assert result["url"] == f"{base_url}{prefix}/{path}"
    assert result["body"] == example_payload(alias)


def test_search_dry_run_does_not_require_token(tmp_path, monkeypatch):
    monkeypatch.delenv("IYW_API_BASE_URL", raising=False)
    payload_file = tmp_path / "payload.json"
    payload_file.write_text(json.dumps({"searchText": "西瓜"}), encoding="utf-8")

    result = asyncio.run(run_search(_args("image", str(payload_file))))

    assert result["method"] == "POST"
    assert result["url"] == "https://tu.iyw.cn/sapi/ai-chat/api/imageSearch/search"
    assert result["body"] == {
        "classify": ["52"],
        "searchText": "西瓜",
        "searchImage": "",
        "exceptClassify": [3],
        "page": 1,
        "pageSize": 50,
        "timeRange": None,
    }


def test_search_rejects_empty_array_payload(tmp_path):
    payload_file = tmp_path / "payload.json"
    payload_file.write_text("[]", encoding="utf-8")

    with pytest.raises(Exception, match="non-empty array"):
        asyncio.run(run_search(_args("dict-industry", str(payload_file))))


def test_search_validates_payload_before_resolving_token(tmp_path, monkeypatch):
    payload_file = tmp_path / "payload.json"
    payload_file.write_text("{}", encoding="utf-8")
    args = _args("report-detail", str(payload_file))
    args.dry_run = False
    monkeypatch.setattr(
        iyw_search,
        "_resolve_token",
        lambda _token: pytest.fail("token must not be read for invalid input"),
    )

    with pytest.raises(Exception, match="reportId"):
        asyncio.run(run_search(args))


def test_search_dry_run_accepts_dictionary_example(tmp_path):
    payload_file = tmp_path / "payload.json"
    payload_file.write_text('["industry"]', encoding="utf-8")

    result = asyncio.run(run_search(_args("dict-industry", str(payload_file))))

    assert result["body"] == ["industry"]


def test_example_command_returns_a_valid_template():
    args = build_parser().parse_args(["example", "image"])

    result = asyncio.run(run_command(args))

    assert result["searchText"] == "西瓜"
    assert result["pageSize"] == 50


def test_example_cli_prints_bare_reusable_json(capsys):
    assert main(["example", "image"]) == 0

    output = json.loads(capsys.readouterr().out)
    assert "ok" not in output
    assert output["searchText"] == "西瓜"


def test_search_parser_rejects_base_url_override():
    with pytest.raises(SystemExit):
        build_parser().parse_args(
            [
                "search",
                "image",
                "--input-file",
                "payload.json",
                "--base-url",
                "https://evil.invalid",
            ]
        )


def test_search_result_redacts_sensitive_fields_and_signed_queries():
    result = redact_search_result(
        {
            "title": "结果",
            "token": "secret",
            "image_url": "https://cdn.example.com/a.png?Expires=1&x=2",
            "oss_url": "https://cdn.example.com/a.png?X-Oss-Signature=secret&x=2",
            "aws_url": "https://cdn.example.com/a.png?X-Amz-Signature=secret&x=2",
            "google_url": "https://cdn.example.com/a.png?X-Goog-Signature=secret&x=2",
            "design_url": "https://cdn.example.com/a.png?design=floral&x=2",
            "asset_url": "HTTPS://user:pass@cdn.example.com/a.png?x=2",
            "designation": "summer",
            "nested": {"authorization": "secret", "value": 1},
        }
    )

    assert "token" not in result
    assert "authorization" not in result["nested"]
    assert result["image_url"] == "https://cdn.example.com/a.png?x=2"
    assert result["oss_url"] == "https://cdn.example.com/a.png?x=2"
    assert result["aws_url"] == "https://cdn.example.com/a.png?x=2"
    assert result["google_url"] == "https://cdn.example.com/a.png?x=2"
    assert result["design_url"].endswith("?design=floral&x=2")
    assert result["asset_url"] == "https://cdn.example.com/a.png?x=2"
    assert result["designation"] == "summer"


def test_search_result_redacts_signed_urls_inside_rich_text():
    result = redact_search_result(
        {
            "content": (
                '<img src="https://cdn.example.com/a.png?x=2&amp;X-Oss-Signature=secret"> '
                "[source](https://cdn.example.com/b.png?X-Cos-Security-Token=secret&x=3) "
                "https://cdn.example.com/c.png?X-Amz-Signature=secret&x=4 "
                "https://cdn.example.com/d.png?X-Goog-Signature=secret&x=5"
            )
        }
    )

    assert "X-Oss-Signature" not in result["content"]
    assert "X-Cos-Security-Token" not in result["content"]
    assert "X-Amz-Signature" not in result["content"]
    assert "X-Goog-Signature" not in result["content"]
    assert "x=2" in result["content"]
    assert "x=3" in result["content"]
    assert "x=4" in result["content"]
    assert "x=5" in result["content"]


def test_tool_config_only_exposes_capabilities():
    result = normalize_tool_config(
        {"model_options": "secret", "ai_agent_tool_config": "[]"}
    )

    assert result == {"available": True, "capabilities": ["ai_agent_tool_config"]}


def test_client_request_uses_only_token_auth_header():
    client = IywClient("https://gateway.iyw.cn", "token-value", "/api", 10)
    request = Request(
        "https://gateway.iyw.cn/api/test", headers={"token": client.token}
    )

    assert request.get_header("Token") == "token-value"
    assert request.get_header("Authorization") is None
    assert request.get_header("Securitykey") is None
