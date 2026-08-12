from __future__ import annotations

import json
import os
import subprocess
from datetime import datetime
from pathlib import Path
from typing import Any

from iyw_sales_labels import (
    ACTION_LABELS,
    ACTION_STATUS_LABELS,
    ACTIVITY_LABELS,
    DECISION_LABELS,
    MARKET_LABELS,
    MATERIAL_LABELS,
    PACKAGE_STATUS_LABELS,
    SECTION_LABELS,
)
from iyw_sales_office_layout import (
    ACCENT,
    FONT,
    HEADER,
    SheetSpec,
)
from iyw_sales_office_layout import (
    display as _display,
)
from iyw_sales_office_layout import (
    sheet_commands as _sheet_commands,
)
from iyw_sales_office_preview import OfficePreviewError, fallback_screenshot

OFFICE_DELIVERABLES = (
    "01-客户信息/客户档案.xlsx",
    "02-联系方式/优先联系人.xlsx",
    "04-企业信息与评分证据/评分与来源.xlsx",
    "06-销售话术/销售跟进建议.docx",
    "07-待办/销售待办与缺项.xlsx",
)
class OfficeCliError(OSError):
    pass



def _date_display(value: object, fallback: datetime) -> str:
    text = str(value or "").strip()
    return text[:10] if len(text) >= 10 else fallback.strftime("%Y-%m-%d")


def _run(args: list[str], *, input_text: str | None = None) -> str:
    command = ["officecli", *args]
    environment = os.environ.copy()
    environment["OFFICECLI_NO_AUTO_RESIDENT"] = "1"
    try:
        result = subprocess.run(
            command,
            input=input_text,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            check=False,
            env=environment,
        )
    except FileNotFoundError as exc:
        raise OfficeCliError("生成销售资料需要安装 officecli") from exc
    if result.returncode:
        detail = (result.stderr or result.stdout).strip()
        raise OfficeCliError(f"officecli 执行失败（{' '.join(args[:2])}）：{detail}")
    return result.stdout


