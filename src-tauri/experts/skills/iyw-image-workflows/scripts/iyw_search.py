#!/usr/bin/env python3
"""固定 IYW 图片、报告、画册、趋势和 IP 查询 CLI。"""

from __future__ import annotations

import argparse
import asyncio
import html
import json
import re
import sys
from pathlib import Path
from typing import Any
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

from iyw_image import IywClient, IywError, _resolve_token
from iyw_search_contracts import (
    SEARCH_CONTRACTS,
    example_payload,
    is_sensitive_query_key,
    normalize_search_response,
    validate_search_payload,
)

SEARCH_SPECS = {
    alias: (contract.base_url, contract.prefix, contract.path)
    for alias, contract in SEARCH_CONTRACTS.items()
}

SENSITIVE_KEY = re.compile(
    r"(?:token|cookie|authorization|security|secret|password|signature|signed|credential|request[_-]?id)",
    re.IGNORECASE,
)
EMBEDDED_URL = re.compile(r"https?://[^\s<>\"')]+", re.IGNORECASE)


def _read_payload(path: str) -> Any:
    value = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(value, (dict, list)):
        raise IywError("search payload must be a JSON object or array", "invalid_input")
    return value


def _safe_url(value: str) -> str:
    parts = urlsplit(value)
    query = [
        (key, item)
        for key, item in parse_qsl(parts.query, keep_blank_values=True)
        if not is_sensitive_query_key(key)
    ]
    netloc = parts.netloc.rsplit("@", 1)[-1]
    return urlunsplit(
        (parts.scheme, netloc, parts.path, urlencode(query), parts.fragment)
    )


def _safe_embedded_url(match: re.Match[str]) -> str:
    original = match.group(0)
    cleaned = _safe_url(html.unescape(original))
    return html.escape(cleaned, quote=False) if "&amp;" in original else cleaned


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
    if isinstance(value, str):
        return EMBEDDED_URL.sub(_safe_embedded_url, value)
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


def _unwrap_client_data(value: Any) -> Any:
    if isinstance(value, dict) and set(value) == {"value"}:
        return value["value"]
    return value


async def run_search(args: argparse.Namespace) -> dict[str, Any]:
    if args.alias not in SEARCH_SPECS:
        raise IywError(f"unsupported search: {args.alias}", "invalid_input")
    default_base, prefix, path = SEARCH_SPECS[args.alias]
    payload = validate_search_payload(args.alias, _read_payload(args.input_file))
    token = "" if args.dry_run else _resolve_token(args.token)
    client = IywClient(
        default_base,
        token,
        prefix,
        args.timeout,
        allow_missing_token=args.dry_run,
    )
    result = await client.request(path, payload, dry_run=args.dry_run)
    if args.dry_run:
        return result
    normalized = normalize_search_response(
        args.alias, _unwrap_client_data(result), payload
    )
    return redact_search_result(normalized)


async def run_command(args: argparse.Namespace) -> dict[str, Any]:
    if args.command == "example":
        return example_payload(args.alias)
    return await run_search(args)


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
    example = sub.add_parser("example", help="print a safe payload template")
    example.add_argument("alias", choices=sorted(SEARCH_SPECS))
    return parser


def main(argv: list[str] | None = None) -> int:
    reconfigure = getattr(sys.stdout, "reconfigure", None)
    if callable(reconfigure):
        reconfigure(encoding="utf-8")
    args = build_parser().parse_args(argv)
    try:
        result = asyncio.run(run_command(args))
    except IywError as exc:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": {
                        "code": exc.code,
                        "message": str(exc),
                        "retryable": exc.retryable,
                    },
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 1
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": {
                        "code": "invalid_input",
                        "message": str(exc),
                        "retryable": False,
                    },
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 1
    output = result if args.command == "example" else {"ok": True, "data": result}
    print(json.dumps(output, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
