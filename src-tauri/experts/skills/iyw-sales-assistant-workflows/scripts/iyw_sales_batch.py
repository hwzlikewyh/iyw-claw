from __future__ import annotations

import shutil
import tempfile
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import Any

from iyw_sales_images import is_supported_image
from iyw_sales_layout import (
    MATERIAL_FOLDERS,
    allocate_batch_directory,
    sanitize_path_component,
)
from iyw_sales_package import copy_selected
from iyw_sales_selection import contact_status, material_status, product_status
from iyw_sales_validation import ValidationError

WORKBOOK_NAME = "今日推荐公司.xlsx"


def _has_analysis(products: list[dict[str, Any]]) -> bool:
    for product in products:
        analysis = product.get("analysis")
        if isinstance(analysis, str) and analysis.strip():
            return True
        if isinstance(analysis, dict) and any(str(value or "").strip() for value in analysis.values()):
            return True
    return False


def _delivery_status(plan: dict[str, Any]) -> str:
    missing: list[str] = []
    if not plan["products"]["actual"]:
        missing.append("images")
    if not plan["analysis_available"]:
        missing.append("analysis")
    if plan["contacts"]["missing"]:
        missing.append("contacts")
    if any(plan["materials"]["missing"].values()):
        missing.append("materials")
    return "complete" if not missing else "incomplete"


def _failed_plan(index: int, error: Exception) -> dict[str, Any]:
    return {
        "index": index,
        "name": f"第{index}家公司",
        "recommended": False,
        "status": "failed",
        "decision": "validation_failed",
        "error": str(error),
    }


def _plan_record(
    record: object,
    index: int,
    evaluator: Callable[[object], dict[str, object]],
) -> dict[str, Any]:
    try:
        evaluation = evaluator(record)
        if not isinstance(record, dict):
            raise ValidationError("record must be an object")
    except (ValidationError, TypeError, ValueError) as error:
        return _failed_plan(index, error)
    decision = evaluation["crm_decision"]
    name = str(record["company"]["name"])
    plan: dict[str, Any] = {
        "index": index,
        "name": name,
        "record": record,
        "evaluation": evaluation,
        "recommended": bool(decision["eligible"]),
        "decision": str(decision["decision"]),
    }
    if not plan["recommended"]:
        plan["status"] = "skipped" if plan["decision"].startswith("skip_") else "review"
        return plan
    product_items = [
        item
        for item in record.get("products", [])
        if isinstance(item, dict) and is_supported_image(str(item.get("local_path") or ""))
    ]
    plan["products"] = product_status(product_items)
    plan["contacts"] = contact_status(record.get("contacts", []))
    plan["materials"] = material_status(record.get("materials", []), record["run"]["market"])
    plan["analysis_available"] = _has_analysis(plan["products"]["selected"])
    plan["status"] = _delivery_status(plan)
    return plan


def _assign_company_folders(plans: list[dict[str, Any]], batch: Path) -> None:
    used: set[str] = set()
    for plan in plans:
        if not plan["recommended"]:
            continue
        base = sanitize_path_component(plan["name"], "未命名公司")
        name = base
        index = 2
        while name.casefold() in used:
            name = f"{base}-{index}"
            index += 1
        used.add(name.casefold())
        plan["folder_name"] = name
        plan["company_dir"] = str(batch / name)
        plan["product_dir"] = str(batch / name / "产品图片")
        plan["material_dir"] = str(batch / name / "销售资料")


def _copy_materials(plan: dict[str, Any], company: Path) -> None:
    market = plan["record"]["run"]["market"]
    material_root = company / "销售资料"
    material_root.mkdir(parents=True, exist_ok=True)
    for kind, folder in MATERIAL_FOLDERS[market].items():
        selected = [item for item in plan["materials"]["selected"] if item.get("type") == kind]
        copy_selected(selected, material_root / folder)


def _render_company(plan: dict[str, Any], staging: Path, final: Path) -> dict[str, Any]:
    folder = str(plan["folder_name"])
    company = staging / folder
    previews = copy_selected(plan["products"]["selected"], company / "产品图片")
    _copy_materials(plan, company)
    return {
        "plan": plan,
        "preview_images": previews[:3],
        "company_dir": final / folder,
    }


def _public_company(plan: dict[str, Any]) -> dict[str, object]:
    result: dict[str, object] = {
        "name": plan["name"],
        "recommended": plan["recommended"],
        "status": plan["status"],
        "decision": plan["decision"],
    }
    for key in ("company_dir", "product_dir", "material_dir", "error"):
        if key in plan:
            result[key] = plan[key]
    return result


def _summary(plans: list[dict[str, Any]]) -> dict[str, int]:
    return {
        "recommended": sum(bool(item["recommended"]) for item in plans),
        "skipped": sum(item["status"] == "skipped" for item in plans),
        "review": sum(item["status"] == "review" for item in plans),
        "failed": sum(item["status"] == "failed" for item in plans),
    }


def build_batch_package(
    records: object,
    output_root: str | Path,
    evaluator: Callable[[object], dict[str, object]],
    *,
    dry_run: bool = False,
    now: datetime | None = None,
) -> dict[str, object]:
    if not isinstance(records, list) or not records:
        raise ValidationError("records must be a non-empty array")
    current = now or datetime.now().astimezone()
    root = Path(output_root)
    batch = allocate_batch_directory(root, current)
    plans = [_plan_record(record, index, evaluator) for index, record in enumerate(records, 1)]
    _assign_company_folders(plans, batch)
    result: dict[str, object] = {
        "batch_dir": str(batch),
        "workbook": str(batch / WORKBOOK_NAME),
        "companies": [_public_company(plan) for plan in plans],
        "summary": _summary(plans),
        "network": False,
        "crm_write": False,
        "writes": False,
    }
    if dry_run:
        return result
    root.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".iyw-sales-batch-", dir=root))
    try:
        rendered = [_render_company(plan, staging, batch) for plan in plans if plan["recommended"]]
        from iyw_sales_batch_office import create_batch_recommendation_workbook

        create_batch_recommendation_workbook(staging / WORKBOOK_NAME, rendered, current)
        staging.replace(batch)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    result["writes"] = True
    return result
