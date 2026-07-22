"""Fission image generation through the confirmed microModel APIs."""

from __future__ import annotations

import asyncio
import copy
import json
import time
from typing import Any

from iyw_image import IywClient, IywError, PROCESS_STATUSES


MODEL_OPTIONS_REQUEST = {
    "nameSpace": "COMMON",
    "keys": ["model_options"],
}
DEFAULT_FISSION_MODELS = (
    {
        "platform": "1",
        "size": 28,
        "stats": {
            "model": "v 7.0",
            "reference_image": "",
            "iw": 0.5,
            "cref": "",
            "cw": 100,
            "sref": "",
            "sw": 250,
        },
    },
    {
        "platform": "8",
        "size": 47,
        "stats": {"model": "V_2", "style": "General", "reference_image": ""},
    },
    {
        "platform": "5",
        "size": 36,
        "stats": {"model": "jimeng_t2i_v40", "jimengName": "通用 V4.0"},
    },
    {"platform": "2", "size": 75},
    {
        "platform": "4",
        "size": 20,
        "stats": {"model": "local_flux"},
    },
    {
        "platform": "12",
        "size": 74,
        "stats": {"model": "realistic_image"},
    },
)


def _decode_model_options(data: dict[str, Any]) -> list[dict[str, Any]]:
    raw = data.get("model_options")
    try:
        options = json.loads(raw) if isinstance(raw, str) else raw
    except json.JSONDecodeError as exc:
        raise IywError(
            "model_options is invalid JSON", "upstream_unavailable", True
        ) from exc
    if not isinstance(options, list):
        raise IywError("model_options is missing", "upstream_unavailable", True)
    return [option for option in options if isinstance(option, dict)]


async def _fetch_model_options(client: IywClient) -> list[dict[str, Any]]:
    data = await client.request("getByKeys", MODEL_OPTIONS_REQUEST)
    return _decode_model_options(data)


async def get_fission_models(
    client: IywClient, *, dry_run: bool = False
) -> dict[str, Any]:
    if dry_run:
        return await client.request("getByKeys", MODEL_OPTIONS_REQUEST, dry_run=True)
    options = await _fetch_model_options(client)
    labels = [
        str(option.get("label"))
        for option in options
        if str(option.get("label") or "").startswith("分身")
    ]
    return {"count": len(labels), "labels": labels}


