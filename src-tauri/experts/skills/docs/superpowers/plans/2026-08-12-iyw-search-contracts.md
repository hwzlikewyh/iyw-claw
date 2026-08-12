# IYW 搜索接口完整合同 Implementation Plan

**Goal:** 为 `iyw-image-workflows` 的 17 个查询别名补齐请求合同、示例、响应校验和安全边界，同时保留现有 `search <alias> --input-file` 调用方式。

**Architecture:** `iyw_search_specs.py` 声明固定 endpoint、示例和字段 schema，
`iyw_search_contracts.py` 负责请求/响应校验，`iyw_search.py` 只负责 CLI、HTTP 调度
与安全结果规范化。合同示例既是 `example` 命令输出，也是参数化 dry-run 测试输入。

**Tech Stack:** Python 3.10+ 标准库、现有 `IywClient`、pytest、uv。

## Constraints

- 17 个别名必须全部具有固定 endpoint、示例、请求校验和响应校验。
- 保留 `search <alias> --input-file payload.json`，不接受任意 base URL、prefix 或 path。
- 所有请求使用 POST；仅发送 `token` 认证头。
- 请求体拒绝凭据、Cookie、签名字段、HTTP URL 和签名 URL。
- dry-run 与 `example` 不读取 token、不访问网络。
- 不修改已经接入的 Commerce operation。
- Python 文件不超过 300 行，函数不超过 50 行。
- 用户已明确允许 `SKILL.md` 超过 300 行。

### Task 1: 建立 17 份合同和参数化测试

**Files:**
- Create: `iyw-image-workflows/scripts/iyw_search_specs.py`
- Create: `iyw-image-workflows/scripts/iyw_search_contracts.py`
- Create: `tests/test_iyw_search_contracts.py`

- [x] 添加合同完整性、示例合法性、字段类型、未知字段、敏感字段和 URL 安全测试。
- [x] 实现 endpoint、示例和声明式字段 schema。
- [x] 为数组、普通 object、列表 object、分页 object 和详情 object 实现响应 shape 校验。

### Task 2: 接入 CLI 和 example 命令

**Files:**
- Modify: `iyw-image-workflows/scripts/iyw_search.py`
- Modify: `tests/test_iyw_search.py`

- [x] 用合同注册表替换独立 `SEARCH_SPECS` 定义，并保留兼容导出。
- [x] 在 HTTP 请求前校验并规范化请求体。
- [x] 新增 `example <alias>`，输出可直接作为输入文件内容的 JSON object/array。
- [x] 实际响应按合同校验后再执行安全脱敏和稳定结果包装。

### Task 3: 更新 Skill 使用文档

**Files:**
- Modify: `iyw-image-workflows/SKILL.md`
- Modify: `iyw-image-workflows/references/tool-contracts.md`
- Modify: `iyw-image-workflows/references/commerce-operations.md`
- Modify: `iyw-image-workflows/agents/openai.yaml`
- Modify: `iyw-image-workflows/scripts/iyw_tool_core.py`
- Create: `iyw-image-workflows/scripts/iyw_tool_specs.py`
- Create: `iyw-image-workflows/scripts/iyw_commerce_safety.py`
- Modify: `iyw-image-workflows/scripts/iyw_commerce_core.py`
- Modify: `tests/test_iyw_image_skill_docs.py`
- Modify: `tests/test_iyw_image_tools.py`
- Create: `tests/test_iyw_commerce_safety.py`

- [x] 写明 `example`、dry-run 和 search 标准流程。
- [x] 列出 17 个别名的用途及结果类型。
- [x] 移除“帮助输出包含 JSON 示例”这一不实描述。
- [x] 增加智能搜索驱动趋势/主题设计的搜索、推荐、结果确认和主题拆页路由。
- [x] 固定单页 `variation`、多页 `mix`、系列 `extend` 均使用模型二，并覆盖 4/6 宫格
  系列套组与企划案行为。
- [x] 固定别名与通用 `invoke` 都强制模型二，并让 `check-image` 在请求前拒绝签名 URL。

### Task 4: 验证与审计

- [x] 运行全部 `test_iyw*.py` 搜索、CLI、图片工具和文档测试：`215 passed`。
- [x] 运行 Python compile、Ruff 和 `git diff --check`。
- [x] 验证 17 个示例均可 dry-run，且 CLI 不读取 token。
- [x] 对 `image` 执行一次 `pageSize=1` 的只读冒烟查询：业务成功，返回 1 条。
- [x] 检查文件/函数上限、敏感内容、生成残留和最终差异。
