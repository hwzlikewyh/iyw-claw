# IYW 图片工作流补充接口 Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 接入功能清单中的图片工具和搜索接口，并让指定/上传图片默认优先走变款。

**Architecture:** 在现有 commerce CLI 上增加固定工具别名与 payload 校验；新增独立搜索 CLI，使用同一 `IywClient` 但为每个别名固定 host/prefix/path。Skill 文档描述输入优先级，所有调用先检测图片，收费任务复用既有任务查询。

**Tech Stack:** Python 3.10+、标准库 `argparse`/`json`/`urllib`、现有 `IywClient`、pytest。

## Global Constraints

- 所有 IYW API 请求只发送 `token` 请求头，不发送 Cookie、`Authorization`、`securitykey`、`tokenInfo`。
- 接口统一使用 POST；仅 dry-run 可无 token。
- 所有输入图片 URL 必须 HTTPS；本地图片先 `upload` 并通过 `checkImage`。
- 不自动重试收费创建请求，不重建超时任务。
- 不暴露模型、渠道、平台、价格、token 或签名 URL。

---

### Task 1: 工具别名和 payload 校验

**Files:**
- Modify: `iyw-image-workflows/scripts/iyw_commerce_core.py`
- Create: `iyw-image-workflows/scripts/iyw_tool_core.py`
- Test: `tests/test_iyw_image_tools.py`

**Interfaces:**
- `TOOL_OPERATIONS: dict[str, str]` 将固定别名映射到 operation。
- `validate_tool_payload(alias: str, payload: dict[str, Any]) -> str` 校验并返回 operation。

- [ ] **Step 1: Write tests** for alias mapping, variation/mix HTTPS and count checks, specialized enum checks, and unknown alias rejection.
- [ ] **Step 2: Implement** alias map and validators with no network access.
- [ ] **Step 3: Run** `pytest -q tests/test_iyw_image_tools.py` and confirm all tests pass.

### Task 2: Commerce CLI tool command

**Files:**
- Modify: `iyw-image-workflows/scripts/iyw_commerce.py`
- Modify: `iyw-image-workflows/references/commerce-operations.md`
- Test: `tests/test_iyw_image_tools.py`

**Interfaces:**
- CLI form: `tool <alias> --input-file payload.json [--dry-run]`.
- `run_command()` loads JSON, calls `validate_tool_payload`, then `invoke_operation` exactly once.

- [ ] **Step 1: Add parser and dispatch tests** using dry-run.
- [ ] **Step 2: Add the command** while retaining `invoke`, upload, fission and task commands.
- [ ] **Step 3: Document** every alias and the task query flow.
- [ ] **Step 4: Run** CLI dry-runs for variation, outpaint, convert and video.

### Task 3: Search CLI

**Files:**
- Create: `iyw-image-workflows/scripts/iyw_search.py`
- Test: `tests/test_iyw_search.py`

**Interfaces:**
- CLI form: `search <alias> --input-file payload.json [--dry-run]`.
- `SEARCH_SPECS` fixes base URL, prefix and path; no arbitrary path argument.

- [ ] **Step 1: Write tests** for image search, report, trend and IP aliases, host/path dry-run, token-only header behavior, and sensitive-field redaction.
- [ ] **Step 2: Implement** fixed specs and safe recursive response normalization.
- [ ] **Step 3: Run** `pytest -q tests/test_iyw_search.py`.

### Task 4: Skill guidance and regression verification

**Files:**
- Modify: `iyw-image-workflows/SKILL.md`
- Modify: `iyw-image-workflows/agents/openai.yaml`
- Test: `tests/test_iyw_image_skill_docs.py`

**Interfaces:**
- Skill decision rule: any specified/uploaded/provided image defaults to the wrapped `tool variation` command, which maps to operation `g_tools_generate_image` and fixes payload `toolName` to `variation`; explicit specialized intent wins.

- [ ] **Step 1: Update** command catalog, search catalog, and image-priority decision table.
- [ ] **Step 2: Update** agent display prompt to mention all fixed tools and search.
- [ ] **Step 3: Assert** the priority and safety phrases in tests.
- [ ] **Step 4: Run** focused tests, Python compile, and all repository tests; remove generated `__pycache__`/temporary files.
