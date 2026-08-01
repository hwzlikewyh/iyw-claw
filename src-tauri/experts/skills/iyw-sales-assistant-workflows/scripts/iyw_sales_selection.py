from __future__ import annotations

from collections import Counter
from pathlib import Path
from typing import Any

from iyw_sales_layout import MATERIAL_TARGETS


def _existing_items(items: object) -> list[dict[str, Any]]:
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


def product_status(products: object) -> dict[str, object]:
    selected = sorted(_existing_items(products), key=_product_priority)[:10]
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
