from __future__ import annotations

import json
import os
import re
import shutil
import tempfile
from collections import Counter
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import Any

MATERIAL_TARGETS = {
    "export": {
        "exhibition_report": 3,
        "trend_theme": 3,
        "retail_image": 20,
        "catalog_image": 20,
    },
    "domestic": {"trend_theme": 5, "pattern_poster": 10, "ai_image": 20},
}
INVALID_FILENAME = re.compile(r'[<>:"/\\|?*\x00-\x1f]')
RESERVED_NAMES = {"CON", "PRN", "AUX", "NUL"} | {
    f"{prefix}{number}" for prefix in ("COM", "LPT") for number in range(1, 10)
}


def sanitize_company_name(value: str) -> str:
    result = INVALID_FILENAME.sub("_", value).rstrip(" .")
    if not result:
        result = "未命名公司"
    if result.upper() in RESERVED_NAMES:
        result = f"_{result}"
    return result[:120]


def _existing_items(items: object, limit: int | None = None) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    seen: set[Path] = set()
    if not isinstance(items, list):
        return result
    for item in items:
        if not isinstance(item, dict) or not item.get("local_path"):
            continue
        path = Path(str(item["local_path"])).resolve()
        if path.is_file() and path not in seen:
            result.append(item)
            seen.add(path)
    result.sort(key=lambda item: not bool(item.get("representative")))
    return result[:limit] if limit is not None else result


def product_status(products: object) -> dict[str, object]:
    selected = _existing_items(products, 10)
    return {
        "target": 10,
        "actual": len(selected),
        "missing": 10 - len(selected),
        "selected": selected,
    }


def contact_status(contacts: object) -> dict[str, object]:
    selected: list[dict[str, Any]] = []
    seen: set[tuple[object, object, object]] = set()
    if isinstance(contacts, list):
        for contact in contacts:
            if not isinstance(contact, dict) or not contact.get("source"):
                continue
            identity = tuple(
                contact.get(key) for key in ("phone", "email", "public_account")
            )
            if any(identity) and identity not in seen:
                selected.append(contact)
                seen.add(identity)
            if len(selected) == 3:
                break
    return {
        "target": 3,
        "actual": len(selected),
        "missing": 3 - len(selected),
        "selected": selected,
    }


def material_status(materials: object, market: str) -> dict[str, object]:
    target = MATERIAL_TARGETS[market]
    selected: list[dict[str, Any]] = []
    counts: Counter[str] = Counter()
    for item in _existing_items(materials):
        kind = str(item.get("type") or "")
        if kind in target and counts[kind] < target[kind]:
            selected.append(item)
            counts[kind] += 1
    actual = {kind: counts[kind] for kind in target}
    missing = {kind: target[kind] - actual[kind] for kind in target}
    return {
        "target": dict(target),
        "actual": actual,
        "missing": missing,
        "complete": not any(missing.values()),
        "selected": selected,
    }


def _pending(action_type: str, company: str, salesperson: str) -> dict[str, object]:
    return {
        "type": action_type,
        "status": "pending",
        "company": company,
        "salesperson": salesperson,
        "requires_confirmation": True,
    }


def create_pending_actions(
    record: dict[str, Any], crm_result: dict[str, object]
) -> list[dict[str, object]]:
    if not crm_result["eligible"]:
        return []
    company = record["company"]["name"]
    salesperson = record["run"]["salesperson"]
    return [
        _pending("crm_claim_or_create", company, salesperson),
        _pending("notify_sales", company, salesperson),
        _pending("crm_writeback", company, salesperson),
    ]


def _package_status(crm: dict[str, object], statuses: list[dict[str, object]]) -> str:
    decision_name = str(crm["decision"])
    if decision_name.startswith("skip_"):
        return "skipped"
    if not crm["eligible"]:
        return "review"
    missing = []
    for item in statuses:
        value = item["missing"]
        missing.append(any(value.values()) if isinstance(value, dict) else bool(value))
    return "incomplete" if any(missing) else "complete"


def _allocate_directory(root: Path, name: str, now: datetime) -> Path:
    initial = root / sanitize_company_name(name)
    if not initial.exists():
        return initial
    stem = f"{initial.name}-{now.strftime('%Y%m%d-%H%M%S')}"
    candidate = root / stem
    index = 2
    while candidate.exists():
        candidate = root / f"{stem}-{index}"
        index += 1
    return candidate


