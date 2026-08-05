from __future__ import annotations

import argparse
import calendar
import json
import os
import re
import sys
from datetime import datetime
from pathlib import Path

from iyw_sales_batch import build_batch_package as _build_batch_package
from iyw_sales_images import download_product_images
from iyw_sales_package import build_package as _build_package
from iyw_sales_validation import ValidationError, parse_datetime, validate_record

ACTIVITY_TYPES = (
    "sales_hiring",
    "designer_hiring",
    "exhibition",
    "shop_update",
    "copyright_work",
)
PROTECTED_STARS = {4, 5, 10}
PACKAGE_FOLDER_NAME = "AI销售助理客户包"


def _windows_desktop_directory() -> Path | None:
    try:
        import winreg

        key_path = (
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders"
        )
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key_path) as key:
            value, _ = winreg.QueryValueEx(key, "Desktop")
    except (ImportError, OSError):
        return None
    expanded = os.path.expandvars(str(value)).strip()
    return Path(expanded) if expanded else None


def default_output_root() -> Path:
    desktop = _windows_desktop_directory() if sys.platform == "win32" else None
    return (desktop or Path.home() / "Desktop") / PACKAGE_FOLDER_NAME


def default_batch_output_root() -> Path:
    desktop = _windows_desktop_directory() if sys.platform == "win32" else None
    return desktop or Path.home() / "Desktop"


def subtract_months(value: datetime, months: int) -> datetime:
    month_index = value.year * 12 + value.month - 1 - months
    year, zero_based_month = divmod(month_index, 12)
    month = zero_based_month + 1
    day = min(value.day, calendar.monthrange(year, month)[1])
    return value.replace(year=year, month=month, day=day)


def _try_datetime(value: object) -> datetime | None:
    if not isinstance(value, str) or not value.strip():
        return None
    try:
        return parse_datetime(value)
    except ValidationError:
        return None


def score_activities(
    activities: list[dict[str, object]], as_of: str
) -> dict[str, object]:
    end = parse_datetime(as_of)
    start = subtract_months(end, 6)
    accepted: dict[str, list[dict[str, object]]] = {
        kind: [] for kind in ACTIVITY_TYPES
    }
    for activity in activities:
        if not isinstance(activity, dict):
            continue
        kind = activity.get("type")
        observed = _try_datetime(activity.get("observed_at"))
        if kind not in accepted or not activity.get("source") or observed is None:
            continue
        if start <= observed <= end:
            accepted[str(kind)].append(activity)
    breakdown = {kind: 10 if accepted[kind] else 0 for kind in ACTIVITY_TYPES}
    return {
        "total": sum(breakdown.values()),
        "breakdown": breakdown,
        "accepted": accepted,
    }


def _parse_chinese_star(value: str) -> int | None:
    names = {
        "零": 0,
        "一": 1,
        "二": 2,
        "三": 3,
        "四": 4,
        "五": 5,
        "六": 6,
        "七": 7,
        "八": 8,
        "九": 9,
        "十": 10,
    }
    for name, number in names.items():
        if f"{name}星" in value:
            return number
    return None


def parse_star(value: object, star_name: object = None) -> int | None:
    for candidate in (value, star_name):
        if isinstance(candidate, bool) or candidate is None:
            continue
        if isinstance(candidate, (int, float)) and int(candidate) == candidate:
            return int(candidate)
        text = str(candidate).strip()
        match = re.fullmatch(r"(10|[0-9])\s*星?", text)
        if match:
            return int(match.group(1))
        chinese = _parse_chinese_star(text)
        if chinese is not None:
            return chinese
    return None


def decision(name: str, eligible: bool) -> dict[str, object]:
    return {"decision": name, "eligible": eligible, "reason": name}


def decide_crm(crm: dict[str, object]) -> dict[str, object]:
    match_status = crm.get("match_status")
    if match_status == "failed":
        return decision("crm_unverified", False)
    if match_status == "ambiguous":
        return decision("crm_ambiguous", False)
    if match_status == "not_found":
        return decision("eligible_new", True)
    if match_status != "matched":
        return decision("crm_review", False)
    star = parse_star(crm.get("star"), crm.get("star_name"))
    if star is None:
        return decision("crm_review", False)
    if star in PROTECTED_STARS:
        return decision("skip_protected_star", False)
    if str(crm.get("owner") or "").strip():
        return decision("skip_owned", False)
    return decision("eligible_unowned", True)


