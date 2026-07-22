#!/usr/bin/env python3
"""Async direct HTTP CLI for the IYW image gateway.

The CLI intentionally sends only the legacy ``token`` authentication header.
It does not use MCP, Authorization, tokenInfo, SQL, Redis, or queue access.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time
import uuid
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urlparse
from urllib.request import Request, urlopen


TERMINAL_STATUSES = frozenset({"succeeded", "failed", "canceled"})
API_PREFIX = "/ai-application"
PROCESS_STATUSES = {
    0: "queued",
    1: "queued",
    2: "running",
    10: "succeeded",
    20: "failed",
    30: "failed",
    40: "running",
}
INTERNAL_RESULT_KEYS = frozenset(
    {
        "comparemodels",
        "commercetype",
        "label",
        "runs",
        "tooltype",
    }
)
INTERNAL_RESULT_PREFIXES = ("channel", "model", "platform", "provider", "submodel")
ACCOUNT_TOKEN_FILENAME = "iyw-account-token.json"


class IywError(RuntimeError):
    def __init__(self, message: str, code: str = "request_failed", retryable: bool = False):
        super().__init__(message)
        self.code = code
        self.retryable = retryable


class IywClient:
    def __init__(
        self,
        base_url: str,
        token: str,
        prefix: str,
        timeout: float,
        *,
        allow_missing_token: bool = False,
    ):
        self.base_url = base_url.rstrip("/")
        self.token = token.strip()
        self.prefix = "/" + prefix.strip("/")
        self.timeout = timeout
        if not self.base_url:
            raise IywError("base URL is required; use --base-url or IYW_API_BASE_URL", "configuration")
        if not self.token and not allow_missing_token:
            raise IywError(
                "token is required; use --token, IYW_TOKEN, or IYW Claw login",
                "authentication_required",
            )

    async def request(
        self,
        path: str,
        payload: dict[str, Any] | None = None,
        *,
        method: str = "POST",
        dry_run: bool = False,
    ) -> dict[str, Any]:
        if dry_run:
            return {"method": method, "url": self._url(path), "body": payload or {}}
        return await asyncio.to_thread(self._request_sync, path, payload, method)

    def _request_sync(
        self, path: str, payload: dict[str, Any] | None, method: str
    ) -> dict[str, Any]:
        body = None
        url = self._url(path)
        if method == "GET" and payload:
            url = f"{url}?{urlencode(payload)}"
        elif payload is not None:
            body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        request = Request(
            url,
            data=body,
            method=method,
            headers={
                "Accept": "application/json",
                "Content-Type": "application/json",
                "token": self.token,
            },
        )
        try:
            with urlopen(request, timeout=self.timeout) as response:
                raw = response.read()
                status_code = response.status
        except HTTPError as exc:
            raw = exc.read()
            status_code = exc.code
        except URLError as exc:
            raise IywError(f"request failed: {exc.reason}", "upstream_unavailable", True) from exc
        except TimeoutError as exc:
            raise IywError("request timed out", "upstream_unavailable", True) from exc
        try:
            result = json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise IywError("backend returned invalid JSON", "upstream_unavailable", True) from exc
        if not isinstance(result, dict):
            raise IywError("backend returned an invalid response")
        code = result.get("code", 1 if 200 <= status_code < 300 else status_code)
        if not (200 <= status_code < 300) or code != 1:
            error_data = result.get("data") if isinstance(result.get("data"), dict) else {}
            error_code = str(
                result.get("error_code")
                or error_data.get("errorCode")
                or _status_error_code(status_code)
            )
            retryable = error_code in {"rate_limited", "upstream_unavailable"}
            raise IywError(str(result.get("message") or "backend request failed"), error_code, retryable)
        data = result.get("data", result)
        return data if isinstance(data, dict) else {"value": data}

    def _url(self, path: str) -> str:
        return f"{self.base_url}{self.prefix}/{path.strip('/')}"


class Progress:
    def __init__(self, enabled: bool, label: str = "task"):
        self.enabled = enabled
        self.label = label
        self.tty = enabled and sys.stderr.isatty()
        self.last_status = ""

    def update(self, status: str, elapsed: float) -> None:
        if not self.enabled:
            return
        status = status or "running"
        if status == "queued":
            percent = 8
        elif status == "running":
            percent = min(92, max(18, int(18 + elapsed * 7)))
        elif status == "succeeded":
            percent = 100
        else:
            percent = 100
        width = 24
        filled = round(width * percent / 100)
        line = f"[{('=' * filled).ljust(width, '-')}] {percent:3d}% {status}"
        if self.tty:
            sys.stderr.write(f"\r{self.label} {line}")
            sys.stderr.flush()
        elif status != self.last_status or status in TERMINAL_STATUSES:
            print(f"{self.label} {line}", file=sys.stderr)
        self.last_status = status

    def finish(self) -> None:
        if self.tty:
            sys.stderr.write("\n")
            sys.stderr.flush()


def normalize_task(data: dict[str, Any], fallback_id: str = "", operation: str | None = None) -> dict[str, Any]:
    nested = data.get("data") if isinstance(data.get("data"), dict) else data
    task_id = nested.get("task_id") or nested.get("taskId") or fallback_id
    status = nested.get("status") or PROCESS_STATUSES.get(nested.get("process"), "running")
    images = nested.get("images") or nested.get("imageUrls") or []
    if isinstance(images, list) and images and isinstance(images[0], str):
        images = [{"url": item} for item in images]
    result = dict(nested)
    result.update({"task_id": str(task_id), "status": status})
    result["images"] = _images_with_run_metadata(result.get("runs"), images)
    if operation and not result.get("operation"):
        result["operation"] = operation
    return result


def _images_with_run_metadata(runs: Any, fallback: list[Any]) -> list[dict[str, Any]]:
    if not isinstance(runs, list):
        return _normalize_images(fallback)
    flattened: list[dict[str, Any]] = []
    for run_index, run in enumerate(runs, 1):
        if not isinstance(run, dict):
            continue
        metadata = {
            "run_index": run_index,
            "task_id": run.get("task_id") or run.get("taskId"),
            "model_key": run.get("model_key") or run.get("modelKey"),
            "submodel_key": run.get("submodel_key") or run.get("submodelKey"),
            "label": run.get("label") or run.get("model_key") or run.get("modelKey"),
        }
        for image_index, image in enumerate(run.get("images") or [], 1):
            item = dict(image) if isinstance(image, dict) else {"url": image}
            if not item.get("url"):
                continue
            item.update(
                {
                    key: value
                    for key, value in metadata.items()
                    if value is not None
                }
            )
            item["image_index"] = image_index
            flattened.append(item)
    return flattened or _normalize_images(fallback)


def _normalize_images(images: list[Any]) -> list[dict[str, Any]]:
    result = []
    for image in images:
        item = dict(image) if isinstance(image, dict) else {"url": image}
        if item.get("url"):
            result.append(item)
    return result


async def wait_for_task(
    client: IywClient,
    task_id: str,
    timeout: int,
    poll_interval: float,
    progress: Progress,
) -> dict[str, Any]:
    started = time.monotonic()
    deadline = started + max(0, timeout)
    while True:
        task = normalize_task(await client.request("tasks/get", {"taskId": task_id}), task_id)
        progress.update(task.get("status", "running"), time.monotonic() - started)
        if task["status"] in TERMINAL_STATUSES:
            progress.finish()
            return task
        if time.monotonic() >= deadline:
            progress.finish()
            task["next_poll_seconds"] = max(1, int(poll_interval))
            return task
        await asyncio.sleep(max(0.1, poll_interval))


async def create_task(
    client: IywClient,
    operation: str,
    payload: dict[str, Any],
    wait_seconds: int,
    poll_interval: float,
    progress_enabled: bool,
    request_id: str,
    dry_run: bool,
) -> dict[str, Any]:
    progress = Progress(progress_enabled, operation)
    progress.update("queued", 0)
    payload = dict(payload)
    payload["clientRequestId"] = request_id
    created = await client.request(
        f"images/{'generate' if operation == 'image_generate' else 'edit' if operation == 'image_edit' else 'upscale'}",
        payload,
        dry_run=dry_run,
    )
    if dry_run:
        return created
    task = normalize_task(created, operation=operation)
    if wait_seconds <= 0 or task.get("status") in TERMINAL_STATUSES:
        progress.update(task.get("status", "queued"), 0)
        progress.finish()
        return task
    task_id = task.get("task_id")
    if not task_id:
        progress.finish()
        raise IywError("task creation returned no task id", "task_create_failed")
    return await wait_for_task(client, task_id, wait_seconds, poll_interval, progress)


async def download_images(images: list[dict[str, Any]], out: str | None, out_dir: str | None, force: bool) -> list[str]:
    entries = [item for item in images if isinstance(item, dict) and item.get("url")]
    urls = [item["url"] for item in entries]
    if not urls or (not out and not out_dir):
        return []
    if out and len(urls) != 1:
        raise IywError("--out accepts exactly one image; use --out-dir for multiple images", "invalid_input")
    target_dir = Path(out_dir) if out_dir else Path(out).parent
    target_dir.mkdir(parents=True, exist_ok=True)
    targets = [Path(out)] if out else [target_dir / _image_filename(index, item) for index, item in enumerate(entries, 1)]

    async def save(url: str, target: Path) -> str:
        if target.exists() and not force:
            raise IywError(f"output already exists: {target}; use --force", "invalid_input")
        data = await asyncio.to_thread(_download, url)
        await asyncio.to_thread(target.write_bytes, data)
        return str(target)

    return list(await asyncio.gather(*(save(url, target) for url, target in zip(urls, targets))))


def _image_filename(index: int, image: dict[str, Any]) -> str:
    suffix = Path(urlparse(str(image.get("url") or "")).path).suffix.lower()
    if suffix not in {".png", ".jpg", ".jpeg", ".webp"}:
        suffix = ".png"
    return f"{index:02d}-image{suffix}"


def _download(url: str) -> bytes:
    if not url.startswith("https://"):
        raise IywError("result image URL must use HTTPS", "invalid_input")
    try:
        with urlopen(Request(url, headers={"Accept": "image/*"}), timeout=60) as response:
            return response.read()
    except (HTTPError, URLError, TimeoutError) as exc:
        raise IywError(f"image download failed: {exc}", "upstream_unavailable", True) from exc


async def run_command(args: argparse.Namespace) -> dict[str, Any]:
    client = IywClient(
        args.base_url,
        "" if args.dry_run else _resolve_token(args.token),
        API_PREFIX,
        args.timeout,
        allow_missing_token=args.dry_run,
    )
    if args.command == "models":
        payload = {"aspectRatio": args.aspect_ratio} if args.aspect_ratio else {}
        return await client.request("images/models", payload, method="GET", dry_run=args.dry_run)
    if args.command == "generate":
        if args.compare_models and (
            args.model_key or args.submodel_key or args.model_preference
        ):
            raise IywError(
                "explicit model selection requires --single-model",
                "invalid_input",
            )
        payload = _drop_none(
            {
                "prompt": args.prompt,
                "aspectRatio": args.aspect_ratio,
                "count": args.count,
                "quality": args.quality,
                "style": args.style,
                "modelKey": args.model_key,
                "submodelKey": args.submodel_key,
                "modelPreference": args.model_preference,
                "compareModels": args.compare_models,
            }
        )
        result = await create_task(client, "image_generate", payload, args.wait_seconds, args.poll_interval, not args.no_progress, args.client_request_id or str(uuid.uuid4()), args.dry_run)
        result["saved_paths"] = await download_images(result.get("images", []), args.out, args.out_dir, args.force) if not args.dry_run else []
        return result
    if args.command == "edit":
        payload = _drop_none(
            {
                "instruction": args.instruction,
                "images": [_parse_image(value) for value in args.image],
                "mode": args.mode,
                "aspectRatio": args.aspect_ratio,
                "count": args.count,
                "quality": args.quality,
            }
        )
        result = await create_task(client, "image_edit", payload, args.wait_seconds, args.poll_interval, not args.no_progress, args.client_request_id or str(uuid.uuid4()), args.dry_run)
        result["saved_paths"] = await download_images(result.get("images", []), args.out, args.out_dir, args.force) if not args.dry_run else []
        return result
    if args.command == "upscale":
        payload = {"imageUrl": args.image_url, "scale": args.scale, "enhance": args.enhance}
        result = await create_task(client, "image_upscale", _drop_none(payload), args.wait_seconds, args.poll_interval, not args.no_progress, args.client_request_id or str(uuid.uuid4()), args.dry_run)
        result["saved_paths"] = await download_images(result.get("images", []), args.out, args.out_dir, args.force) if not args.dry_run else []
        return result
    if args.command == "task-get":
        return normalize_task(await client.request("tasks/get", {"taskId": args.task_id}), args.task_id)
    if args.command == "task-wait":
        return await wait_for_task(client, args.task_id, args.wait_seconds, args.poll_interval, Progress(not args.no_progress, "task"))
    if args.command == "task-list":
        return await client.request("tasks/list", _drop_none({"page": args.page, "pageSize": args.page_size, "operation": args.operation, "status": args.status, "startDate": args.start_date, "endDate": args.end_date}))
    if args.command == "generate-batch":
        return await run_batch(client, args)
    raise IywError(f"unsupported command: {args.command}", "invalid_input")


async def run_batch(client: IywClient, args: argparse.Namespace) -> dict[str, Any]:
    jobs = [json.loads(line) for line in Path(args.input).read_text(encoding="utf-8").splitlines() if line.strip()]
    semaphore = asyncio.Semaphore(args.concurrency)
    results: list[dict[str, Any]] = [{} for _ in jobs]

    async def run(index: int, job: dict[str, Any]) -> None:
        async with semaphore:
            if not isinstance(job, dict) or not str(job.get("prompt", "")).strip():
                raise IywError(f"job {index} requires a non-empty prompt", "invalid_input")
            payload = _drop_none({"prompt": str(job["prompt"]).strip(), "aspectRatio": job.get("aspect_ratio"), "count": job.get("count", 1), "quality": job.get("quality"), "style": job.get("style"), "modelKey": job.get("model_key"), "submodelKey": job.get("submodel_key"), "modelPreference": job.get("model_preference"), "compareModels": job.get("compare_models", args.batch_compare_models)})
            request_id = str(job.get("client_request_id") or f"batch-{index}-{uuid.uuid4()}")
            if payload.get("compareModels") and any(
                payload.get(key)
                for key in ("modelKey", "submodelKey", "modelPreference")
            ):
                raise IywError(
                    f"job {index} explicit model selection requires compare_models=false",
                    "invalid_input",
                )
            if payload.get("compareModels") and int(payload.get("count", 1)) != 1:
                raise IywError(
                    f"job {index} multi-channel generation uses count=1",
                    "invalid_input",
                )
            results[index - 1] = await create_task(client, "image_generate", payload, args.wait_seconds, args.poll_interval, not args.no_progress, request_id, args.dry_run)

    await asyncio.gather(*(run(index, job) for index, job in enumerate(jobs, 1)))
    return {"count": len(results), "results": results}


def _parse_image(value: str) -> dict[str, str]:
    if "=" in value:
        role, url = value.split("=", 1)
        if role not in {"source", "reference", "mask"}:
            raise IywError("image role must be source, reference, or mask", "invalid_input")
        return {"url": url, "role": role}
    return {"url": value, "role": "source"}


def _drop_none(payload: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in payload.items() if value is not None}


def _public_result(value: Any) -> Any:
    if isinstance(value, list):
        return [_public_result(item) for item in value]
    if not isinstance(value, dict):
        return value
    public: dict[str, Any] = {}
    for key, item in value.items():
        normalized_key = key.replace("_", "").replace("-", "").lower()
        if normalized_key in INTERNAL_RESULT_KEYS or normalized_key.startswith(
            INTERNAL_RESULT_PREFIXES
        ):
            continue
        public[key] = _public_result(item)
    return public


def _status_error_code(status_code: int) -> str:
    return {401: "authentication_required", 403: "permission_denied", 402: "insufficient_credits", 404: "task_not_found", 409: "idempotency_conflict", 429: "rate_limited"}.get(status_code, "upstream_unavailable" if status_code >= 500 else "request_failed")


def _emit_run_report(result: dict[str, Any]) -> None:
    runs = result.get("runs")
    if not isinstance(runs, list) or not runs:
        return
    image_counts: dict[int, int] = {}
    for image in result.get("images") or []:
        if isinstance(image, dict) and image.get("run_index") is not None:
            run_index = int(image["run_index"])
            image_counts[run_index] = image_counts.get(run_index, 0) + 1
    print("IYW image results:", file=sys.stderr)
    for index, run in enumerate(runs, 1):
        if not isinstance(run, dict):
            continue
        status = run.get("status") or result.get("status") or "unknown"
        count = image_counts.get(index, len(run.get("images") or []))
        print(
            f"  image {index}: status={status}, images={count}",
            file=sys.stderr,
        )


def _load_account_access_token(path: Path | None = None) -> str:
    token_path = path or Path.home() / ".iyw-claw" / ACCOUNT_TOKEN_FILENAME
    try:
        raw = token_path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""
    except OSError as exc:
        raise IywError(
            f"failed to read IYW account token file: {token_path}",
            "configuration",
        ) from exc
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise IywError(
            f"IYW account token file contains invalid JSON: {token_path}",
            "configuration",
        ) from exc
    if not isinstance(data, dict):
        raise IywError(
            f"IYW account token file must contain a JSON object: {token_path}",
            "configuration",
        )
    access_token = data.get("access_token")
    return access_token.strip() if isinstance(access_token, str) else ""


def _resolve_token(explicit_token: str | None) -> str:
    if explicit_token and explicit_token.strip():
        return explicit_token.strip()
    env_token = os.getenv("IYW_TOKEN", "").strip()
    return env_token or _load_account_access_token()


def _add_connection_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--base-url", default=os.getenv("IYW_API_BASE_URL", "https://gateway.iyw.cn"))
    parser.add_argument(
        "--token",
        help="IYW token; overrides IYW_TOKEN and the IYW Claw account token",
    )
    parser.add_argument("--timeout", type=float, default=300.0)
    parser.add_argument("--poll-interval", type=float, default=5.0)
    parser.add_argument("--no-progress", action="store_true")
    parser.add_argument("--dry-run", action="store_true")


def _add_create_args(parser: argparse.ArgumentParser) -> None:
    _add_connection_args(parser)
    parser.add_argument("--wait-seconds", type=int, default=30)
    parser.add_argument("--client-request-id")
    parser.add_argument("--out")
    parser.add_argument("--out-dir")
    parser.add_argument("--force", action="store_true")


def _validate_args(args: argparse.Namespace) -> None:
    if args.poll_interval <= 0:
        raise IywError("--poll-interval must be greater than zero", "invalid_input")
    if getattr(args, "wait_seconds", 0) < 0:
        raise IywError("--wait-seconds must not be negative", "invalid_input")
    if args.command in {"generate", "edit"} and not 1 <= args.count <= 4:
        raise IywError("--count must be between 1 and 4", "invalid_input")
    if args.command == "generate" and args.compare_models and args.count != 1:
        raise IywError("multi-channel generation uses one image per model; use --single-model for --count", "invalid_input")
    if args.command == "upscale" and args.scale is not None and not 1 <= args.scale <= 4:
        raise IywError("--scale must be between 1 and 4", "invalid_input")
    if args.command == "generate-batch" and not 1 <= args.concurrency <= 25:
        raise IywError("--concurrency must be between 1 and 25", "invalid_input")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Direct async CLI for IYW image workflows")
    sub = parser.add_subparsers(dest="command", required=True)

    models = sub.add_parser("models", help="list live image models")
    _add_connection_args(models)
    models.add_argument("--aspect-ratio")

    generate = sub.add_parser("generate", help="generate images from a prompt")
    _add_create_args(generate)
    generate.add_argument("--prompt", required=True)
    generate.add_argument("--aspect-ratio")
    generate.add_argument("--count", type=int, default=1)
    generate.add_argument("--quality")
    generate.add_argument("--style")
    generate.add_argument("--model-key")
    generate.add_argument("--submodel-key")
    generate.add_argument("--model-preference")
    mode = generate.add_mutually_exclusive_group()
    mode.add_argument("--compare-models", dest="compare_models", action="store_true")
    mode.add_argument("--single-model", dest="compare_models", action="store_false")
    generate.set_defaults(compare_models=True)

    edit = sub.add_parser("edit", help="edit HTTPS images")
    _add_create_args(edit)
    edit.add_argument("--instruction", required=True)
    edit.add_argument("--image", action="append", required=True, help="URL or role=URL; repeat for multiple images")
    edit.add_argument("--mode", choices=["auto", "general", "background", "erase", "inpaint", "outpaint"], default="auto")
    edit.add_argument("--aspect-ratio")
    edit.add_argument("--count", type=int, default=1)
    edit.add_argument("--quality")

    upscale = sub.add_parser("upscale", help="upscale or enhance an HTTPS image")
    _add_create_args(upscale)
    upscale.add_argument("--image-url", required=True)
    upscale.add_argument("--scale", type=int)
    upscale.add_argument("--enhance", action="store_true")

    get_task = sub.add_parser("task-get", help="get authoritative task state")
    _add_connection_args(get_task)
    get_task.add_argument("--task-id", required=True)

    wait_task = sub.add_parser("task-wait", help="wait for an existing task")
    _add_connection_args(wait_task)
    wait_task.add_argument("--task-id", required=True)
    wait_task.add_argument("--wait-seconds", type=int, default=60)

    task_list = sub.add_parser("task-list", help="list task history")
    _add_connection_args(task_list)
    task_list.add_argument("--page", type=int, default=1)
    task_list.add_argument("--page-size", type=int, default=20)
    task_list.add_argument("--operation")
    task_list.add_argument("--status")
    task_list.add_argument("--start-date")
    task_list.add_argument("--end-date")

    batch = sub.add_parser("generate-batch", help="generate JSONL prompts concurrently")
    _add_create_args(batch)
    batch.add_argument("--input", required=True, help="JSONL file; each line contains prompt and optional generation fields")
    batch.add_argument("--concurrency", type=int, default=5)
    batch_mode = batch.add_mutually_exclusive_group()
    batch_mode.add_argument("--compare-models", dest="batch_compare_models", action="store_true")
    batch_mode.add_argument("--single-model", dest="batch_compare_models", action="store_false")
    batch.set_defaults(batch_compare_models=True)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        _validate_args(args)
        result = asyncio.run(run_command(args))
    except IywError as exc:
        print(json.dumps({"ok": False, "error": {"code": exc.code, "message": str(exc), "retryable": exc.retryable}}, ensure_ascii=False, indent=2))
        return 1
    except (ValueError, OSError, json.JSONDecodeError) as exc:
        print(json.dumps({"ok": False, "error": {"code": "invalid_input", "message": str(exc), "retryable": False}}, ensure_ascii=False, indent=2))
        return 1
    _emit_run_report(result)
    public_result = result if args.command == "models" else _public_result(result)
    print(json.dumps({"ok": True, "data": public_result}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
