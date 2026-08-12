import sys
from datetime import datetime
from pathlib import Path

import pytest

SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-sales-assistant-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from iyw_sales_batch_office import (  # noqa: E402
    HEADERS,
    _company_values,
    activity_display,
    batch_workbook_commands,
)
from iyw_sales_batch import _render_company  # noqa: E402
from iyw_sales_ppt import build_ppt_input  # noqa: E402
from iyw_sales_validation import validate_record  # noqa: E402
import iyw_sales_office  # noqa: E402
from sales_test_helpers import (  # noqa: E402
    sales_ppt_manifest,
    write_minimal_pptx,
    write_prepared_sales_pptx,
)


def valid_record(as_of: str = "2026-08-07T12:00:00+08:00") -> dict:
    return {
        "company": {"name": "示例公司", "shop_url": "https://example.com/shop"},
        "run": {
            "market": "export",
            "salesperson": "销售甲",
            "as_of": as_of,
            "market_keywords": ["欧美"],
        },
        "activities": [
            {
                "type": "sales_hiring",
                "observed_at": "2026-07-01T09:00:00+08:00",
                "source": "lixiao",
                "evidence": "招聘外贸业务员",
            }
        ],
        "crm": {"match_status": "not_found"},
        "products": [
            {
                "name": "示例产品",
                "local_path": "C:\\staging\\product.png",
                "product_url": "https://example.com/product",
            }
        ],
        "contacts": [],
        "materials": [],
        "outreach": {"opening_copy": "您好，我们针对贵司产品准备了资料。"},
    }


def test_validate_record_accepts_business_market_and_activity_fields():
    record = valid_record()
    record["company"]["business_info"] = {"legal_representative": "张三"}
    record["activities"] += [
        {
            "type": "trademark",
            "observed_at": "2026-05-01T09:00:00+08:00",
            "source": "lixiao",
            "evidence": "商标",
        },
        {
            "type": "patent",
            "observed_at": "2026-04-01T09:00:00+08:00",
            "source": "lixiao",
            "evidence": "专利",
        },
    ]
    validated = validate_record(record)
    assert validated["company"]["business_info"]["legal_representative"] == "张三"


def test_validate_record_rejects_track_result_from_other_company():
    record = valid_record()
    record["track_results"] = [
        {
            "company_key": "其他公司",
            "track": "product_images",
            "status": "completed",
            "missing": [],
        }
    ]
    with pytest.raises(ValueError, match="company_key"):
        validate_record(record)


def test_validate_record_rejects_duplicate_track_results():
    record = valid_record()
    receipt = {
        "company_key": "示例公司",
        "track": "product_images",
        "status": "completed",
        "missing": [],
    }
    record["track_results"] = [receipt, dict(receipt)]
    with pytest.raises(ValueError, match="duplicate track"):
        validate_record(record)


def test_activity_window_uses_one_year_only_when_six_month_category_is_empty():
    record = valid_record()
    record["activities"] = [
        {
            "type": "patent",
            "observed_at": "2026-01-01T09:00:00+08:00",
            "source": "lixiao",
            "evidence": "专利",
        }
    ]
    result = activity_display(record)
    assert "近一年补充" in result
    assert "2026-01-01" in result


def company_item(tmp_path: Path) -> dict:
    record = valid_record()
    record["company"]["business_info"] = {
        "unified_social_credit_code": "913500000000000000",
        "legal_representative": "张三",
        "registered_capital": "128万人民币",
    }
    record["contacts"] = [
        {"name": "李四", "role": "外贸经理", "phone": "13800000000", "source": "lixiao"}
    ]
    record["activities"] = [
        {"type": "patent", "observed_at": "2026-01-01T09:00:00+08:00", "source": "lixiao", "evidence": "外观专利"}
    ]
    products = record["products"]
    products[0]["analysis"] = {"target_market": "欧美礼品市场"}
    return {
        "plan": {
            "record": record,
            "products": {"selected": products, "actual": 1},
            "contacts": {"selected": record["contacts"], "missing": 2},
            "materials": {"missing": {}},
            "analysis_available": True,
        },
        "company_dir": tmp_path / "示例公司",
        "ppt_path": tmp_path / "示例公司" / "准备资料" / "示例公司-销售资料.pptx",
        "preview_images": [],
    }