def _configured_model_payloads(
    options: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    templates = {item["platform"]: item for item in DEFAULT_FISSION_MODELS}
    payloads = []
    seen = set()
    for option in options:
        label = str(option.get("label") or "")
        if not label.startswith("分身"):
            continue
        platform = str(option.get("value") or "")
        if platform in seen:
            raise IywError("duplicate fission platform configuration", "configuration")
        template = templates.get(platform)
        if template is None:
            raise IywError(
                f"unsupported live fission configuration: {label}", "configuration"
            )
        seen.add(platform)
        payloads.append(copy.deepcopy(template))
    if not payloads:
        raise IywError("no supported fission models are available", "configuration")
    return payloads


def _batch_payload(prompt: str, models: list[dict[str, Any]]) -> dict[str, Any]:
    normalized = prompt.strip()
    if not normalized:
        raise IywError("fission prompt is required", "invalid_input")
    if len(normalized) > 12000:
        raise IywError("fission prompt exceeds 12000 characters", "invalid_input")
    return {"prompt": normalized, "jsonData": None, "models": models}


def _normalize_created_tasks(data: dict[str, Any]) -> dict[str, Any]:
    raw_tasks = data.get("tasks")
    if not isinstance(raw_tasks, list) or not raw_tasks:
        raise IywError("fission batch returned no tasks", "task_create_failed")
    tasks = []
    for item in raw_tasks:
        nested = item.get("data") if isinstance(item, dict) else None
        task_id = nested.get("taskId") if isinstance(nested, dict) else None
        succeeded = isinstance(item, dict) and item.get("code") == 1 and task_id
        task = {"status": "queued" if succeeded else "failed"}
        if succeeded:
            task["task_id"] = str(task_id)
        tasks.append(task)
    statuses = {task["status"] for task in tasks}
    status = "queued" if statuses == {"queued"} else "failed"
    if len(statuses) > 1:
        status = "partial"
    return {
        "group_id": str(data.get("groupId") or ""),
        "status": status,
        "tasks": tasks,
    }


async def create_fission_tasks(
    api_client: IywClient,
    config_client: IywClient,
    prompt: str,
    *,
    dry_run: bool = False,
) -> dict[str, Any]:
    if dry_run:
        payload = _batch_payload(prompt, list(copy.deepcopy(DEFAULT_FISSION_MODELS)))
        return await api_client.request(
            "api/microModel/v2/batch", payload, dry_run=True
        )
    options = await _fetch_model_options(config_client)
    payload = _batch_payload(prompt, _configured_model_payloads(options))
    data = await api_client.request("api/microModel/v2/batch", payload)
    return _normalize_created_tasks(data)


def normalize_fission_task(
    data: dict[str, Any], fallback_id: str = ""
) -> dict[str, Any]:
    task_id = data.get("task_id") or data.get("taskId") or fallback_id
    status = data.get("status") or PROCESS_STATUSES.get(data.get("process"), "running")
    images = []
    for image in data.get("images") or []:
        url = image.get("image") if isinstance(image, dict) else image
        if isinstance(url, str) and url.startswith("https://"):
            images.append({"url": url})
    return {"task_id": str(task_id), "status": status, "images": images}


async def get_fission_task(
    client: IywClient, task_id: str, *, dry_run: bool = False
) -> dict[str, Any]:
    data = await client.request(
        "api/microModel/GetDetails", {"taskId": task_id}, dry_run=dry_run
    )
    return data if dry_run else normalize_fission_task(data, task_id)


def _fission_group_result(tasks: list[dict[str, Any]]) -> dict[str, Any]:
    statuses = {task["status"] for task in tasks}
    if statuses <= {"succeeded"}:
        status = "succeeded"
    elif statuses & {"queued", "running"}:
        status = "running"
    elif "succeeded" in statuses:
        status = "partial"
    else:
        status = "failed"
    images = [image for task in tasks for image in task.get("images", [])]
    return {"status": status, "tasks": tasks, "images": images}


async def wait_for_fission_tasks(
    client: IywClient,
    task_ids: list[str],
    wait_seconds: int,
    poll_interval: float,
    *,
    dry_run: bool = False,
) -> dict[str, Any]:
    normalized_ids = [str(task_id).strip() for task_id in task_ids]
    if not normalized_ids or any(not task_id for task_id in normalized_ids):
        raise IywError("at least one fission task ID is required", "invalid_input")
    if len(set(normalized_ids)) != len(normalized_ids):
        raise IywError("fission task IDs must be unique", "invalid_input")
    if dry_run:
        requests = [
            await get_fission_task(client, task_id, dry_run=True)
            for task_id in normalized_ids
        ]
        return {"requests": requests}
    tasks = [
        {"task_id": task_id, "status": "queued", "images": []}
        for task_id in normalized_ids
    ]
    pending = set(range(len(tasks)))
    deadline = time.monotonic() + wait_seconds
    while pending:
        for index in list(pending):
            tasks[index] = await get_fission_task(client, normalized_ids[index])
            if tasks[index]["status"] in {"succeeded", "failed", "canceled"}:
                pending.remove(index)
        if not pending or time.monotonic() >= deadline:
            break
        await asyncio.sleep(poll_interval)
    result = _fission_group_result(tasks)
    if pending:
        result["next_poll_seconds"] = max(1, int(poll_interval))
    return result


async def generate_fission_images(
    api_client: IywClient,
    config_client: IywClient,
    prompt: str,
    wait_seconds: int,
    poll_interval: float,
    *,
    dry_run: bool = False,
) -> dict[str, Any]:
    created = await create_fission_tasks(
        api_client, config_client, prompt, dry_run=dry_run
    )
    if dry_run or wait_seconds <= 0 or created["status"] != "queued":
        return created
    task_ids = [task["task_id"] for task in created["tasks"]]
    result = await wait_for_fission_tasks(
        api_client, task_ids, wait_seconds, poll_interval
    )
    result["group_id"] = created["group_id"]
    return result
