from __future__ import annotations

from datetime import datetime
from pathlib import Path
from typing import Any

from iyw_sales_office import _write_batch
from iyw_sales_office_layout import ACCENT, FONT, HEADER, display

SHEET_NAME = "今日推荐公司"
DATA_START_ROW = 4
PREVIEW_COUNT = 3
MAX_TEXT_LENGTH = 240
ROW_HEIGHT = 76
HEADERS = (
    "序号",
    "公司名称",
    "平台与店铺",
    "主推产品",
    "产品分析",
    "适合市场",
    "销售切入点",
    "联系人",
    "联系方式",
    "产品图片",
    "销售资料",
    "状态",
    "产品预览1",
    "产品预览2",
    "产品预览3",
)
WIDTHS = (8, 24, 24, 26, 42, 20, 36, 24, 28, 18, 18, 22, 16, 16, 16)
COLUMNS = "ABCDEFGHIJKLMNO"


def _clip(value: object) -> str:
    text = display(value).strip()
    if len(text) <= MAX_TEXT_LENGTH:
        return text
    return f"{text[: MAX_TEXT_LENGTH - 1]}..."


def _unique_text(values: list[object]) -> str:
    result: list[str] = []
    for value in values:
        items = value if isinstance(value, (list, tuple)) else [value]
        for item in items:
            text = str(item or "").strip()
            if text and text not in result:
                result.append(text)
    return _clip("、".join(result) if result else "未提供")


def _analysis_fields(products: list[dict[str, Any]]) -> dict[str, str]:
    summaries: list[object] = []
    markets: list[object] = []
    angles: list[object] = []
    for product in products:
        analysis = product.get("analysis")
        if isinstance(analysis, str):
            summaries.append(analysis)
            continue
        if not isinstance(analysis, dict):
            continue
        summaries.extend([analysis.get("summary"), analysis.get("selling_points")])
        markets.append(analysis.get("target_market"))
        angles.append(analysis.get("sales_angle"))
    return {
        "summary": _unique_text(summaries),
        "market": _unique_text(markets),
        "angle": _unique_text(angles),
    }


def _contact_fields(contacts: list[dict[str, Any]]) -> tuple[str, str]:
    people: list[str] = []
    details: list[str] = []
    for contact in contacts:
        people.append(" / ".join(filter(None, (str(contact.get("name") or ""), str(contact.get("role") or "")))))
        values = [contact.get("phone"), contact.get("email"), contact.get("public_account")]
        detail = " / ".join(str(value) for value in values if value)
        if detail:
            details.append(detail)
    return _unique_text(people), _unique_text(details)


def _status_text(plan: dict[str, Any]) -> str:
    missing: list[str] = []
    if not plan["products"]["actual"]:
        missing.append("图片")
    if not plan["analysis_available"]:
        missing.append("分析")
    if plan["contacts"]["missing"]:
        missing.append("联系人")
    if any(plan["materials"]["missing"].values()):
        missing.append("销售资料")
    return "可跟进" if not missing else f"待补：{'、'.join(missing)}"


def _folder_uri(path: Path, child: str) -> str:
    return (path / child).resolve().as_uri()


def _company_values(item: dict[str, Any], index: int) -> tuple[list[str], dict[int, str]]:
    plan = item["plan"]
    record = plan["record"]
    products = plan["products"]["selected"]
    analysis = _analysis_fields(products)
    contacts, details = _contact_fields(plan["contacts"]["selected"])
    company = record["company"]
    opening = (record.get("outreach") or {}).get("opening_copy")
    angle = analysis["angle"] if analysis["angle"] != "未提供" else _clip(opening)
    values = [
        str(index),
        _clip(company.get("name")),
        _unique_text([company.get("platform"), company.get("shop_url")]),
        _unique_text([product.get("name") for product in products]),
        analysis["summary"],
        analysis["market"],
        angle,
        contacts,
        details,
        "打开产品图片",
        "打开销售资料",
        _status_text(plan),
        *("" for _ in range(PREVIEW_COUNT)),
    ]
    folder = item["company_dir"]
    links = {9: _folder_uri(folder, "产品图片"), 10: _folder_uri(folder, "销售资料")}
    shop = str(company.get("shop_url") or "").strip()
    if shop.startswith("https://"):
        links[2] = shop
    return values, links


