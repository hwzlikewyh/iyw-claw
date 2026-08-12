import hashlib
import sys
from pathlib import Path


SCRIPTS_DIR = Path(__file__).parents[1] / "iyw-sales-assistant-workflows" / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from iyw_sales_batch import (  # noqa: E402
    BUSINESS_INFO_KEYS,
    _delivery_status,
    _plan_record,
    _render_company,
    _summary,
)
from iyw_sales_images import download_product_images, is_supported_image  # noqa: E402
from iyw_sales_selection import material_status  # noqa: E402
from sales_test_helpers import (  # noqa: E402
    png_bytes,
    sales_ppt_manifest,
    write_minimal_pptx,
    write_prepared_sales_pptx,
    write_test_png,
)


def _record(product: dict) -> dict:
    return {
        "company": {"name": "来源测试公司"},
        "run": {"market": "export", "as_of": "2026-08-07T12:00:00+08:00"},
        "activities": [],
        "crm": {"match_status": "not_found"},
        "products": [product],
        "contacts": [],
        "materials": [],
        "outreach": {},
    }


def _eligible(_record):
    return {"crm_decision": {"eligible": True, "decision": "eligible_new"}}


def _download_receipt(path: Path) -> dict[str, str]:
    return {
        "source_url": "https://example.com/source.png",
        "resolved_url": "https://cdn.example.com/final.png",
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
    }


def test_batch_product_selection_requires_store_or_product_url(tmp_path):
    image = tmp_path / "product.png"
    image.write_bytes(b"png")
    plan = _plan_record(
        _record({"local_path": str(image), "name": "无链接产品"}),
        1,
        _eligible,
    )
    assert plan["products"]["actual"] == 0


def test_batch_product_selection_requires_company_product_receipt(tmp_path):
    image = tmp_path / "product.png"
    write_test_png(image)
    plan = _plan_record(
        _record(
            {
                "local_path": str(image),
                "image_url": "https://example.com/source.png",
                "product_url": "https://example.com/product",
                "source": "other",
                "download_receipt": _download_receipt(image),
            }
        ),
        1,
        _eligible,
    )
    assert plan["products"]["actual"] == 0


def test_batch_product_selection_rejects_wrong_company_id(tmp_path):
    image = tmp_path / "product.png"
    write_test_png(image)
    record = _record(
        {
            "local_path": str(image),
            "image_url": "https://example.com/source.png",
            "product_url": "https://example.com/product",
            "source": "lixiao:company-products",
            "company_id": "other-company",
            "download_receipt": _download_receipt(image),
        }
    )
    record["company"]["lixiao_id"] = "expected-company"
    plan = _plan_record(record, 1, _eligible)
    assert plan["products"]["actual"] == 0


def test_batch_accepts_company_product_with_matching_download_receipt(tmp_path):
    image = tmp_path / "product.png"
    write_test_png(image)
    record = _record(
        {
            "local_path": str(image),
            "image_url": "https://example.com/source.png",
            "product_url": "https://example.com/product",
            "source": "lixiao:company-products",
            "company_id": "expected-company",
            "download_receipt": _download_receipt(image),
        }
    )
    record["company"]["lixiao_id"] = "expected-company"
    plan = _plan_record(record, 1, _eligible)
    assert plan["products"]["actual"] == 1


def test_batch_rejects_product_without_positive_company_identity(tmp_path):
    image = tmp_path / "product.png"
    write_test_png(image)
    record = _record(
        {
            "local_path": str(image),
            "image_url": "https://example.com/source.png",
            "product_url": "https://example.com/product",
            "source": "lixiao:company-products",
            "download_receipt": _download_receipt(image),
        }
    )
    assert _plan_record(record, 1, _eligible)["products"]["actual"] == 0


def test_batch_rejects_receipt_for_another_image_url(tmp_path):
    image = tmp_path / "product.png"
    write_test_png(image)
    record = _record(
        {
            "local_path": str(image),
            "image_url": "https://example.com/other.png",
            "product_url": "https://example.com/product",
            "source": "lixiao:company-products",
            "company_name": "来源测试公司",
            "download_receipt": _download_receipt(image),
        }
    )
    assert _plan_record(record, 1, _eligible)["products"]["actual"] == 0


def test_product_download_preserves_resolved_url_and_hash(tmp_path):
    data = png_bytes()

    def fetcher(_url):
        return data, "image/png", "https://cdn.example.com/final.png"

    result = download_product_images(
        [{"name": "产品", "image_url": "https://example.com/source.png"}],
        tmp_path,
        fetcher=fetcher,
    )
    receipt = result["products"][0]["download_receipt"]
    assert receipt["resolved_url"] == "https://cdn.example.com/final.png"
    assert len(receipt["sha256"]) == 64


