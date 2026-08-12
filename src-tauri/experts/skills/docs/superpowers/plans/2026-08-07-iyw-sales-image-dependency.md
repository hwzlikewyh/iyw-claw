# IYW 销售资料图片工作流优先依赖实施计划

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 强制 `iyw-sales-assistant-workflows` 的六类销售资料先由 `iyw-image-workflows` 获取或生成，结果不足时允许合规补足并可审计地继续交付。

**Architecture:** 销售 skill 继续负责业务编排和批次交付；图片 skill 是报告、趋势、图片、画册、图案和 AI 图的首选能力边界，只有其真实文件不足时才进入补足阶段。通过主说明、操作参考、数据契约示例和 agent 默认提示词固化优先级、来源追踪和缺项规则，不增加重复 API 适配代码。

**Tech Stack:** Markdown skill 文档、YAML agent metadata、Python `pytest` 文档契约测试、`quick_validate.py`。

## Global Constraints

- 六类资料必须先来自 `iyw-image-workflows`；只有其真实文件低于目标时才允许使用其他合规来源补足。
- 补足资料必须记录实际来源或生成任务；完成补足后仍不足时继续流程并记录缺项。
- 不改变 `materials` 枚举、数量目标或图片 skill CLI 接口；主代理记录首选尝试，销售 CLI 校验 type/alias/source、真实文件和补充资料凭证。
- 不执行 Git commit；仓库规则要求 Git 历史操作先取得用户明确授权。

---

### Task 1: 建立跨 Skill 依赖契约测试

**Files:**
- Create: `tests/test_iyw_sales_image_dependency_docs.py`

**Interfaces:**
- Consumes: `iyw-sales-assistant-workflows/SKILL.md`、`references/operations.md`、`references/data-contract.md`、`agents/openai.yaml`。
- Produces: 文档级测试，要求六类资料先映射到图片 skill，并锁定不足时的合规补足规则。

- [x] **Step 1: Write the failing test**

创建以下测试，先锁定首选来源、六类资料名、固定能力映射、补足顺序和可追溯来源：

```python
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
        "exhibition_report", "trend_theme", "retail_image",
        "catalog_image", "pattern_poster", "ai_image",
    ):
        assert material_type in text
    assert "才可使用合规的公开资料检索、其他图库或其他生图能力补足" in text
    assert "优先复用可用的 IYW 报告、内容库、`iyw-image-workflows`" not in text
    assert "唯一来源" not in text


def test_operations_map_materials_to_fixed_image_capabilities():
    text = _read("iyw-sales-assistant-workflows/references/operations.md")
    expected = {
        "exhibition_report": ("report-list", "report-detail"),
        "trend_theme": ("trend-list", "trend-detail"),
        "retail_image": ("image",),
        "catalog_image": ("catalog",),
        "pattern_poster": ("ip-patterns",),
        "ai_image": ("fission-generate",),
    }
    for material_type, capabilities in expected.items():
        assert material_type in text
        for capability in capabilities:
            assert capability in text


def test_material_sources_are_traceable_to_image_workflow():
    contract = _read("iyw-sales-assistant-workflows/references/data-contract.md")
    agent = _read("iyw-sales-assistant-workflows/agents/openai.yaml")
    assert "iyw-image-workflows:" in contract
    assert "first-choice provider" in agent
    assert "Only when its real local files remain below the target" in agent
    assert "The sales workflow may use other compliant sources to fill the remaining gap" in agent
```

- [x] **Step 2: Run test to verify it fails**

Run: `uv run --no-project --with pytest python -m pytest tests/test_iyw_sales_image_dependency_docs.py -q`

Expected: FAIL because the original sales skill does not define the six fixed mappings or the ordered supplementation rule.

### Task 2: 固化图片工作流首选来源

