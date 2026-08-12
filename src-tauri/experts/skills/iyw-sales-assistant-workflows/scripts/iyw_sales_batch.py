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
from iyw_sales_ppt import generate_company_presentation, validate_company_presentation
from iyw_sales_ppt_package import prepared_presentation_path
from iyw_sales_selection import (
    TRUSTED_PRODUCT_SOURCE,
    contact_status,
    material_status,
    preferred_attempted_types,
    product_status,
)
from iyw_sales_validation import ValidationError, track_results_complete

WORKBOOK_NAME = "今日推荐公司.xlsx"
BUSINESS_INFO_KEYS = (
    "unified_social_credit_code", "legal_representative", "registration_number",
    "organization_code", "registered_capital", "paid_in_capital", "company_type",
    "industry", "approval_date", "business_period", "english_name",
)


def _has_analysis(products: list[dict[str, Any]]) -> bool:
    for product in products:
        analysis = product.get("analysis")
        if isinstance(analysis, str) and analysis.strip():
            return True
        if isinstance(analysis, dict) and any(str(value or "").strip() for value in analysis.values()):
            return True
    return False


def _is_store_product(item: dict[str, Any]) -> bool:
    has_sales_link = any(
        str(item.get(field) or "").strip().startswith("https://")
        for field in ("product_url", "store_url")
    )
    has_image_link = str(item.get("image_url") or "").strip().startswith("https://")
    source = str(item.get("source") or "").strip().casefold()
    return has_sales_link and has_image_link and source == TRUSTED_PRODUCT_SOURCE


def _delivery_status(plan: dict[str, Any]) -> str:
    missing: list[str] = []
    products = plan.get("products", {})
    contacts = plan.get("contacts", {})
    materials = plan.get("materials", {})
    if products.get("missing", 10 - len(products.get("selected", []))):
        missing.append("images")
    if not plan.get("analysis_available", _has_analysis(products.get("selected", []))):
        missing.append("analysis")
    if contacts.get("missing", 0):
        missing.append("contacts")
    if any(materials.get("missing", {}).values()):
        missing.append("materials")
    if not plan.get("ppt_available", False):
        missing.append("ppt")
    record = plan.get("record", {})
    company = record.get("company", {})
    business = company.get("business_info", {})
    if not isinstance(business, dict) or any(not str(business.get(key) or "").strip() for key in BUSINESS_INFO_KEYS):
        missing.append("business_info")
    if not str((record.get("outreach") or {}).get("opening_copy") or "").strip():
        missing.append("opening_copy")
    if not track_results_complete(record):
        missing.append("tracks")
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
        if isinstance(item, dict)
        and is_supported_image(str(item.get("local_path") or ""))
        and _is_store_product(item)
    ]
    plan["products"] = product_status(product_items, record.get("company"))
    plan["contacts"] = contact_status(record.get("contacts", []))
    attempted = preferred_attempted_types(record.get("material_workflow"))
    plan["materials"] = material_status(
        record.get("materials", []), record["run"]["market"], preferred_attempted=attempted
    )
    plan["analysis_available"] = _has_analysis(plan["products"]["selected"])
    plan["ppt_available"] = prepared_presentation_path(record) is not None
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


def _copy_materials(plan: dict[str, Any], company: Path) -> dict[str, str]:
    market = plan["record"]["run"]["market"]
    material_root = company / "销售资料"
    material_root.mkdir(parents=True, exist_ok=True)
    staged: dict[str, str] = {}
    for kind, folder in MATERIAL_FOLDERS[market].items():
        selected = [item for item in plan["materials"]["selected"] if item.get("type") == kind]
        for item, path in zip(selected, copy_selected(selected, material_root / folder)):
            staged[str(item["local_path"])] = path
    return staged


def _prepare_ppt(
    plan: dict[str, Any],
    company: Path,
    final: Path,
    generator: Callable[[dict[str, Any], str | Path], Path] | None,
    validator: Callable[[str | Path], Path] | None,
) -> Path | None:
    folder = company / "准备资料"
    folder.mkdir(parents=True, exist_ok=True)
    filename = f"{sanitize_path_component(plan['name'], '未命名公司')}-销售资料.pptx"
    target = folder / filename
    source = prepared_presentation_path(plan["record"])
    if source is not None:
        try:
            shutil.copy2(source, target)
            if validator is not None:
                validator(target)
        except Exception:
            target.unlink(missing_ok=True)
    if not target.is_file() and generator is not None:
        try:
            generator(plan, target)
        except Exception:
            target.unlink(missing_ok=True)
    if not target.is_file():
        return None
    return final / str(plan["folder_name"]) / "准备资料" / filename


def _render_company(
    plan: dict[str, Any],
    staging: Path,
    final: Path,
    *,
    ppt_generator: Callable[[dict[str, Any], str | Path], Path] | None = generate_company_presentation,
    ppt_validator: Callable[[str | Path], Path] | None = validate_company_presentation,
) -> dict[str, Any]:
    folder = str(plan["folder_name"])
    company = staging / folder
    try:
        selected_products = plan["products"]["selected"]
        previews = copy_selected(selected_products, company / "产品图片")
        for item, path in zip(selected_products, previews):
            item["local_path"] = path
        staged_materials = _copy_materials(plan, company)
        for item in plan["materials"]["selected"]:
            path = staged_materials.get(str(item.get("local_path")))
            if path:
                item["local_path"] = path
        ppt_path = _prepare_ppt(plan, company, final, ppt_generator, ppt_validator)
    except Exception as error:
        plan["error"] = str(error)
        plan["ppt_available"] = False
        plan["status"] = "incomplete"
        ppt_path = None
        previews = []
    else:
        plan["ppt_available"] = ppt_path is not None
        plan["status"] = _delivery_status(plan)
    return {
        "plan": plan,
        "preview_images": previews[:3],
        "company_dir": final / folder,
        "ppt_path": ppt_path,
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
        "complete": sum(item["status"] == "complete" for item in plans),
        "incomplete": sum(item["status"] == "incomplete" for item in plans),
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
    result["summary"] = _summary(plans)
    result["companies"] = [_public_company(plan) for plan in plans]
    return result
