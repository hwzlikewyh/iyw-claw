from __future__ import annotations

import hashlib
from collections import Counter
from pathlib import Path
from typing import Any
from zipfile import is_zipfile

from iyw_sales_images import is_supported_image
from iyw_sales_layout import MATERIAL_TARGETS

MATERIAL_IMAGE_TYPES = {"retail_image", "catalog_image", "pattern_poster", "ai_image"}
MATERIAL_DOCUMENT_SUFFIXES = {".pdf", ".ppt", ".pptx", ".doc", ".docx", ".xls", ".xlsx"}
PREFERRED_CAPABILITIES = {
    "exhibition_report": ("report-list", "report-detail", "report-full", "report-images"),
    "trend_theme": ("trend-list", "trend-detail"),
    "retail_image": ("image",),
    "catalog_image": ("catalog",),
    "pattern_poster": ("ip-patterns",),
    "ai_image": ("fission-generate",),
}
FINAL_ATTEMPT_STATUSES = {
    "ok", "complete", "completed", "empty", "failed", "partial", "no_results"
}
TRUSTED_PRODUCT_SOURCE = "lixiao:company-products"


def _existing_items(items: object) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    seen: set[Path] = set()
    if not isinstance(items, list):
        return result
    for item in items:
        if not isinstance(item, dict) or not item.get("local_path"):
            continue
        path = Path(str(item["local_path"])).resolve()
        try:
            usable = path.is_file() and path.stat().st_size > 0
        except OSError:
            usable = False
        if usable and path not in seen:
            result.append(item)
            seen.add(path)
    return result


def _product_rank(item: dict[str, Any]) -> int | None:
    value = item.get("rank")
    if isinstance(value, bool) or value is None:
        return None
    try:
        rank = int(value)
    except (TypeError, ValueError):
        return None
    return rank if rank > 0 else None


def _product_priority(item: dict[str, Any]) -> tuple[bool, bool, int, bool]:
    rank = _product_rank(item)
    pinned = bool(item.get("pinned") or item.get("is_top"))
    return (not pinned, rank is None, rank or 0, not bool(item.get("representative")))


def _url_host(value: object) -> str:
    from urllib.parse import urlparse

    parsed = urlparse(str(value or "").strip())
    return parsed.hostname.casefold() if parsed.scheme == "https" and parsed.hostname else ""


def _belongs_to_company(item: dict[str, Any], company: dict[str, Any] | None) -> bool:
    if not company:
        return True
    matched = False
    for item_key, company_keys in (("company_id", ("lixiao_id", "id")), ("company_name", ("name",))):
        item_value = str(item.get(item_key) or "").strip()
        candidates = [str(company.get(key) or "").strip() for key in company_keys]
        if item_value and any(item_value.casefold() == value.casefold() for value in candidates if value):
            matched = True
            continue
        if item_value:
            return False
    company_shop = _url_host(company.get("shop_url"))
    item_shop = _url_host(item.get("store_url"))
    if company_shop and item_shop:
        if company_shop != item_shop:
            return False
        matched = True
    return matched


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _has_product_receipt(item: dict[str, Any]) -> bool:
    image_url = str(item.get("image_url") or "").strip()
    has_link = bool(_url_host(item.get("product_url")) or _url_host(item.get("store_url")))
    source = str(item.get("source") or "").casefold()
    receipt = item.get("download_receipt")
    path = Path(str(item.get("local_path") or ""))
    if not isinstance(receipt, dict) or not path.is_file():
        return False
    try:
        digest = _sha256(path)
    except OSError:
        return False
    valid_download = (
        bool(_url_host(image_url))
        and receipt.get("source_url") == image_url
        and bool(_url_host(receipt.get("resolved_url")))
        and receipt.get("sha256") == digest
    )
    return has_link and valid_download and source == TRUSTED_PRODUCT_SOURCE


def product_status(products: object, company: dict[str, Any] | None = None) -> dict[str, object]:
    selected = sorted(
        [
            item
            for item in _existing_items(products)
            if _belongs_to_company(item, company)
            and (company is None or _has_product_receipt(item))
        ],
        key=_product_priority,
    )[:10]
    return {
        "target": 10,
        "actual": len(selected),
        "missing": 10 - len(selected),
        "selection_rule": "pinned, rank, representative, upstream order",
        "selected": selected,
    }


