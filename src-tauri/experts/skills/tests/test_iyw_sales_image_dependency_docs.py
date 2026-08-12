from pathlib import Path


ROOT = Path(__file__).parents[1]


def _read(relative_path: str) -> str:
    return (ROOT / relative_path).read_text(encoding="utf-8")


def test_sales_skill_requires_image_workflow_for_all_materials():
    text = _read("iyw-sales-assistant-workflows/SKILL.md")
    assert "首选来源" in text
    assert "必须先" in text
    assert "iyw-image-workflows" in text
    for material_type in (
        "exhibition_report",
        "trend_theme",
        "retail_image",
        "catalog_image",
        "pattern_poster",
        "ai_image",
    ):
        assert material_type in text
    assert "才可使用合规的公开资料检索、其他图库或其他生图能力补足" in text
    assert "优先复用可用的 IYW 报告、内容库、`iyw-image-workflows`" not in text
    assert "唯一来源" not in text


def test_operations_map_materials_to_fixed_image_capabilities():
    text = _read("iyw-sales-assistant-workflows/references/operations.md")
    expected_rows = (
        "| `exhibition_report` | `report-list`、`report-detail`、`report-full`、`report-images` |",
        "| `trend_theme` | `trend-list`、`trend-detail` |",
        "| `retail_image` | `image` |",
        "| `catalog_image` | `catalog` |",
        "| `pattern_poster` | `ip-patterns` |",
        "| `ai_image` | `$commerceCli fission-generate` |",
    )
    for row in expected_rows:
        assert row in text


def test_materials_require_staging_and_source_before_batch_package():
    skill = _read("iyw-sales-assistant-workflows/SKILL.md")
    operations = _read("iyw-sales-assistant-workflows/references/operations.md")
    assert "必须把可用资源下载或保存到暂存目录" in skill
    assert "`batch-package` 会校验能力与类型匹配" in skill
    assert "`materials[]` 项中写入实际 `source`" in operations
    assert "`material_workflow.attempts[]`" in operations
    assert "首选文件和补充文件都必须有类型匹配的尝试" in operations


def test_material_sources_are_traceable_to_image_workflow():
    contract = _read("iyw-sales-assistant-workflows/references/data-contract.md")
    agent = _read("iyw-sales-assistant-workflows/agents/openai.yaml")
    assert "iyw-image-workflows:" in contract
    assert "first-choice provider" in agent
    assert "Only when its real local files remain below the target" in agent
    assert "The sales workflow may use other compliant sources to fill the remaining gap" in agent


def test_sales_batch_documents_parallel_tracks_and_final_file_ownership():
    skill = _read("iyw-sales-assistant-workflows/SKILL.md")
    operations = _read("iyw-sales-assistant-workflows/references/operations.md")
    agent = _read("iyw-sales-assistant-workflows/agents/openai.yaml")
    for text in (skill, operations):
        assert "工商与联系人" in text
        assert "产品图片" in text
        assert "活动证据" in text
        assert "图片工作流资料与 PPT" in text
        assert "有界并发" in text
        assert "主代理独占最终 Excel 和 PPT 写入" in text
        assert "每家公司生成一份 PPT" in text
        assert "六个月" in text
        assert "一年" in text
    assert "bounded concurrency" in agent
    assert "eight tracks globally" in agent
    assert "company_key, track, status, missing" in agent
    assert "only the main agent writes the final excel and ppt files" in agent.lower()
    assert "one PPT per company" in agent


def test_sales_batch_documents_exact_ten_headers():
    skill = _read("iyw-sales-assistant-workflows/SKILL.md")
    operations = _read("iyw-sales-assistant-workflows/references/operations.md")
    headers = (
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
    for header in headers:
        assert header in skill
        assert header in operations


def test_sales_data_contract_carries_machine_verifiable_track_and_source_receipts():
    contract = _read("iyw-sales-assistant-workflows/references/data-contract.md")
    for field in (
        "material_workflow.attempts[]",
        "track_results[]",
        "download_receipt",
        "outreach.ppt_manifest",
    ):
        assert field in contract
    for count in ("`complete`", "`incomplete`"):
        assert count in contract
    assert "重新执行渲染和越界检查" in contract
