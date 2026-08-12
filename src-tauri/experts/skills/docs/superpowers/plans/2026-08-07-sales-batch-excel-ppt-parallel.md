# 销售批次 Excel、PPT 与并行资料工作流实施计划

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将批次 Excel 改为十列销售摘要，为每家公司生成资料 PPT，并在 skill 中固化按公司并行、主代理统一合并的执行方式。

**Architecture:** `iyw_sales_batch_office.py` 只负责十列工作表和缩略图/链接；新增 `iyw_sales_ppt.py` 负责单家公司 PPT；`iyw_sales_batch.py` 在渲染公司目录时调用 PPT 生成器并把 PPT 链接传给 Excel。活动窗口和输入字段由 `iyw_sales_assistant.py`、验证器及数据契约共同定义，skill 文档负责指导并行子代理返回结构化结果。

**Tech Stack:** Python 3.10+、现有 `officecli` Excel 命令、PowerPoint `@oai/artifact-tool` JavaScript 模块、pytest、PowerShell。

## Global Constraints

- 工作簿只有一个“今日推荐公司”工作表，每家公司一行，固定十列表头，不保留旧的分析/切入点/三列预览表头。
- 六个月内某一类招聘、知识产权或参展信息为空时，才把该类检索窗口扩展到十二个月，并标注“近一年补充”。
- 产品图只能来自该公司店铺产品图片；PPT 资料先从 `iyw-image-workflows` 获取，不足时才合规补足并保留来源。
- 子代理只能写独立临时目录或返回结构化结果；主代理统一写 Excel 和 PPT，使用有界并发。
- PPT 必须使用 `@oai/artifact-tool` 生成，渲染所有页面并检查溢出和重叠。
- 不执行 Git commit 或 push；仓库规则要求 Git 历史操作先取得明确授权。

---

### Task 1: 扩展输入数据契约和活动窗口

**Files:**
- Modify: `iyw-sales-assistant-workflows/references/data-contract.md`
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_validation.py`
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_assistant.py`
- Test: `tests/test_iyw_sales_batch_output.py`

**Interfaces:**
- Consumes: 现有 `company`、`run`、`activities`、`products`、`contacts`、`outreach` 字段。
- Produces: `company.business_info`、`run.market_keywords`、`products[].product_url` 和扩展活动类型可被输出层稳定读取；评分仍只按原有五类六个月证据计算。

- [ ] **Step 1: Write failing contract tests**

在 `tests/test_iyw_sales_batch_output.py` 中增加：