def _has_phone(contact: dict[str, Any]) -> bool:
    return bool(str(contact.get("phone") or "").strip())


def contact_status(contacts: object) -> dict[str, object]:
    candidates: list[dict[str, Any]] = []
    seen: set[tuple[object, object, object]] = set()
    if isinstance(contacts, list):
        for contact in contacts:
            if not isinstance(contact, dict) or not contact.get("source"):
                continue
            identity = tuple(
                contact.get(key) for key in ("phone", "email", "public_account")
            )
            if any(identity) and identity not in seen:
                candidates.append(contact)
                seen.add(identity)
    selected = sorted(candidates, key=lambda item: not _has_phone(item))[:3]
    phone_actual = sum(_has_phone(item) for item in selected)
    return {
        "target": 3,
        "actual": len(selected),
        "phone_actual": phone_actual,
        "missing": max(3 - len(selected), 3 - phone_actual),
        "selected": selected,
    }


def _material_file_is_usable(item: dict[str, Any], kind: str) -> bool:
    path = Path(str(item.get("local_path") or ""))
    if kind in MATERIAL_IMAGE_TYPES:
        if not is_supported_image(path):
            return False
        if kind != "ai_image":
            return True
        receipt = item.get("generation_receipt")
        return (
            isinstance(receipt, dict)
            and receipt.get("status") == "succeeded"
            and bool(receipt.get("task_id") or receipt.get("task_ids"))
        )
    if is_supported_image(path):
        return True
    suffix = path.suffix.casefold()
    if suffix not in MATERIAL_DOCUMENT_SUFFIXES or not path.is_file():
        return False
    try:
        with path.open("rb") as stream:
            header = stream.read(8)
    except OSError:
        return False
    if suffix == ".pdf":
        return header.startswith(b"%PDF-")
    if suffix in {".pptx", ".docx", ".xlsx"}:
        return is_zipfile(path)
    return header == b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"


def _preferred_source(kind: str, source: object) -> bool:
    value = str(source or "").strip()
    if not value.startswith("iyw-image-workflows:"):
        return False
    capability = value.split(":", 1)[1]
    return capability in PREFERRED_CAPABILITIES.get(kind, ())


def preferred_attempted_types(workflow: object) -> dict[str, dict[str, str]]:
    attempts = workflow.get("attempts", []) if isinstance(workflow, dict) else []
    result: dict[str, dict[str, str]] = {}
    for item in attempts:
        if not isinstance(item, dict) or item.get("provider") != "iyw-image-workflows":
            continue
        kind = str(item.get("type") or "")
        alias = str(item.get("alias") or "")
        status = str(item.get("status") or "")
        if alias in PREFERRED_CAPABILITIES.get(kind, ()) and status in FINAL_ATTEMPT_STATUSES:
            result.setdefault(kind, {})[alias] = status
    return result


def material_status(
    materials: object,
    market: str,
    *,
    preferred_attempted: dict[str, dict[str, str]] | None = None,
) -> dict[str, object]:
    target = MATERIAL_TARGETS[market]
    selected: list[dict[str, Any]] = []
    counts: Counter[str] = Counter()
    candidates = sorted(
        (
            item
            for item in _existing_items(materials)
            if str(item.get("source") or "").strip()
            and _material_file_is_usable(item, str(item.get("type") or ""))
        ),
        key=lambda item: not _preferred_source(str(item.get("type") or ""), item.get("source")),
    )
    for item in candidates:
        kind = str(item.get("type") or "")
        if kind in target and counts[kind] < target[kind]:
            preferred = _preferred_source(kind, item.get("source"))
            attempts = (preferred_attempted or {}).get(kind, {})
            if not attempts:
                continue
            if preferred:
                capability = str(item.get("source") or "").split(":", 1)[1]
                if attempts.get(capability) not in {"ok", "complete", "completed", "partial"}:
                    continue
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
        "preferred_actual": {
            kind: sum(1 for item in selected if item.get("type") == kind and _preferred_source(kind, item.get("source")))
            for kind in target
        },
    }