def _cell(path: str, value: object, **props: object) -> dict[str, object]:
    return {
        "command": "set",
        "path": path,
        "props": {"value": value, "font.name": FONT, **props},
    }


def _header_commands() -> list[dict[str, object]]:
    commands: list[dict[str, object]] = []
    for index, header in enumerate(HEADERS):
        commands.append(
            _cell(
                f"/{SHEET_NAME}/{COLUMNS[index]}3",
                header,
                fill=HEADER,
                **{"font.bold": True, "alignment.wrapText": True},
            )
        )
    return commands


def _company_commands(item: dict[str, Any], index: int) -> list[dict[str, object]]:
    row = DATA_START_ROW + index - 1
    values, links = _company_values(item, index)
    commands: list[dict[str, object]] = [
        {"command": "set", "path": f"/{SHEET_NAME}/row[{row}]", "props": {"height": ROW_HEIGHT}}
    ]
    for column, value in enumerate(values):
        props: dict[str, object] = {
            "alignment.wrapText": True,
            "alignment.vertical": "center",
        }
        if column in links:
            props.update({"link": links[column], "font.color": "0563C1", "font.underline": "single"})
        commands.append(_cell(f"/{SHEET_NAME}/{COLUMNS[column]}{row}", value, **props))
    for offset, image in enumerate(item["preview_images"][:PREVIEW_COUNT]):
        commands.append(
            {
                "command": "add",
                "parent": f"/{SHEET_NAME}",
                "type": "picture",
                "props": {
                    "src": image,
                    "x": 12 + offset,
                    "y": row - 1,
                    "width": 1,
                    "height": 1,
                    "name": f"公司{index}-产品{offset + 1}",
                    "alt": f"{item['plan']['name']}代表产品图{offset + 1}",
                },
            }
        )
    return commands


def batch_workbook_commands(
    companies: list[dict[str, Any]], generated_at: datetime
) -> list[dict[str, object]]:
    end_row = max(DATA_START_ROW, DATA_START_ROW + len(companies) - 1)
    commands: list[dict[str, object]] = [
        {"command": "set", "path": "/Sheet1", "props": {"name": SHEET_NAME}},
        {"command": "set", "path": f"/{SHEET_NAME}", "props": {"freeze": "A4", "autoFilter": f"A3:L{end_row}", "orientation": "landscape", "fitToPage": "1x0"}},
        _cell(f"/{SHEET_NAME}/A1", f"今日推荐公司（{generated_at:%Y-%m-%d}）", merge="A1:O1", fill=ACCENT, **{"font.color": "FFFFFF", "font.bold": True, "font.size": "18pt"}),
        {"command": "set", "path": f"/{SHEET_NAME}/row[1]", "props": {"height": 28}},
    ]
    commands.extend(_header_commands())
    for index, company in enumerate(companies, 1):
        commands.extend(_company_commands(company, index))
    if not companies:
        commands.append(_cell(f"/{SHEET_NAME}/A4", "今日无推荐公司", merge="A4:O4", **{"font.color": "666666", "font.italic": True}))
    for index, width in enumerate(WIDTHS):
        commands.append({"command": "set", "path": f"/{SHEET_NAME}/col[{COLUMNS[index]}]", "props": {"width": width}})
    return commands


def create_batch_recommendation_workbook(
    path: Path,
    companies: list[dict[str, Any]],
    generated_at: datetime,
) -> None:
    _write_batch(path, batch_workbook_commands(companies, generated_at))