def _write_batch(path: Path, commands: list[dict[str, object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    _run(["create", str(path)])
    _run(["batch", str(path)], input_text=json.dumps(commands, ensure_ascii=False))
    _run(["close", str(path)])
    _verify(path)


def _verify_html(path: Path) -> None:
    preview = path.with_name(f".{path.stem}-preview.html")
    try:
        _run(["view", str(path), "html", "-o", str(preview)])
        if not preview.is_file() or not preview.stat().st_size:
            raise OfficeCliError(f"officecli 未生成 {path.name} 的 HTML 预览")
        content = preview.read_text(encoding="utf-8", errors="replace")
        markers = ("###", "$fy$", "{var}", "<TODO>")
        leaked = next((marker for marker in markers if marker in content), None)
        if leaked:
            raise OfficeCliError(f"{path.name} 的 HTML 预览包含异常标记：{leaked}")
    finally:
        preview.unlink(missing_ok=True)


def _verify(path: Path) -> None:
    _run(["validate", str(path)])
    issues = _run(["view", str(path), "issues"])
    normalized = issues.casefold()
    issue_markers = ("error:", "overflow", "overlap", "clipped", "truncated")
    if any(marker in normalized for marker in issue_markers):
        raise OfficeCliError(f"{path.name} 的布局检查未通过：{issues.strip()}")
    preview = path.with_name(f".{path.stem}-preview.png")
    preview_available = True
    args = ["view", str(path), "screenshot"]
    if path.suffix.casefold() == ".docx":
        args.extend(["--grid", "auto"])
    args.extend(["-o", str(preview)])
    try:
        try:
            output = _run(args)
            if "No headless browser available" in output:
                raise OfficeCliError(output.strip())
        except OfficeCliError as error:
            if "No headless browser available" not in str(error):
                raise
            try:
                fallback_screenshot(path, preview, _run)
            except OfficePreviewError as fallback_error:
                if "浏览器" not in str(fallback_error):
                    raise
                _verify_html(path)
                preview_available = False
        if preview_available and (
            not preview.is_file() or preview.stat().st_size == 0
        ):
            raise OfficeCliError(f"officecli 未生成 {path.name} 的预览图")
    finally:
        _run(["close", str(path)])
        preview.unlink(missing_ok=True)


def _workbook(path: Path, sheets: tuple[SheetSpec, ...]) -> None:
    commands: list[dict[str, object]] = [
        {"command": "set", "path": "/Sheet1", "props": {"name": sheets[0].name}}
    ]
    for sheet in sheets[1:]:
        commands.append(
            {"command": "add", "parent": "/", "type": "sheet", "props": {"name": sheet.name}}
        )
    for sheet in sheets:
        commands.extend(_sheet_commands(sheet))
    _write_batch(path, commands)


def _overview(record: dict[str, Any], result: dict[str, Any], generated_at: datetime) -> SheetSpec:
    company = record["company"]
    run = record["run"]
    rows = (
        ("公司全称", company.get("name"), "负责销售", run.get("salesperson")),
        ("曾用名", company.get("aliases"), "市场", MARKET_LABELS.get(str(run.get("market")), run.get("market"))),
        ("来源平台", company.get("platform"), "数据日期", _date_display(run.get("as_of"), generated_at)),
        ("店铺地址", company.get("shop_url"), "产品关键词", run.get("product_keywords")),
        ("励销企业编号", company.get("lixiao_id"), "综合评分", result["evaluation"]["score"].get("total")),
        ("客户管理系统状态", DECISION_LABELS.get(str(result["evaluation"]["crm_decision"].get("decision")), "待复核"), "客户包状态", PACKAGE_STATUS_LABELS.get(str(result.get("status")), "待复核")),
        ("产品图片", f"{result['products']['actual']} / {result['products']['target']}", "优先联系人", f"{result['contacts']['actual']} / {result['contacts']['target']}"),
    )
    return SheetSpec("客户概览", f"{company['name']} - 客户档案", ("项目", "内容", "项目", "内容"), rows, (18, 42, 18, 42))


def _contacts(record: dict[str, Any], result: dict[str, Any]) -> SheetSpec:
    rows = tuple(
        (
            index,
            item.get("name"),
            item.get("role"),
            item.get("phone"),
            item.get("email"),
            item.get("public_account"),
            item.get("source"),
            item.get("observed_at"),
        )
        for index, item in enumerate(result["contacts"]["selected"], 1)
    )
    return SheetSpec("优先联系人", f"{record['company']['name']} - 优先联系人", ("序号", "姓名", "职务", "电话", "邮箱", "公众号", "来源", "核验时间"), rows, (8, 16, 18, 18, 28, 20, 28, 22), "暂无可用联系人")


def _score_sheet(record: dict[str, Any], result: dict[str, Any]) -> SheetSpec:
    score = result["evaluation"]["score"]
    breakdown = score.get("breakdown") or {}
    accepted = score.get("accepted") or {}
    rows = [
        (ACTIVITY_LABELS.get(key, key), value, len(accepted.get(key, [])), "近六个月有效证据")
        for key, value in breakdown.items()
    ]
    rows.append(("总分", score.get("total"), sum(len(items) for items in accepted.values()), "满分 50 分"))
    return SheetSpec("评分", f"{record['company']['name']} - 客户评分", ("评分项目", "分值", "证据数量", "说明"), tuple(rows), (24, 12, 14, 32))


def _source_sheet(record: dict[str, Any]) -> SheetSpec:
    rows: list[tuple[object, ...]] = []
    for section in ("activities", "products", "contacts", "materials"):
        for item in record.get(section, []):
            if not isinstance(item, dict) or not item.get("source"):
                continue
            detail = item.get("evidence") or item.get("name")
            if not detail and item.get("local_path"):
                detail = Path(str(item["local_path"])).name
            rows.append((len(rows) + 1, SECTION_LABELS[section], item["source"], detail, item.get("observed_at")))
    return SheetSpec("来源", f"{record['company']['name']} - 信息来源", ("序号", "资料类别", "来源", "内容摘要", "观察时间"), tuple(rows), (8, 18, 34, 44, 22), "暂无来源记录")


def _todo_sheets(record: dict[str, Any], result: dict[str, Any]) -> tuple[SheetSpec, SheetSpec]:
    actions = tuple(
        (index, ACTION_LABELS.get(str(item.get("type")), item.get("type")), ACTION_STATUS_LABELS.get(str(item.get("status")), item.get("status")), item.get("salesperson"))
        for index, item in enumerate(result["pending_actions"], 1)
    )
    gaps: list[tuple[object, ...]] = [
        ("产品图片", result["products"]["target"], result["products"]["actual"], result["products"]["missing"]),
        ("优先联系人", result["contacts"]["target"], result["contacts"]["actual"], result["contacts"]["missing"]),
    ]
    for kind, missing in result["materials"]["missing"].items():
        gaps.append((MATERIAL_LABELS.get(kind, kind), result["materials"]["target"][kind], result["materials"]["actual"][kind], missing))
    company = record["company"]["name"]
    return (
        SheetSpec("销售待办", f"{company} - 销售待办", ("序号", "待办事项", "状态", "负责销售"), actions, (8, 34, 16, 18), "暂无待办事项"),
        SheetSpec("资料缺项", f"{company} - 资料完整度", ("资料类别", "目标数量", "实际数量", "缺少数量"), tuple(gaps), (28, 14, 14, 14)),
    )


def _document_commands(record: dict[str, Any], result: dict[str, Any], generated_at: datetime) -> list[dict[str, object]]:
    outreach = record.get("outreach") or {}
    opening = outreach.get("opening_copy") or "暂未生成开场话术。"
    ideas = outreach.get("social_ideas") or []
    company = record["company"]["name"]
    run = record["run"]
    commands: list[dict[str, object]] = [
        {"command": "set", "path": "/", "props": {"docDefaults.font": FONT, "docDefaults.fontSize": "11pt", "marginTop": "1.8cm", "marginBottom": "1.8cm", "marginLeft": "2cm", "marginRight": "2cm"}},
        {"command": "add", "parent": "/body", "type": "paragraph", "props": {"text": f"{company} - 销售跟进建议", "style": "Title", "font": FONT, "size": "22pt", "bold": True, "color": ACCENT, "spaceAfter": "12pt"}},
        {"command": "add", "parent": "/body", "type": "paragraph", "props": {"text": f"负责销售：{_display(run.get('salesperson'))}    市场：{_display(MARKET_LABELS.get(str(run.get('market')), run.get('market')))}    生成日期：{generated_at:%Y-%m-%d}", "font": FONT, "size": "10pt", "color": "666666", "spaceAfter": "10pt"}},
        {"command": "add", "parent": "/body", "type": "paragraph", "props": {"text": f"客户评分：{result['evaluation']['score'].get('total')}    客户管理系统状态：{DECISION_LABELS.get(str(result['evaluation']['crm_decision'].get('decision')), '待复核')}", "font": FONT, "size": "11pt", "fill": HEADER, "spaceAfter": "12pt"}},
        {"command": "add", "parent": "/body", "type": "paragraph", "props": {"text": "开场话术", "style": "Heading1", "font": FONT, "size": "18pt", "bold": True, "color": ACCENT, "spaceBefore": "10pt", "spaceAfter": "6pt"}},
        {"command": "add", "parent": "/body", "type": "paragraph", "props": {"text": _display(opening), "style": "Normal", "font": FONT, "size": "11pt", "lineSpacing": "1.3x", "spaceAfter": "12pt"}},
        {"command": "add", "parent": "/body", "type": "paragraph", "props": {"text": "内容选题", "style": "Heading1", "font": FONT, "size": "18pt", "bold": True, "color": ACCENT, "spaceBefore": "10pt", "spaceAfter": "6pt"}},
    ]
    if ideas:
        for idea in ideas:
            commands.append({"command": "add", "parent": "/body", "type": "paragraph", "props": {"text": _display(idea), "style": "Normal", "font": FONT, "size": "11pt", "listStyle": "bullet", "spaceAfter": "4pt"}})
    else:
        commands.append({"command": "add", "parent": "/body", "type": "paragraph", "props": {"text": "暂未生成内容选题。", "style": "Normal", "font": FONT, "size": "11pt"}})
    commands.append({"command": "add", "parent": "/", "type": "footer", "props": {"type": "default", "field": "page", "align": "center", "font": FONT, "size": "9pt", "color": "666666"}})
    return commands


def create_office_deliverables(
    package: Path,
    record: dict[str, Any],
    result: dict[str, Any],
    generated_at: datetime,
) -> list[str]:
    _workbook(package / OFFICE_DELIVERABLES[0], (_overview(record, result, generated_at),))
    _workbook(package / OFFICE_DELIVERABLES[1], (_contacts(record, result),))
    _workbook(package / OFFICE_DELIVERABLES[2], (_score_sheet(record, result), _source_sheet(record)))
    document = package / OFFICE_DELIVERABLES[3]
    _write_batch(document, _document_commands(record, result, generated_at))
    _workbook(package / OFFICE_DELIVERABLES[4], _todo_sheets(record, result))
    return list(OFFICE_DELIVERABLES)
