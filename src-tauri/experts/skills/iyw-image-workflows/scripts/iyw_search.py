#!/usr/bin/env python3
"""固定 IYW 图片、报告、画册、趋势和 IP 查询 CLI。"""

from __future__ import annotations

import argparse
import asyncio
import json
import re
from pathlib import Path
from typing import Any
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

from iyw_image import IywClient, IywError, _resolve_token


DEFAULT_GATEWAY = "https://gateway.iyw.cn"


SEARCH_SPECS: dict[str, tuple[str, str, str]] = {
    "image": ("https://tu.iyw.cn", "/sapi", "ai-chat/api/imageSearch/search"),
    "catalog": ("https://www.iyw.cn", "/gateway", "ai-chat/api/procurementCatalog/list"),
    "dict-industry": ("https://www.iyw.cn", "/gateway", "account-search/basic/dict/getByKeys"),
    "report-areas": ("https://www.iyw.cn", "/gateway", "exhibition/report/getAreaList"),
    "report-years": ("https://www.iyw.cn", "/gateway", "exhibition/report/getPublishYear"),
    "report-list": ("https://www.iyw.cn", "/gateway", "exhibition/report/queryList"),
    "report-detail": ("https://www.iyw.cn", "/gateway", "exhibition/report/detail"),
    "report-detail-tu": ("https://tu.iyw.cn", "/sapi", "exhibition/report/detail"),
    "report-recommendations": ("https://www.iyw.cn", "/gateway", "exhibition/report/recommendationReport"),
    "report-images": ("https://www.iyw.cn", "/gateway", "exhibition/report/getReportImg"),
    "report-full": ("https://www.iyw.cn", "/gateway", "exhibition/report/getFullReport"),
    "trend-dict": ("https://tu.iyw.cn", "/sapi", "platform/basic/dict/getByKeys"),
    "tool-config": ("https://gateway.iyw.cn", "/platform", "basic/dict/getByKeys"),
    "trend-list": (DEFAULT_GATEWAY, "/theme-activity", "api/Trend/GetTrendList"),
    "trend-detail": (DEFAULT_GATEWAY, "/theme-activity", "api/Trend/GetTrendDetail"),
    "ip-list": (DEFAULT_GATEWAY, "/tu-zp", "api/Ip/GetList"),
    "ip-patterns": (DEFAULT_GATEWAY, "/tu-zp", "api/ip/GetDesignPatternList"),
}

SENSITIVE_KEY = re.compile(
    r"(?:token|cookie|authorization|security|secret|password|signature|signed|credential|request[_-]?id)",
    re.IGNORECASE,
)
SENSITIVE_QUERY = re.compile(
    r"(?:token|signature|sign|expires?|credential|accesskey|securitytoken|policy)",
    re.IGNORECASE,
)


def _read_payload(path: str) -> Any:
    value = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(value, (dict, list)):
        raise IywError("search payload must be a JSON object or array", "invalid_input")
    return value


def _safe_url(value: str) -> str:
    parts = urlsplit(value)
    if not parts.query:
        return value
    query = [(key, item) for key, item in parse_qsl(parts.query, keep_blank_values=True) if not SENSITIVE_QUERY.search(key)]
    return urlunsplit((parts.scheme, parts.netloc, parts.path, urlencode(query), parts.fragment))


def redact_search_result(value: Any, *, key: str = "") -> Any:
    if SENSITIVE_KEY.search(key):
        return None
    if isinstance(value, list):
        return [redact_search_result(item) for item in value]
    if isinstance(value, dict):
        return {
            item_key: redact_search_result(item, key=item_key)
            for item_key, item in value.items()
            if not SENSITIVE_KEY.search(item_key)
        }
    if isinstance(value, str) and value.startswith(("http://", "https://")):
        return _safe_url(value)
    return value


def normalize_tool_config(value: Any) -> dict[str, Any]:
    """仅返回工具能力键，避免把模型和渠道配置暴露给调用方。"""
    if not isinstance(value, dict):
        return {"available": False}
    public_keys = {
        "ai_clothing_type",
        "ai_video",
        "ai_agent_tool_config",
        "ai_imitation_prompt",
        "vector_search_merge_category",
        "optimize_write_prompt",
        "ai_agent_page",
    }
    allowed = sorted(key for key in value if key in public_keys)
    return {"available": True, "capabilities": allowed}


async def run_search(args: argparse.Namespace) -> dict[str, Any]:
    if args.alias not in SEARCH_SPECS:
        raise IywError(f"unsupported search: {args.alias}", "invalid_input")
    default_base, prefix, path = SEARCH_SPECS[args.alias]
    token = "" if args.dry_run else _resolve_token(args.token)
    client = IywClient(
        default_base,
        token,
        prefix,
        args.timeout,
        allow_missing_token=args.dry_run,
    )
    payload = _read_payload(args.input_file)
    result = await client.request(path, payload, dry_run=args.dry_run)
    if args.dry_run:
        return result
    if args.alias == "tool-config":
        return normalize_tool_config(result)
    return redact_search_result(result)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="IYW fixed search CLI")
    sub = parser.add_subparsers(dest="command", required=True)
    search = sub.add_parser("search", help="run a fixed IYW search")
    search.add_argument("--token", help="fallback IYW token")
    search.add_argument("--timeout", type=float, default=300.0)
    search.add_argument("--no-progress", action="store_true")
    search.add_argument("--dry-run", action="store_true")
    search.add_argument("alias", choices=sorted(SEARCH_SPECS))
    search.add_argument("--input-file", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        result = asyncio.run(run_search(args))
    except IywError as exc:
        print(json.dumps({"ok": False, "error": {"code": exc.code, "message": str(exc), "retryable": exc.retryable}}, ensure_ascii=False, indent=2))
        return 1
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(json.dumps({"ok": False, "error": {"code": "invalid_input", "message": str(exc), "retryable": False}}, ensure_ascii=False, indent=2))
        return 1
    print(json.dumps({"ok": True, "data": result}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