**Files:**
- Modify: `iyw-sales-assistant-workflows/SKILL.md:155-174`
- Modify: `iyw-sales-assistant-workflows/references/operations.md:82-100`
- Modify: `iyw-sales-assistant-workflows/references/data-contract.md:99-104`
- Modify: `iyw-sales-assistant-workflows/agents/openai.yaml:20-45`

**Interfaces:**
- Consumes: `iyw-image-workflows/SKILL.md` 的 `iyw_search.py` 固定搜索别名和 `fission-generate` 入口。
- Produces: 销售代理可执行的首选来源映射、补足条件和统一缺项规则；不新增脚本接口。

- [x] **Step 1: Replace the sales material instructions**

在 `SKILL.md` 的资料匹配章节中保留现有内外销数量，改为明确要求六类资料必须先由 `iyw-image-workflows` 提供，并写明：报告使用报告检索，趋势使用趋势检索，卖场使用图片检索，画册使用画册检索，爆款图案使用图案/IP 检索，AI 图使用 `fission-generate`；对远程结果先落盘并写入来源；销售 skill 不猜 payload 或接口；只有真实文件不足时才允许合规补足，完成补足后仍不足再记缺项。

- [x] **Step 2: Add the fixed alias mapping to operations reference**

在 `Material Targets` 后增加表格，使用以下精确映射：

```text
exhibition_report -> report-list / report-detail / report-full / report-images
trend_theme       -> trend-list / trend-detail
retail_image      -> image
catalog_image     -> catalog
pattern_poster    -> ip-patterns
ai_image          -> fission-generate
```

同时注明 `$searchCli`、`$commerceCli` 的设置和 payload 契约必须回读 `iyw-image-workflows`。主代理在 `batch-package` 前写入 `material_workflow.attempts[]`，销售 CLI 校验类型、固定 alias 和补充来源凭证。补足只能在首选来源的真实文件低于目标时发生，并且仅适用于销售资料。

- [x] **Step 3: Make the data contract example traceable**

只修改示例的 `materials[0].source` 为 `iyw-image-workflows:trend-detail`，保留 `local_path`、类型和其他字段不变。

- [x] **Step 4: Update the agent default prompt**

在默认提示词中加入等价英文门禁：`iyw-image-workflows` is the first-choice provider for all six material types; use its fixed search aliases and `fission-generate` first; only supplement when its real local files remain below target; preserve actual sources and continue with any remaining gap.

- [x] **Step 5: Run the focused tests**

Run: `uv run --no-project --with pytest python -m pytest tests/test_iyw_sales_image_dependency_docs.py -q`

Expected: PASS with all four tests green.

### Task 3: 完成 skill 校验与回归验证

**Files:**
- Verify: `iyw-sales-assistant-workflows/`、`iyw-image-workflows/`

**Interfaces:**
- Consumes: 已更新的文档和现有图片工作流测试。
- Produces: 可发布的 skill 文档，且固定搜索/工具路由回归通过。

- [x] **Step 1: Run skill metadata validation**

Run: `$env:PYTHONUTF8='1'; uv run --no-project --with pyyaml python C:\Users\iyw\.codex\skills\.system\skill-creator\scripts\quick_validate.py iyw-sales-assistant-workflows`

Expected: exit code 0 and no frontmatter or naming errors.

- [x] **Step 2: Run related image workflow regressions**

Run: `uv run --no-project --with pytest python -m pytest tests/test_iyw_search.py tests/test_iyw_image_tools.py -q`

Expected: all existing search and fixed-tool tests pass; no API requests are made by these tests.

- [x] **Step 3: Run the full test suite**

Run: `uv run --no-project --with pytest python -m pytest -q`

Expected: no new failures attributable to the sales material dependency change. Preserve any pre-existing unrelated failures in the final report.

- [x] **Step 4: Review the final diff**

Run: `git diff --check; git status --short`

Expected: no whitespace errors; only the approved design/plan, sales documentation, and focused test changes are present. Do not commit without separate Git authorization.
