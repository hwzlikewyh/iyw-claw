#!/usr/bin/env python3
"""Direct HTTP CLI for IYW commerce image operations."""

from __future__ import annotations

import argparse
import asyncio
import json
from pathlib import Path
from typing import Any

from iyw_commerce_core import (
    check_image,
    get_commerce_task,
    invoke_operation,
    upload_and_check,
    wait_for_commerce_task,
)
from iyw_fission_core import (
    generate_fission_images,
    get_fission_models,
    get_fission_task,
    wait_for_fission_tasks,
)
from iyw_image import (
    API_PREFIX,
    IywClient,
    IywError,
    _add_connection_args,
    _resolve_token,
)


FISSION_CONFIG_PREFIX = "/platform/basic/dict"


def _read_payload(path: str) -> dict[str, Any]:
    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise IywError("commerce payload must be a JSON object", "invalid_input")
    return payload


def _client(args: argparse.Namespace, prefix: str = API_PREFIX) -> IywClient:
    token = "" if args.dry_run else _resolve_token(args.token)
    return IywClient(
        args.base_url,
        token,
        prefix,
        args.timeout,
        allow_missing_token=args.dry_run,
    )


def _command_parser(
    subparsers: Any, name: str, help_text: str
) -> argparse.ArgumentParser:
    parser = subparsers.add_parser(name, help=help_text)
    _add_connection_args(parser)
    return parser


async def run_command(args: argparse.Namespace) -> dict[str, Any]:
    client = _client(args)
    if args.command.startswith("fission-"):
        return await _run_fission_command(args, client)
    if args.command == "upload":
        return await upload_and_check(
            client,
            Path(args.file),
            object_key=args.object_key,
            timeout=args.timeout,
            dry_run=args.dry_run,
        )
    if args.command == "check-image":
        return await check_image(client, args.image_url, dry_run=args.dry_run)
    if args.command == "invoke":
        return await invoke_operation(
            client,
            args.operation,
            _read_payload(args.input_file),
            confirm_destructive=args.confirm_destructive,
            dry_run=args.dry_run,
        )
    if args.command == "task-get":
        return await get_commerce_task(client, args.task_id, dry_run=args.dry_run)
    if args.command == "task-wait":
        return await wait_for_commerce_task(
            client,
            args.task_id,
            args.wait_seconds,
            args.poll_interval,
            dry_run=args.dry_run,
        )
    raise IywError(f"unsupported command: {args.command}", "invalid_input")


async def _run_fission_command(
    args: argparse.Namespace, client: IywClient
) -> dict[str, Any]:
    if args.command == "fission-models":
        return await get_fission_models(
            _client(args, FISSION_CONFIG_PREFIX), dry_run=args.dry_run
        )
    if args.command == "fission-generate":
        return await generate_fission_images(
            client,
            _client(args, FISSION_CONFIG_PREFIX),
            args.prompt,
            args.wait_seconds,
            args.poll_interval,
            dry_run=args.dry_run,
        )
    if args.command == "fission-task-get":
        return await get_fission_task(client, args.task_id, dry_run=args.dry_run)
    if args.command == "fission-task-wait":
        return await wait_for_fission_tasks(
            client,
            args.task_id,
            args.wait_seconds,
            args.poll_interval,
            dry_run=args.dry_run,
        )
    raise IywError(f"unsupported command: {args.command}", "invalid_input")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="IYW commerce image CLI")
    sub = parser.add_subparsers(dest="command", required=True)

    upload = _command_parser(sub, "upload", "upload and check a local image")
    upload.add_argument("--file", required=True)
    upload.add_argument("--object-key")

    check = _command_parser(sub, "check-image", "check a public image URL")
    check.add_argument("--image-url", required=True)

    invoke = _command_parser(sub, "invoke", "invoke a commerce operation")
    invoke.add_argument("operation")
    invoke.add_argument("--input-file", required=True)
    invoke.add_argument("--confirm-destructive", action="store_true")

    task_get = _command_parser(sub, "task-get", "get a commerce task")
    task_get.add_argument("--task-id", required=True)

    task_wait = _command_parser(sub, "task-wait", "wait for a commerce task")
    task_wait.add_argument("--task-id", required=True)
    task_wait.add_argument("--wait-seconds", type=int, default=60)

    _command_parser(sub, "fission-models", "list configured fission models")

    fission_generate = _command_parser(
        sub, "fission-generate", "generate one image with each fission model"
    )
    fission_generate.add_argument("--prompt", required=True)
    fission_generate.add_argument("--wait-seconds", type=int, default=120)

    fission_get = _command_parser(sub, "fission-task-get", "get a fission task")
    fission_get.add_argument("--task-id", required=True)

    fission_wait = _command_parser(
        sub, "fission-task-wait", "wait for one or more fission tasks"
    )
    fission_wait.add_argument("--task-id", action="append", required=True)
    fission_wait.add_argument("--wait-seconds", type=int, default=120)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        if args.poll_interval <= 0 or getattr(args, "wait_seconds", 0) < 0:
            raise IywError("poll and wait values must not be negative", "invalid_input")
        result = asyncio.run(run_command(args))
    except IywError as exc:
        result = {"code": exc.code, "message": str(exc), "retryable": exc.retryable}
        print(json.dumps({"ok": False, "error": result}, ensure_ascii=False, indent=2))
        return 1
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        result = {"code": "invalid_input", "message": str(exc), "retryable": False}
        print(json.dumps({"ok": False, "error": result}, ensure_ascii=False, indent=2))
        return 1
    print(json.dumps({"ok": True, "data": result}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