```python
def test_validate_record_accepts_business_market_and_activity_fields():
    record = valid_record()
    record["company"]["business_info"] = {"legal_representative": "张三"}
    record["run"]["market_keywords"] = ["欧美", "日韩"]
    record["products"][0]["product_url"] = "https://example.com/product"
    record["activities"] += [
        {"type": "trademark", "observed_at": "2026-05-01T09:00:00+08:00", "source": "lixiao", "evidence": "商标"},
        {"type": "patent", "observed_at": "2026-04-01T09:00:00+08:00", "source": "lixiao", "evidence": "专利"},
    ]
    assert validate_record(record)["company"]["business_info"]["legal_representative"] == "张三"


def test_activity_window_uses_one_year_only_when_six_month_category_is_empty():
    record = valid_record(as_of="2026-08-07T12:00:00+08:00")
    record["activities"] = [{"type": "patent", "observed_at": "2026-01-01T09:00:00+08:00", "source": "lixiao", "evidence": "专利"}]
    result = activity_display(record)
    assert "近一年补充" in result
    assert "2026-01-01" in result
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `uv run --no-project --with pytest python -m pytest tests/test_iyw_sales_batch_output.py -q`

Expected: FAIL because the new output helpers and activity display contract do not exist.

- [ ] **Step 3: Implement the canonical fields and window helper**

Keep nested fields permissive but document canonical names. Add an `activity_display(record)` helper in `iyw_sales_batch_office.py` that groups `recruitment`, `sales_hiring`, `designer_hiring`, `copyright_work`, `trademark`, `patent`, and `exhibition`; it selects six-month records per group, then adds twelve-month records for groups with none and prefixes those entries with `近一年补充`.

- [ ] **Step 4: Run focused tests and verify pass**

Run: `uv run --no-project --with pytest python -m pytest tests/test_iyw_sales_batch_output.py -q`

Expected: PASS.

### Task 2: Rebuild the batch workbook as the requested ten-column view

**Files:**
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_batch_office.py`
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_labels.py`
- Test: `tests/test_iyw_sales_batch_output.py`

**Interfaces:**
- Consumes: `_plan_record` output (`products.selected`, `contacts.selected`, `record.company`, `record.run`, `record.activities`, `record.outreach`) and PPT path returned by Task 3.
- Produces: `HEADERS`, `_company_values`, `_company_commands` and `batch_workbook_commands` that write the exact ten headers and one row per company.

- [ ] **Step 1: Add header and value-shape tests**

Assert:

```python
assert HEADERS == (
    "公司名", "前三联系及角色", "前三联系人电话", "基本信息(工商信息)",
    "产品图", "市场关键词", "店铺", "近半年招聘/知识产权/参展情况",
    "准备资料", "针对性开场白",
)
values, links = _company_values(plan, 1)
assert len(values) == 10
assert "法定代表人：张三" in values[3]
assert "欧美" in values[5]
assert "商品：https://example.com/product" in values[6]
assert "近一年补充" in values[7]
assert links[8].endswith(".pptx")
```

- [ ] **Step 2: Run tests and verify failure**

Run: `uv run --no-project --with pytest python -m pytest tests/test_iyw_sales_batch_output.py -q`

Expected: FAIL against the current fifteen-column workbook.

- [ ] **Step 3: Implement the ten-column value helpers**

Replace the old headers and widths. Format contacts as names plus roles in one cell and phones only in the next. Build labeled multiline business info from the eleven canonical fields. Build store text from `company.shop_url` plus unique `products[].product_url` links. Use `run.market_keywords` and product analysis target markets for market text. Put the opening copy in the last column. Remove the three old preview columns and place up to three product pictures inside the single `产品图` column with stable row height and width.

- [ ] **Step 4: Run focused tests and verify pass**

Run: `uv run --no-project --with pytest python -m pytest tests/test_iyw_sales_batch_output.py -q`

Expected: PASS.

### Task 3: Generate one PPT per recommended company

**Files:**
- Create: `iyw-sales-assistant-workflows/scripts/iyw_sales_ppt.mjs`
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_batch.py`
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_batch_office.py`
- Test: `tests/test_iyw_sales_batch_output.py`

**Interfaces:**
- Consumes: one company plan plus staged product/material files and `outreach.opening_copy`.
- Produces: `<company>/准备资料/<company>-销售资料.pptx` and a linkable absolute path in the rendered company item.

- [ ] **Step 1: Read the required presentation resources**

Read `C:\Users\iyw\.codex\plugins\cache\openai-primary-runtime\presentations\26.805.11740\skills\presentations\style_guidelines.md`, `artifact_tool_docs/API_QUICK_START.md`, and `artifact_tool_docs/api/API_DOCS.md` before writing the `.mjs` module. Initialize a temporary artifact-tool workspace with `setup_artifact_tool_workspace.mjs`.

- [ ] **Step 2: Add PPT path and content tests**

Test the batch renderer with a stub PPT generator and assert the returned company item includes a `.pptx` path and that the PPT source text includes company name, market keywords, activity display, and opening copy.

- [ ] **Step 3: Implement the artifact-tool deck**

Create a readable deck with a title slide, company/business overview, product and store slide, market and activity slide, image-workflow material slide, and opening-copy slide. Use actual local product/material images where available, add source notes, keep copy short, and export to the company `准备资料` directory. Do not use `python-pptx` or programmatic image drawing.

- [ ] **Step 4: Integrate PPT generation into batch rendering**

Render the PPT inside the staging batch directory before `staging.replace(batch)`, pass the final PPT path to `_company_values`, and make the `准备资料` cell link to it. A PPT failure marks that company incomplete but does not stop other companies.

- [ ] **Step 5: Render and inspect the PPT**

Run `node C:\Users\iyw\.codex\plugins\cache\openai-primary-runtime\presentations\26.805.11740\skills\presentations\container_tools\render_slides.py <pptx>` and `slides_test.py <pptx>`. Expected: every slide renders, no overflow is reported, and no text or image overlap is visible.

### Task 4: Document parallel subagent orchestration and activity fallback

**Files:**
- Modify: `iyw-sales-assistant-workflows/SKILL.md`
- Modify: `iyw-sales-assistant-workflows/agents/openai.yaml`
- Modify: `iyw-sales-assistant-workflows/references/operations.md`
- Modify: `iyw-sales-assistant-workflows/references/data-contract.md`
- Test: `tests/test_iyw_sales_image_dependency_docs.py`

**Interfaces:**
- Consumes: the four independent company tracks defined in the design.
- Produces: imperative instructions for bounded parallel dispatch, private staging, structured return, six-to-twelve-month fallback, and PPT/Excel ownership.

- [ ] **Step 1: Add documentation contract tests**

Assert the skill and agent prompt contain the four tracks, bounded concurrency, private staging/no shared Excel writes, six-month-first/year-fallback wording, PPT-per-company wording, and the exact ten headers.

- [ ] **Step 2: Update the skill documents**

State that the main agent dispatches one subagent per track per company, waits for all independent results, validates source/local paths, merges records, and alone calls `batch-package` and PPT generation. State that activity retrieval uses six months first and extends only missing categories to one year.

- [ ] **Step 3: Run documentation tests**

Run: `uv run --no-project --with pytest python -m pytest tests/test_iyw_sales_image_dependency_docs.py -q`

Expected: PASS.

### Task 5: Full validation and artifact QA

**Files:**
- Verify: `iyw-sales-assistant-workflows/` and generated temporary artifacts.

- [ ] **Step 1: Run all Python tests**

Run: `$env:PYTHONDONTWRITEBYTECODE='1'; uv run --no-project --with pytest python -m pytest -q`

Expected: all tests pass.

- [ ] **Step 2: Validate the skill**

Run: `$env:PYTHONUTF8='1'; uv run --no-project --with pyyaml python C:\Users\iyw\.codex\skills\.system\skill-creator\scripts\quick_validate.py iyw-sales-assistant-workflows`

Expected: `Skill is valid!`.

- [ ] **Step 3: Validate the workbook**

Run `officecli load_skill excel`, `officecli validate`, `officecli view ... issues`, and a screenshot or HTML preview on a generated sample workbook. Verify exactly ten headers, one row per company, readable wrap, product thumbnails inside `产品图`, PPT hyperlinks, and no overlap.

- [ ] **Step 4: Clean temporary artifacts and review diff**

Remove only generated temporary staging/PPT render directories, run `git diff --check`, and confirm no generated `.json`, `.md`, `.csv`, `.html`, or cache files remain in the delivery directory. Do not commit or push without authorization.

### Task 6: Harden provenance, failure isolation, and prepared artifact QA

**Files:**
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_selection.py`
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_batch.py`
- Create: `iyw-sales-assistant-workflows/scripts/iyw_sales_ppt_package.py`
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_ppt.py`
- Modify: `iyw-sales-assistant-workflows/scripts/iyw_sales_batch_office.py`
- Test: `tests/test_iyw_sales_provenance.py`
- Test: `tests/test_iyw_sales_batch_output.py`

- [x] Require type-matched `iyw-image-workflows` aliases and a typed attempt receipt before preferred or fallback materials count.
- [x] Reject empty/invalid material files and incomplete AI generation receipts.
- [x] Verify exact product source, positive company ownership, source/resolved URL, file hash, and complete image structure.
- [x] Count complete/incomplete companies and require exactly one receipt for every company track.
- [x] Validate prepared PPT content/QA manifest, re-run render/overflow QA, and fallback when it fails.
- [x] Bind the workbook representative thumbnail to the company row and parse Office issue output.
- [x] Run final repository validation and remove temporary QA artifacts.