def test_batch_workbook_uses_requested_ten_headers():
    assert HEADERS == (
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


def test_company_values_fill_sales_summary_and_links(tmp_path):
    values, links = _company_values(company_item(tmp_path), 1)
    assert len(values) == 10
    assert values[1] == "李四 / 外贸经理"
    assert values[2] == "13800000000"
    assert "法定代表人：张三" in values[3]
    assert "欧美" in values[5]
    assert "商品：https://example.com/product" in values[6]
    assert "近一年补充" in values[7]
    assert links[8].endswith(".pptx")


def test_store_url_falls_back_to_product_store_url(tmp_path):
    item = company_item(tmp_path)
    record = item["plan"]["record"]
    record["company"].pop("shop_url")
    record["products"][0]["store_url"] = "https://example.com/product-store"
    values, links = _company_values(item, 1)
    assert "店铺：https://example.com/product-store" in values[6]
    assert links[6] == "https://example.com/product-store"


def test_contact_phone_column_keeps_contact_order(tmp_path):
    item = company_item(tmp_path)
    item["plan"]["contacts"]["selected"] = [
        {"name": "甲", "role": "总经理", "phone": "10086"},
        {"name": "乙", "role": "外贸经理"},
        {"name": "丙", "role": "设计负责人", "phone": "10010"},
    ]
    values, _ = _company_values(item, 1)
    assert values[2] == "10086、电话待补、10010"


def test_contact_columns_keep_placeholder_rows_aligned(tmp_path):
    item = company_item(tmp_path)
    item["plan"]["contacts"]["selected"] = [
        {"phone": "10086"},
        {"name": "乙", "role": "外贸经理"},
    ]
    values, _ = _company_values(item, 1)
    assert values[1] == "联系人待补、乙 / 外贸经理"
    assert values[2] == "10086、电话待补"


def test_workbook_keeps_long_sales_text_in_cell(tmp_path):
    item = company_item(tmp_path)
    opening = "针对性开场白" * 6000
    item["plan"]["record"]["outreach"]["opening_copy"] = opening
    values, _ = _company_values(item, 1)
    assert len(values[9]) == 32767
    assert values[9].startswith("针对性开场白" * 100)
    assert values[9].endswith("...")


def test_product_previews_use_officecli_unit_qualified_geometry(tmp_path):
    item = company_item(tmp_path)
    item["preview_images"] = ["a.png", "b.png", "c.png"]
    commands = batch_workbook_commands([item], datetime(2026, 8, 7))
    pictures = [command for command in commands if command.get("type") == "picture"]
    assert len(pictures) == 1
    for picture in pictures:
        assert picture["props"]["anchorMode"] == "oneCell"
        assert picture["props"]["x"] == 4
        assert picture["props"]["y"] == 3
        for key in ("width", "height"):
            assert picture["props"][key].endswith("in")


def test_render_company_copies_prepared_ppt(tmp_path):
    source = tmp_path / "source.pptx"
    write_prepared_sales_pptx(source, "示例公司")
    record = valid_record()
    record["outreach"]["ppt_path"] = str(source)
    record["outreach"]["ppt_manifest"] = sales_ppt_manifest("示例公司")
    plan = {
        "folder_name": "示例公司",
        "name": "示例公司",
        "record": record,
        "products": {"selected": []},
        "materials": {"selected": []},
    }
    staging = tmp_path / "staging"
    final = tmp_path / "final"
    rendered = _render_company(plan, staging, final, ppt_validator=lambda path: Path(path))
    staged_ppt = staging / "示例公司" / "准备资料" / "示例公司-销售资料.pptx"
    assert staged_ppt.read_bytes() == source.read_bytes()
    assert rendered["ppt_path"] == final / "示例公司" / "准备资料" / "示例公司-销售资料.pptx"


def test_ppt_input_contains_sales_ready_company_context(tmp_path):
    item = company_item(tmp_path)
    payload = build_ppt_input(item["plan"])
    assert payload["company_name"] == "示例公司"
    assert "欧美" in payload["market_keywords"]
    assert "法定代表人：张三" in payload["business_info"]
    assert payload["opening_copy"] == "您好，我们针对贵司产品准备了资料。"
    assert payload["activities"]["招聘"]


def test_ppt_builder_uses_artifact_tool_and_source_notes():
    script = (SCRIPTS_DIR / "iyw_sales_ppt.mjs").read_text(encoding="utf-8")
    theme = (SCRIPTS_DIR / "iyw_sales_ppt_theme.mjs").read_text(encoding="utf-8")
    assert "@oai/artifact-tool" in script
    assert "PresentationFile.exportPptx" in script
    assert "[Sources]" in theme
    for field in ("company_name", "market_keywords", "activities", "opening_copy"):
        assert f"input.{field}" in script


def test_ppt_material_summary_keeps_all_non_image_files():
    script = (SCRIPTS_DIR / "iyw_sales_ppt.mjs").read_text(encoding="utf-8")
    assert "const nonImages" in script
    assert "const listed = [...nonImages" in script
    assert "].slice(0, 6)" not in script


def test_render_company_ppt_failure_does_not_abort_company(tmp_path):
    record = valid_record()
    plan = {
        "folder_name": "示例公司",
        "name": "示例公司",
        "record": record,
        "products": {"selected": []},
        "materials": {"selected": []},
    }

    def failing_generator(*_args, **_kwargs):
        raise RuntimeError("artifact tool unavailable")

    rendered = _render_company(
        plan,
        tmp_path / "staging",
        tmp_path / "final",
        ppt_generator=failing_generator,
    )
    assert rendered["ppt_path"] is None


def test_invalid_prepared_ppt_uses_fallback_generator(tmp_path):
    source = tmp_path / "broken.pptx"
    source.write_bytes(b"not-a-pptx")
    record = valid_record()
    record["outreach"]["ppt_path"] = str(source)
    plan = {
        "folder_name": "示例公司",
        "name": "示例公司",
        "record": record,
        "products": {"selected": []},
        "materials": {"selected": []},
    }

    def generator(_plan, output):
        write_minimal_pptx(Path(output))
        return Path(output)

    rendered = _render_company(
        plan,
        tmp_path / "staging",
        tmp_path / "final",
        ppt_generator=generator,
    )
    assert rendered["ppt_path"] is not None


def test_office_verify_falls_back_when_screenshot_reports_no_browser(tmp_path, monkeypatch):
    workbook = tmp_path / "sample.xlsx"
    calls = []

    def fake_run(args, **_kwargs):
        if args[:3] == ["view", str(workbook), "screenshot"]:
            return "No headless browser available"
        return ""

    def fake_fallback(_path, preview, _runner):
        calls.append(preview)
        preview.write_bytes(b"png")

    monkeypatch.setattr(iyw_sales_office, "_run", fake_run)
    monkeypatch.setattr(iyw_sales_office, "fallback_screenshot", fake_fallback)
    iyw_sales_office._verify(workbook)
    assert len(calls) == 1


def test_office_verify_uses_html_when_no_browser_is_available(tmp_path, monkeypatch):
    workbook = tmp_path / "sample.xlsx"
    html_calls = []

    def fake_run(args, **_kwargs):
        if args[:3] == ["view", str(workbook), "screenshot"]:
            return "No headless browser available"
        if args[:3] == ["view", str(workbook), "html"]:
            preview = Path(args[-1])
            preview.write_text("<html><body>可读表格</body></html>", encoding="utf-8")
            html_calls.append(preview)
        return ""

    def missing_browser(*_args):
        raise iyw_sales_office.OfficePreviewError("未找到可用于 Office 预览的本机浏览器")

    monkeypatch.setattr(iyw_sales_office, "_run", fake_run)
    monkeypatch.setattr(iyw_sales_office, "fallback_screenshot", missing_browser)
    iyw_sales_office._verify(workbook)
    assert len(html_calls) == 1


def test_office_verify_rejects_reported_layout_issues(tmp_path, monkeypatch):
    workbook = tmp_path / "sample.xlsx"

    def fake_run(args, **_kwargs):
        if args[:3] == ["view", str(workbook), "issues"]:
            return "ERROR: overlap in 产品图"
        return ""

    monkeypatch.setattr(iyw_sales_office, "_run", fake_run)
    with pytest.raises(iyw_sales_office.OfficeCliError, match="布局检查未通过"):
        iyw_sales_office._verify(workbook)
