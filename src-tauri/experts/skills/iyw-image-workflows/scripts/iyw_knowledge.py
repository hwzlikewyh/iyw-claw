#!/usr/bin/env python3
"""Standalone CLI for the captured IYW knowledge search API."""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
from typing import Any

from iyw_image import IywClient, IywError, _add_connection_args, _resolve_token

KNOWLEDGE_PREFIX = "/ai-agent-new/api/knowledge"
DEFAULT_LIMIT = 10
DEFAULT_DENSE_WEIGHT = 0.5


def build_payload(args: argparse.Namespace) -> dict[str, Any]:
    query = str(args.query or "").strip()
    if not query:
        raise IywError("knowledge query is required", "invalid_input")
    if args.limit < 1:
        raise IywError("--limit must be greater than zero", "invalid_input")
    if not 0 <= args.dense_weight <= 1:
        raise IywError("--dense-weight must be between 0 and 1", "invalid_input")
    return {
        "category": args.category,
        "query": query,
        "folderId": args.folder_id,
        "fileId": args.file_id,
        "limit": args.limit,
        "denseWeight": args.dense_weight,
    }


def _normalized_chunk(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise IywError(
            "knowledge search returned an invalid result item", "invalid_response"
        )
    document = value.get("doc_info")
    if document is not None and not isinstance(document, dict):
        raise IywError(
            "knowledge search returned invalid document metadata", "invalid_response"
        )
    document = document or {}
    return {
        "id": value.get("id"),
        "score": value.get("score"),
        "content": value.get("content"),
        "md_content": value.get("md_content"),
        "chunk_type": value.get("chunk_type"),
        "doc_id": document.get("doc_id"),
        "doc_name": document.get("doc_name"),
        "doc_type": document.get("doc_type"),
    }


def normalize_search_response(data: dict[str, Any]) -> dict[str, Any]:
    result = data.get("result") if isinstance(data, dict) else None
    if not isinstance(result, dict):
        raise IywError("knowledge search omitted result", "invalid_response")
    if result.get("code") != 0:
        raise IywError(
            str(result.get("message") or "knowledge search failed"),
            "knowledge_search_failed",
        )
    result_data = result.get("data")
    if not isinstance(result_data, dict):
        raise IywError("knowledge search omitted result data", "invalid_response")
    items = result_data.get("result_list")
    count = result_data.get("count")
    if not isinstance(items, list) or not isinstance(count, int) or count < 0:
        raise IywError(
            "knowledge search returned an unexpected shape", "invalid_response"
        )
    return {
        "count": count,
        "results": [_normalized_chunk(item) for item in items],
    }


def _client(args: argparse.Namespace) -> IywClient:
    if args.dry_run:
        return IywClient(
            args.base_url,
            "",
            KNOWLEDGE_PREFIX,
            args.timeout,
            allow_missing_token=True,
        )
    return IywClient(
        args.base_url,
        _resolve_token(args.token),
        KNOWLEDGE_PREFIX,
        args.timeout,
    )


async def run_search(args: argparse.Namespace) -> dict[str, Any]:
    payload = build_payload(args)
    response = await _client(args).request(
        "search", payload, dry_run=args.dry_run
    )
    return response if args.dry_run else normalize_search_response(response)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Standalone IYW knowledge search CLI")
    subparsers = parser.add_subparsers(dest="command", required=True)
    search = subparsers.add_parser("search", help="search the IYW knowledge base")
    _add_connection_args(search)
    search.add_argument("--query", required=True)
    search.add_argument("--category", type=int, default=0)
    search.add_argument("--folder-id", type=int)
    search.add_argument("--file-id")
    search.add_argument("--limit", type=int, default=DEFAULT_LIMIT)
    search.add_argument(
        "--dense-weight", type=float, default=DEFAULT_DENSE_WEIGHT
    )
    return parser


def _failure(exc: Exception) -> int:
    code = exc.code if isinstance(exc, IywError) else "invalid_input"
    retryable = exc.retryable if isinstance(exc, IywError) else False
    print(
        json.dumps(
            {
                "ok": False,
                "error": {
                    "code": code,
                    "message": str(exc),
                    "retryable": retryable,
                },
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 1


def _configure_stdout() -> None:
    reconfigure = getattr(sys.stdout, "reconfigure", None)
    if callable(reconfigure):
        reconfigure(encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    _configure_stdout()
    args = build_parser().parse_args(argv)
    try:
        result = asyncio.run(run_search(args))
    except (IywError, OSError, TypeError, ValueError) as exc:
        return _failure(exc)
    print(json.dumps({"ok": True, "data": result}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
