from __future__ import annotations

import re
from datetime import datetime
from pathlib import Path


MATERIAL_TARGETS = {
    "export": {
        "exhibition_report": 3,
        "trend_theme": 3,
        "retail_image": 20,
        "catalog_image": 20,
    },
    "domestic": {"trend_theme": 5, "pattern_poster": 10, "ai_image": 20},
}
MATERIAL_FOLDERS = {
    "export": {
        "exhibition_report": "01-展会报告-3份",
        "trend_theme": "02-趋势主题-3份",
        "retail_image": "03-卖场图片-20张",
        "catalog_image": "04-目录图片-20张",
    },
    "domestic": {
        "trend_theme": "01-趋势主题-5份",
        "pattern_poster": "02-爆款图案海报-10张",
        "ai_image": "03-AI图片-20张",
    },
}
INVALID_FILENAME = re.compile(r'[<>:"/\\|?*\x00-\x1f]')
RESERVED_NAMES = {"CON", "PRN", "AUX", "NUL"} | {
    f"{prefix}{number}" for prefix in ("COM", "LPT") for number in range(1, 10)
}


def sanitize_path_component(value: str, fallback: str = "未命名") -> str:
    result = INVALID_FILENAME.sub("_", value).rstrip(" .")
    if not result:
        result = fallback
    if result.upper() in RESERVED_NAMES:
        result = f"_{result}"
    return result[:120]


def allocate_package_directory(
    root: Path,
    company_name: str,
    salesperson: str,
    now: datetime,
) -> Path:
    dated_root = root / now.strftime("%Y-%m-%d")
    sales_root = dated_root / sanitize_path_component(salesperson, "当前销售")
    initial = sales_root / sanitize_path_component(company_name, "未命名公司")
    if not initial.exists():
        return initial
    stem = f"{initial.name}-{now.strftime('%Y%m%d-%H%M%S')}"
    candidate = sales_root / stem
    index = 2
    while candidate.exists():
        candidate = sales_root / f"{stem}-{index}"
        index += 1
    return candidate


def material_directory(package: Path, market: str, kind: str) -> Path:
    return package / "05-销售资料" / MATERIAL_FOLDERS[market][kind]


def create_material_directories(package: Path, market: str) -> None:
    for kind in MATERIAL_FOLDERS[market]:
        material_directory(package, market, kind).mkdir(parents=True, exist_ok=True)