def _write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
    os.close(handle)
    try:
        Path(temporary).write_text(content, encoding="utf-8")
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def _write_json(path: Path, value: object) -> None:
    _write_text(path, json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def _copy_selected(items: list[dict[str, Any]], destination: Path) -> list[str]:
    copied: list[str] = []
    destination.mkdir(parents=True, exist_ok=True)
    for index, item in enumerate(items, 1):
        source = Path(str(item["local_path"]))
        target = destination / f"{index:02d}-{sanitize_company_name(source.name)}"
        shutil.copy2(source, target)
        copied.append(str(target))
    return copied


def _sources(record: dict[str, Any]) -> list[dict[str, object]]:
    result: list[dict[str, object]] = []
    for section in ("activities", "products", "contacts", "materials"):
        for item in record.get(section, []):
            if isinstance(item, dict) and item.get("source"):
                result.append({"section": section, "source": item["source"]})
    return result


def _contact_markdown(contacts: list[dict[str, Any]]) -> str:
    lines = ["# 优先联系人", ""]
    for index, contact in enumerate(contacts, 1):
        methods = [
            str(contact[key])
            for key in ("phone", "email", "public_account")
            if contact.get(key)
        ]
        lines.append(
            f"{index}. {contact.get('name', '未知')} | "
            f"{' / '.join(methods)} | {contact['source']}"
        )
    return "\n".join(lines) + "\n"


def _render_package(
    package: Path, record: dict[str, Any], result: dict[str, Any]
) -> None:
    folders = [
        "01-客户档案",
        "02-产品图片",
        "03-联系人",
        "04-匹配资料",
        "05-销售话术",
        "06-待办",
    ]
    for folder in folders:
        (package / folder).mkdir(parents=True, exist_ok=True)
    products = result["products"]["selected"]
    materials = result["materials"]["selected"]
    _copy_selected(products, package / "02-产品图片")
    _copy_selected(materials, package / "04-匹配资料")
    _write_json(package / "01-客户档案" / "customer.json", record)
    summary = (
        f"# {record['company']['name']}\n\n"
        f"评分：{result['evaluation']['score']['total']}\n"
        f"CRM：{result['evaluation']['crm_decision']['decision']}\n"
    )
    _write_text(package / "01-客户档案" / "customer.md", summary)
    _write_json(package / "01-客户档案" / "sources.json", _sources(record))
    contacts = result["contacts"]["selected"]
    _write_json(package / "03-联系人" / "contacts.json", contacts)
    _write_text(package / "03-联系人" / "contacts.md", _contact_markdown(contacts))
    outreach = record.get("outreach") or {}
    _write_text(
        package / "05-销售话术" / "opening-copy.md",
        str(outreach.get("opening_copy") or ""),
    )
    ideas = "\n".join(f"- {idea}" for idea in outreach.get("social_ideas", []))
    _write_text(
        package / "05-销售话术" / "social-content.md",
        ideas + ("\n" if ideas else ""),
    )
    _write_json(
        package / "06-待办" / "pending-actions.json", result["pending_actions"]
    )
    manifest = {key: value for key, value in result.items() if key != "package_dir"}
    manifest["files"] = [
        str(path.relative_to(package))
        for path in package.rglob("*")
        if path.is_file()
    ] + ["manifest.json"]
    _write_json(package / "manifest.json", manifest)


def build_package(
    record: object,
    output_root: str | Path,
    evaluator: Callable[[object], dict[str, object]],
    *,
    dry_run: bool = False,
    now: datetime | None = None,
) -> dict[str, object]:
    evaluation = evaluator(record)
    assert isinstance(record, dict)
    current = now or datetime.now().astimezone()
    package = _allocate_directory(Path(output_root), record["company"]["name"], current)
    products = product_status(record.get("products", []))
    contacts = contact_status(record.get("contacts", []))
    materials = material_status(record.get("materials", []), record["run"]["market"])
    crm_result = evaluation["crm_decision"]
    actions = create_pending_actions(record, crm_result)
    status = _package_status(crm_result, [products, contacts, materials])
    result: dict[str, Any] = {
        "package_dir": str(package), "status": status, "evaluation": evaluation,
        "products": products, "contacts": contacts, "materials": materials,
        "pending_actions": actions, "network": False, "crm_write": False,
        "writes": False,
    }
    if dry_run or not crm_result["eligible"]:
        return result
    root = Path(output_root)
    root.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".iyw-sales-package-", dir=root))
    try:
        _render_package(staging, record, result)
        staging.replace(package)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    result["writes"] = True
    return result