def test_existing_image_without_receipt_is_downloaded_again(tmp_path):
    existing = tmp_path / "existing.png"
    write_test_png(existing)
    calls: list[str] = []

    def fetcher(url):
        calls.append(url)
        return png_bytes(), "image/png", "https://cdn.example.com/final.png"

    result = download_product_images(
        [
            {
                "name": "产品",
                "image_url": "https://example.com/source.png",
                "local_path": str(existing),
            }
        ],
        tmp_path / "downloads",
        fetcher=fetcher,
    )
    assert calls == ["https://example.com/source.png"]
    assert result["products"][0]["local_path"] != str(existing)


def test_truncated_image_with_valid_magic_is_rejected(tmp_path):
    image = tmp_path / "truncated.png"
    image.write_bytes(b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR")
    assert is_supported_image(image) is False


def test_material_selection_requires_source_and_prefers_image_workflow(tmp_path):
    supplement = tmp_path / "supplement.pdf"
    preferred = tmp_path / "preferred.pdf"
    missing_source = tmp_path / "missing.pdf"
    for path in (supplement, preferred, missing_source):
        path.write_bytes(b"%PDF-1.4")
    result = material_status(
        [
            {"type": "trend_theme", "local_path": str(supplement), "source": "other"},
            {"type": "trend_theme", "local_path": str(preferred), "source": "iyw-image-workflows:trend-detail"},
            {"type": "trend_theme", "local_path": str(missing_source)},
        ],
        "export",
        preferred_attempted={"trend_theme": {"trend-detail": "completed"}},
    )
    assert result["selected"][0]["source"].startswith("iyw-image-workflows:")
    assert all(item.get("source") for item in result["selected"])


def test_batch_rejects_wrong_image_workflow_alias_and_unproven_fallback(tmp_path):
    material = tmp_path / "trend.pdf"
    material.write_bytes(b"%PDF-1.4")
    record = _record({})
    record["company"]["business_info"] = {key: "已核验" for key in BUSINESS_INFO_KEYS}
    record["outreach"]["opening_copy"] = "针对性开场白"
    record["materials"] = [
        {
            "type": "trend_theme",
            "local_path": str(material),
            "source": "iyw-image-workflows:catalog",
        }
    ]
    plan = _plan_record(record, 1, _eligible)
    assert plan["materials"]["actual"]["trend_theme"] == 0


def test_batch_allows_fallback_after_typed_image_workflow_attempt(tmp_path):
    material = tmp_path / "trend.pdf"
    material.write_bytes(b"%PDF-1.4")
    record = _record({})
    record["materials"] = [
        {"type": "trend_theme", "local_path": str(material), "source": "public-report"}
    ]
    record["material_workflow"] = {
        "attempts": [
            {
                "provider": "iyw-image-workflows",
                "type": "trend_theme",
                "alias": "trend-detail",
                "status": "empty",
            }
        ]
    }
    plan = _plan_record(record, 1, _eligible)
    assert plan["materials"]["actual"]["trend_theme"] == 1


def test_preferred_material_requires_matching_successful_attempt(tmp_path):
    material = tmp_path / "trend.pdf"
    material.write_bytes(b"%PDF-1.4")
    item = {
        "type": "trend_theme",
        "local_path": str(material),
        "source": "iyw-image-workflows:trend-detail",
    }
    assert material_status([item], "export")["actual"]["trend_theme"] == 0
    failed = {"trend_theme": {"trend-detail": "failed"}}
    assert material_status([item], "export", preferred_attempted=failed)["actual"]["trend_theme"] == 0


def test_zero_byte_material_is_not_counted(tmp_path):
    material = tmp_path / "trend.pdf"
    material.write_bytes(b"")
    result = material_status(
        [
            {
                "type": "trend_theme",
                "local_path": str(material),
                "source": "iyw-image-workflows:trend-detail",
            }
        ],
        "export",
        preferred_attempted={"trend_theme": {"trend-detail": "completed"}},
    )
    assert result["actual"]["trend_theme"] == 0


def test_material_with_wrong_file_signature_is_not_counted(tmp_path):
    material = tmp_path / "trend.pdf"
    material.write_bytes(b"not a pdf")
    result = material_status(
        [
            {
                "type": "trend_theme",
                "local_path": str(material),
                "source": "iyw-image-workflows:trend-detail",
            }
        ],
        "export",
        preferred_attempted={"trend_theme": {"trend-detail": "completed"}},
    )
    assert result["actual"]["trend_theme"] == 0


def test_ai_material_requires_succeeded_generation_receipt(tmp_path):
    image = tmp_path / "ai.png"
    write_test_png(image)
    item = {
        "type": "ai_image",
        "local_path": str(image),
        "source": "iyw-image-workflows:fission-generate",
    }
    attempts = {"ai_image": {"fission-generate": "completed"}}
    assert material_status([item], "domestic", preferred_attempted=attempts)["actual"]["ai_image"] == 0
    item["generation_receipt"] = {"status": "succeeded", "task_id": "task-1"}
    assert material_status([item], "domestic", preferred_attempted=attempts)["actual"]["ai_image"] == 1


def test_company_copy_failure_isolated_to_one_company(tmp_path):
    product = {"local_path": str(tmp_path / "missing.png"), "name": "缺失产品"}
    record = _record(product)
    plan = {
        "folder_name": "失败公司",
        "name": "失败公司",
        "record": record,
        "products": {"selected": [product]},
        "contacts": {"selected": []},
        "materials": {"selected": []},
        "analysis_available": True,
    }

    rendered = _render_company(plan, tmp_path / "staging", tmp_path / "final")

    assert rendered["ppt_path"] is None
    assert rendered["plan"]["status"] == "incomplete"
    assert rendered["plan"]["error"]


def test_invalid_xml_prepared_ppt_falls_back(tmp_path):
    source = tmp_path / "invalid-xml.pptx"
    from zipfile import ZipFile

    with ZipFile(source, "w") as archive:
        archive.writestr("[Content_Types].xml", "<Types />")
        archive.writestr("ppt/presentation.xml", "<p:presentation />")
    record = _record({})
    record["outreach"]["ppt_path"] = str(source)
    plan = {
        "folder_name": "坏文件公司",
        "name": "坏文件公司",
        "record": record,
        "products": {"selected": []},
        "contacts": {"selected": []},
        "materials": {"selected": []},
        "analysis_available": True,
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


def test_prepared_ppt_qa_failure_falls_back_to_generator(tmp_path):
    source = tmp_path / "prepared.pptx"
    write_prepared_sales_pptx(source, "来源测试公司")
    record = _record({})
    record["outreach"]["ppt_path"] = str(source)
    record["outreach"]["ppt_manifest"] = sales_ppt_manifest("来源测试公司")
    plan = {
        "folder_name": "待校验公司",
        "name": "待校验公司",
        "record": record,
        "products": {"selected": []},
        "contacts": {"selected": []},
        "materials": {"selected": []},
        "analysis_available": True,
    }

    def generator(_plan, output):
        write_minimal_pptx(Path(output))
        return Path(output)

    rendered = _render_company(
        plan,
        tmp_path / "staging",
        tmp_path / "final",
        ppt_generator=generator,
        ppt_validator=lambda _path: (_ for _ in ()).throw(RuntimeError("overflow")),
    )
    assert rendered["ppt_path"] is not None


def test_batch_summary_counts_incomplete_companies():
    plans = [
        {"recommended": True, "status": "complete"},
        {"recommended": True, "status": "incomplete"},
        {"recommended": False, "status": "skipped"},
    ]
    assert _summary(plans)["complete"] == 1
    assert _summary(plans)["incomplete"] == 1


def test_failed_track_keeps_company_incomplete():
    record = _record({})
    record["track_results"] = [
        {
            "company_key": "来源测试公司",
            "track": "activity_evidence",
            "status": "failed",
            "missing": ["exhibition"],
        }
    ]
    plan = {
        "record": record,
        "products": {"selected": [], "missing": 0},
        "contacts": {"missing": 0},
        "materials": {"missing": {}},
        "analysis_available": True,
        "ppt_available": True,
    }
    assert _delivery_status(plan) == "incomplete"


def test_all_four_track_receipts_are_required_for_complete_status():
    record = _record({})
    record["company"]["business_info"] = {key: "已核验" for key in BUSINESS_INFO_KEYS}
    record["outreach"]["opening_copy"] = "针对性开场白"
    plan = {
        "record": record,
        "products": {"selected": [], "missing": 0},
        "contacts": {"missing": 0},
        "materials": {"missing": {}},
        "analysis_available": True,
        "ppt_available": True,
    }
    assert _delivery_status(plan) == "incomplete"
    record["track_results"] = [
        {
            "company_key": "来源测试公司",
            "track": track,
            "status": "completed",
            "missing": [],
        }
        for track in (
            "business_contacts",
            "product_images",
            "activity_evidence",
            "image_materials_ppt",
        )
    ]
    assert _delivery_status(plan) == "complete"
