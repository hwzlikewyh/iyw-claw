from __future__ import annotations

import shutil
import tempfile
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import Any

from iyw_sales_layout import (
    MATERIAL_FOLDERS,
    allocate_package_directory,
    create_material_directories,
    material_directory,
    sanitize_path_component,
)
from iyw_sales_office import OFFICE_DELIVERABLES, create_office_deliverables
from iyw_sales_selection import (
    contact_status,
    material_status,
    preferred_attempted_types,
    product_status,
)


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


def copy_selected(items: list[dict[str, Any]], destination: Path) -> list[str]:
    copied: list[str] = []
    destination.mkdir(parents=True, exist_ok=True)
    for index, item in enumerate(items, 1):
        source = Path(str(item["local_path"]))
        filename = sanitize_path_component(source.name, "未命名文件")
        target = destination / f"{index:02d}-{filename}"
        shutil.copy2(source, target)
        copied.append(str(target))
    return copied


def _copy_materials(
    items: list[dict[str, Any]], package: Path, market: str
) -> None:
    create_material_directories(package, market)
    for kind in MATERIAL_FOLDERS[market]:
        selected = [item for item in items if item.get("type") == kind]
        copy_selected(selected, material_directory(package, market, kind))


def _render_package(
    package: Path,
    record: dict[str, Any],
    result: dict[str, Any],
    generated_at: datetime,
) -> None:
    folders = [
        "01-客户信息",
        "02-联系方式",
        "03-置顶产品图片-10张",
        "04-企业信息与评分证据",
        "06-销售话术",
        "07-待办",
    ]
    for folder in folders:
        (package / folder).mkdir(parents=True, exist_ok=True)
    products = result["products"]["selected"]
    materials = result["materials"]["selected"]
    copy_selected(products, package / "03-置顶产品图片-10张")
    _copy_materials(materials, package, record["run"]["market"])
    create_office_deliverables(package, record, result, generated_at)


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
    package = allocate_package_directory(
        Path(output_root),
        record["company"]["name"],
        record["run"]["salesperson"],
        current,
    )
    products = product_status(record.get("products", []), record.get("company"))
    contacts = contact_status(record.get("contacts", []))
    materials = material_status(
        record.get("materials", []),
        record["run"]["market"],
        preferred_attempted=preferred_attempted_types(record.get("material_workflow")),
    )
    crm_result = evaluation["crm_decision"]
    actions = create_pending_actions(record, crm_result)
    status = _package_status(crm_result, [products, contacts, materials])
    result: dict[str, Any] = {
        "package_dir": str(package), "status": status, "evaluation": evaluation,
        "products": products, "contacts": contacts, "materials": materials,
        "pending_actions": actions, "network": False, "crm_write": False,
        "deliverables": list(OFFICE_DELIVERABLES), "writes": False,
    }
    if dry_run or not crm_result["eligible"]:
        return result
    package.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".iyw-sales-package-", dir=package.parent))
    try:
        _render_package(staging, record, result, current)
        staging.replace(package)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    result["writes"] = True
    return result
