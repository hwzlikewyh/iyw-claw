from __future__ import annotations

import calendar
from datetime import datetime
from pathlib import Path
from typing import Any

from iyw_sales_office import _write_batch
from iyw_sales_office_layout import ACCENT, FONT, HEADER, display
from iyw_sales_validation import parse_datetime

SHEET_NAME, DATA_START_ROW, PREVIEW_COUNT = "今日推荐公司", 4, 1
MAX_TEXT_LENGTH = 32767
ROW_HEIGHT = 180
PRODUCT_IMAGE_WIDTH, PRODUCT_IMAGE_HEIGHT = "2.30in", "1.50in"
HEADERS = (
    "公司名",
    "前三联系及角色",
    "前三联系人电话",
    "基本信息(工商信息)",
    "产品图",
    "市场关键词",
    "店铺",
    "近半年招聘/知识产权/参展情况",
    "准备资料",
    "针对性开场白",
)
WIDTHS = (24, 28, 22, 42, 30, 22, 42, 54, 22, 48)
COLUMNS = "ABCDEFGHIJ"
BUSINESS_FIELDS = (
    ("unified_social_credit_code", "统一社会信用代码"),
    ("legal_representative", "法定代表人"),
    ("registration_number", "工商注册号"),
    ("organization_code", "组织机构代码"),
    ("registered_capital", "注册资本"),
    ("paid_in_capital", "实缴资本"),
    ("company_type", "公司类型"),
    ("industry", "所属行业"),
    ("approval_date", "核准日期"),
    ("business_period", "营业期限"),
    ("english_name", "英文名"),
)
ACTIVITY_GROUPS = {
    "招聘": {"recruitment", "sales_hiring", "designer_hiring"},
    "知识产权": {"copyright_work", "trademark", "patent"},
    "参展": {"exhibition"},
}


def _clip(value: object, limit: int = MAX_TEXT_LENGTH) -> str:
    text = display(value).strip()
    if len(text) <= limit:
        return text
    return f"{text[: limit - 3]}..."


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
    phones: list[str] = []
    for contact in contacts[:3]:
        identity = " / ".join(filter(None, (str(contact.get("name") or ""), str(contact.get("role") or ""))))
        people.append(identity or "联系人待补")
        phones.append(str(contact.get("phone") or "电话待补"))
    return "、".join(people), "、".join(phones)


def _business_info(company: dict[str, Any]) -> str:
    value = company.get("business_info")
    info = value if isinstance(value, dict) else {}
    lines = [
        f"{label}：{info.get(key) or company.get(key) or '未提供'}"
        for key, label in BUSINESS_FIELDS
    ]
    return _clip("\n".join(lines))


def _store_text(company: dict[str, Any], products: list[dict[str, Any]]) -> str:
    shop = company.get("shop_url") or next((item.get("store_url") for item in products if item.get("store_url")), None)
    lines = [f"店铺：{shop or '未提供'}"]
    links: list[str] = []
    for product in products:
        value = str(product.get("product_url") or "").strip()
        if value and value not in links:
            links.append(value)
    lines.extend(f"商品：{value}" for value in links)
    return _clip("\n".join(lines))


def _market_text(record: dict[str, Any], analysis: dict[str, str]) -> str:
    values: list[object] = [record["run"].get("market_keywords")]
    if analysis["market"] != "未提供":
        values.extend(analysis["market"].replace("，", "、").replace(",", "、").split("、"))
    return _unique_text(values)


def _subtract_months(value: datetime, months: int) -> datetime:
    index = value.year * 12 + value.month - 1 - months
    year, month_index = divmod(index, 12)
    month = month_index + 1
    day = min(value.day, calendar.monthrange(year, month)[1])
    return value.replace(year=year, month=month, day=day)


def _activity_text(item: dict[str, Any]) -> str:
    detail = item.get("evidence") or item.get("job_title") or item.get("event_name") or item.get("name")
    date = str(item.get("observed_at") or "")[:10]
    return f"{detail or '有记录'}（{date or '日期未知'}）"


def _activity_entries(record: dict[str, Any]) -> dict[str, dict[str, list[dict[str, Any]]]]:
    as_of = parse_datetime(str(record["run"]["as_of"]))
    six_months = _subtract_months(as_of, 6)
    one_year = _subtract_months(as_of, 12)
    grouped = {name: {"six": [], "year": []} for name in ACTIVITY_GROUPS}
    for item in record.get("activities", []):
        if not isinstance(item, dict) or not item.get("source"):
            continue
        try:
            observed = parse_datetime(str(item.get("observed_at")))
        except (TypeError, ValueError):
            continue
        group = next((name for name, types in ACTIVITY_GROUPS.items() if item.get("type") in types), None)
        if not group or observed > as_of or observed < one_year:
            continue
        grouped[group]["six" if observed >= six_months else "year"].append(item)
    for values in grouped.values():
        values["six"].sort(key=lambda item: str(item.get("observed_at") or ""), reverse=True)
        values["year"].sort(key=lambda item: str(item.get("observed_at") or ""), reverse=True)
    return grouped