def evaluate_record(
    record: object, as_of: str | None = None
) -> dict[str, object]:
    validated = validate_record(record)
    run = validated["run"]
    effective_as_of = as_of or run["as_of"]
    return {
        "score": score_activities(validated["activities"], effective_as_of),
        "crm_decision": decide_crm(validated["crm"]),
    }


def build_package(
    record: object,
    output_root: str | Path,
    *,
    dry_run: bool = False,
    now: datetime | None = None,
) -> dict[str, object]:
    return _build_package(
        record, output_root, evaluate_record, dry_run=dry_run, now=now
    )


def build_batch_package(
    records: object,
    output_root: str | Path,
    *,
    dry_run: bool = False,
    now: datetime | None = None,
) -> dict[str, object]:
    return _build_batch_package(
        records, output_root, evaluate_record, dry_run=dry_run, now=now
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Evaluate and package IYW sales leads")
    subparsers = parser.add_subparsers(dest="command", required=True)
    evaluate = subparsers.add_parser("evaluate", help="score and apply CRM gates")
    evaluate.add_argument("--input", required=True, help="lead JSON path or -")
    download = subparsers.add_parser(
        "download-products", help="download product images for local analysis"
    )
    download.add_argument("--input", required=True, help="lead JSON path or -")
    download.add_argument("--output-dir", required=True)
    download.add_argument("--limit", type=int, default=10)
    download.add_argument("--force", action="store_true")
    package = subparsers.add_parser("package", help="create a customer package")
    package.add_argument("--input", required=True, help="lead JSON path or -")
    package.add_argument(
        "--output-root",
        default=str(default_output_root()),
        help="defaults to the current user's Desktop/AI销售助理客户包 folder",
    )
    package.add_argument("--dry-run", action="store_true")
    batch = subparsers.add_parser(
        "batch-package", help="create one workbook for a batch of recommended companies"
    )
    batch.add_argument("--input", required=True, help="batch JSON path or -")
    batch.add_argument(
        "--output-root",
        default=str(default_batch_output_root()),
        help="defaults to the current user's Desktop folder",
    )
    batch.add_argument("--dry-run", action="store_true")
    return parser


def _read_input(value: str) -> object:
    content = sys.stdin.read() if value == "-" else Path(value).read_text("utf-8")
    try:
        return json.loads(content)
    except json.JSONDecodeError as error:
        raise ValidationError(f"invalid JSON input: {error.msg}") from error


def _execute(args: argparse.Namespace) -> dict[str, object]:
    record = _read_input(args.input)
    if args.command == "evaluate":
        return evaluate_record(record)
    if args.command == "download-products":
        validated = validate_record(record)
        result = download_product_images(
            validated.get("products", []),
            args.output_dir,
            limit=args.limit,
            force=args.force,
        )
        return {
            "company": validated["company"]["name"],
            **result,
            "network": True,
            "crm_write": False,
        }
    if args.command == "batch-package":
        if not isinstance(record, dict) or set(record) - {"records", "run"}:
            raise ValidationError("batch input must contain only records and optional run")
        return build_batch_package(
            record.get("records"), args.output_root, dry_run=args.dry_run
        )
    return build_package(
        record, args.output_root, dry_run=args.dry_run
    )


def _response_error(error: Exception) -> dict[str, object]:
    return {
        "ok": False,
        "error": {
            "code": "validation_error" if isinstance(error, ValidationError) else "io_error",
            "message": str(error),
            "retryable": False,
        },
    }


def main(argv: list[str] | None = None) -> int:
    try:
        data = _execute(build_parser().parse_args(argv))
        response: dict[str, object] = {"ok": True, "data": data}
        exit_code = 0
    except (ValidationError, OSError) as error:
        response = _response_error(error)
        exit_code = 2
    print(json.dumps(response, ensure_ascii=False))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