def activity_display(record: dict[str, Any]) -> str:
    lines: list[str] = []
    for group, values in _activity_entries(record).items():
        selected = values["six"]
        prefix = ""
        if not selected and values["year"]:
            selected = values["year"]
            prefix = "近一年补充："
        detail = "、".join(_activity_text(item) for item in selected[:3]) if selected else "无有效记录"
        lines.append(f"{group}：{prefix}{detail}")
    return "\n".join(lines)


def _folder_uri(path: Path, child: str) -> str:
    return (path / child).resolve().as_uri()


def _company_values(item: dict[str, Any], index: int) -> tuple[list[str], dict[int, str]]:
    plan = item["plan"]
    record = plan["record"]
    products = plan["products"]["selected"]
    analysis = _analysis_fields(products)
    contacts, phones = _contact_fields(plan["contacts"]["selected"])
    company = record["company"]
    opening = (record.get("outreach") or {}).get("opening_copy")
    ppt_path = Path(item["ppt_path"]) if item.get("ppt_path") else None
    materials_missing = any(plan["materials"].get("missing", {}).values())
    values = [
        _clip(company.get("name")),
        contacts if contacts != "未提供" else "联系人待补",
        phones if phones != "未提供" else "电话待补",
        _business_info(company),
        "打开产品图片" if item.get("preview_images") else "产品图待补",
        _market_text(record, analysis),
        _store_text(company, products),
        activity_display(record),
        ("打开公司PPT" + ("（资料部分待补）" if materials_missing else "")) if ppt_path else "PPT待补",
        _clip(opening or "开场白待补"),
    ]
    folder = item["company_dir"]
    links = {4: _folder_uri(folder, "产品图片")}
    if ppt_path:
        links[8] = ppt_path.resolve().as_uri()
    shop = str(company.get("shop_url") or next((item.get("store_url") for item in products if item.get("store_url")), "")).strip()
    if shop.startswith("https://"):
        links[6] = shop
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
    plan = item["plan"]
    company_name = plan.get("name") or plan["record"]["company"].get("name") or f"公司{index}"
    row_height = max(ROW_HEIGHT, min(360, 18 * max(_line_count(value) for value in values) + 24))
    commands: list[dict[str, object]] = [{"command": "set", "path": f"/{SHEET_NAME}/row[{row}]", "props": {"height": row_height}}]
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
                    "anchorMode": "oneCell",
                    "x": 4,
                    "y": row - 1,
                    "width": PRODUCT_IMAGE_WIDTH,
                    "height": PRODUCT_IMAGE_HEIGHT,
                    "name": f"公司{index}-产品{offset + 1}",
                    "alt": f"{company_name}代表产品图{offset + 1}",
                },
            }
        )
    return commands


def _line_count(value: object) -> int:
    lines = str(value or "").splitlines() or [""]
    return sum(max(1, (len(line) + 19) // 20) for line in lines)


def batch_workbook_commands(
    companies: list[dict[str, Any]], generated_at: datetime
) -> list[dict[str, object]]:
    end_row = max(DATA_START_ROW, DATA_START_ROW + len(companies) - 1)
    commands: list[dict[str, object]] = [
        {"command": "set", "path": "/Sheet1", "props": {"name": SHEET_NAME}},
        {"command": "set", "path": f"/{SHEET_NAME}", "props": {"freeze": "A4", "autoFilter": f"A3:J{end_row}", "orientation": "landscape", "fitToPage": "1x0"}},
        _cell(f"/{SHEET_NAME}/A1", f"今日推荐公司（{generated_at:%Y-%m-%d}）", merge="A1:J1", fill=ACCENT, **{"font.color": "FFFFFF", "font.bold": True, "font.size": "18pt"}),
        {"command": "set", "path": f"/{SHEET_NAME}/row[1]", "props": {"height": 28}},
    ]
    commands.extend(_header_commands())
    for index, company in enumerate(companies, 1):
        commands.extend(_company_commands(company, index))
    if not companies:
        commands.append(_cell(f"/{SHEET_NAME}/A4", "今日无推荐公司", merge="A4:J4", **{"font.color": "666666", "font.italic": True}))
    for index, width in enumerate(WIDTHS):
        commands.append({"command": "set", "path": f"/{SHEET_NAME}/col[{COLUMNS[index]}]", "props": {"width": width}})
    return commands

def create_batch_recommendation_workbook(
    path: Path,
    companies: list[dict[str, Any]],
    generated_at: datetime,
) -> None:
    _write_batch(path, batch_workbook_commands(companies, generated_at))
