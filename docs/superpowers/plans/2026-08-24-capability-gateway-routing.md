# 能力网关统一发现与技能退役实施计划

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Agent 先读取网关 Skill，再通过可搜索的中文/英文能力目录使用宿主工具，增加脱敏用户资料读取，修正记忆结果语义，退役 `self-improving`，并确保任务成果在当前会话 lineage 中可见。

**Architecture:** 保留三件套 `search -> read -> invoke` 和现有稳定能力 ID；在网关目录增加显式意图元数据和 Unicode-aware 搜索。用户资料作为一个只读 companion 能力走现有 broker，记忆仍归 `UserMemoryService` 所有；任务成果继续由 MCP token 动态解析当前会话，但列表查询扩展到当前会话的祖先与后代，不把文件夹全部成果混入。

**Tech Stack:** Rust 2021、Axum/rmcp Streamable HTTP、SeaORM/SQLite、serde JSON、Next.js/React/TypeScript、TOML Skill manifest。

## Global Constraints

- 遵循 `iyw-claw/AGENTS.md`：本机不运行桌面构建、Cargo build/check/test 或前端完整测试；不新增测试文件，交付前执行静态审查、`git diff --check` 和可执行的 JSON/TOML/格式检查。
- 保留现有三件套名称、稳定 capability ID、schema digest、catalog digest 和 `delivery_ack` 语义。
- 不记录 token、凭证、完整账户资料、完整文件内容；用户资料只返回白名单字段。
- 只修改本任务文件；保留工作区已有的 `auto_continuation.rs`、`connection.rs`、前端上下文/i18n 和未跟踪文件。
- `skill` 是系统 Skill 发布源，`iyw-claw/src-tauri/experts/` 是内嵌基线；两处清单必须同步。
- 任何旧 Skill 清理只针对 iyw-claw 管理的 central/link/copy，保留用户自有或市场覆盖内容以及 `~/.iyw-claw/self-improving/` 运行状态。

---

### Task 1: 建立网关意图索引与 Agent 预置门禁

**Files:**
- Modify: `iyw-claw/src-tauri/src/acp/builtin_mcp/capability_metadata.rs`
- Modify: `iyw-claw/src-tauri/src/acp/builtin_mcp/capability.rs`
- Modify: `iyw-claw/src-tauri/src/acp/builtin_mcp/capability_registry.rs`
- Modify: `iyw-claw/src-tauri/src/acp/builtin_mcp/gateway.rs`
- Modify: `iyw-claw/src-tauri/src/acp/builtin_agent_prompt.rs`
- Modify: `iyw-claw/src-tauri/experts/skills/iyw-capability-gateway/SKILL.md`

**Interfaces:**
- `CapabilityCatalog::load()` 继续从 `TOOL_SCHEMA_JSON` 读取工具，并校验每个工具都存在意图元数据。
- `search_score(...)` 接受归一化 Unicode 查询和 `CapabilityIntentMetadata`，返回可排序的加权分数。
- `CapabilitySummary`/`CapabilityDetail` 增加可选 `intent_terms`、`when_to_use` 字段，旧字段和稳定 ID 不变。

- [ ] **Step 1: 记录基线**：保存当前 39 个 `CAPABILITY_BINDINGS`、当前搜索仅使用 `capability_aliases`/`search_score` 的代码位置，以及当前预置提示词中三件套激活规则；只读执行 `git diff --check`，不修改既有文件。
- [ ] **Step 2: 增加显式意图元数据**：在 `capability_metadata.rs` 定义 `CapabilityIntentMetadata { tool_name, aliases, intent_terms, negative_terms, when_to_use }`，为现有自动化、浏览器、artifact、delegation、interaction、session、media、memory、channel 39 个工具各提供至少一条中英文动作+对象词；新增 profile 工具的元数据在 Task 2 一并登记。
- [ ] **Step 3: 改造归一化和评分**：对查询和元数据做 Unicode 小写、空白折叠；英文按空格分词，中文保留连续片段并按 alias/动作/对象组合匹配；精确 capability ID > 完整 alias > 动作+对象 > 单词 > 描述文本。`negative_terms` 只用于扣分，不能让唯一精确命中消失。
- [ ] **Step 4: 校验覆盖**：让 `CapabilityCatalog::load()` 在绑定工具缺元数据、元数据重复或未知工具时返回明确 `CatalogError`；catalog digest 纳入元数据摘要，schema 变化和索引变化都会要求 Agent 重新读取。
- [ ] **Step 5: 加入预置提示词门禁**：在 `builtin_agent_prompt.rs` 的网关规则之前写明“启动后，当前目标可能需要宿主状态/动作时，先读取当前 `iyw-capability-gateway` Skill，再检查实际 callable surface”；明确禁止 `list_mcp_resources` 代替网关发现，Skill 不可读时只能依据实际可见工具降级。
- [ ] **Step 6: 同步网关 Skill**：把五层记忆规则、profile/memory 路由表、领域 Skill 分层和失败停止策略压缩进网关 Skill；删除其中任何直接写用户记忆文件的示例。
- [ ] **Step 7: 静态验证**：运行 `git diff --check`；使用 `node -e "JSON.parse(require('fs').readFileSync('src-tauri/src/acp/delegation/tool_schema.json','utf8'))"` 和 TOML 解析检查确认清单/JSON 未损坏；不运行桌面构建。

### Task 2: 增加脱敏当前用户资料能力

**Files:**
- Modify: `iyw-claw/src-tauri/src/acp/delegation/tool_schema.json`
- Modify: `iyw-claw/src-tauri/src/acp/delegation/companion.rs`
- Modify: `iyw-claw/src-tauri/src/acp/delegation/transport.rs`
- Modify: `iyw-claw/src-tauri/src/acp/delegation/listener.rs`
- Modify: `iyw-claw/src-tauri/src/acp/builtin_mcp/capability_registry.rs`
- Modify: `iyw-claw/src-tauri/src/acp/delegation/transport.rs` (`COMPANION_PROTOCOL_VERSION`)
- Modify: `iyw-claw/src-tauri/src/commands/iyw_account.rs`
- Modify: `iyw-claw/src-tauri/src/lib.rs`
- Modify: `iyw-claw/src-tauri/src/bin_targets/iyw_claw_server.rs`

**Interfaces:**
- 新 companion tool 名：`get_current_user_profile`；稳定 ID：`iyw.session.user_profile.read.v1`。
- 新 broker 消息：`BrokerMessage::UserProfile(BrokerUserProfileRequest { token: String })`。
- 新 listener trait：`UserProfileAccess::current_profile() -> Future<Output = Value>`。
- Agent-facing outcome：`{ "status": "ok|logged_out|profile_unavailable", "profile": { "display_name"?, "preferred_name"?, "organization_name"? } }`。

- [ ] **Step 1: 先扩展 schema 和绑定**：在 `tool_schema.json` 增加无参数、`additionalProperties: false` 的 `get_current_user_profile`；在 `capability_registry.rs` 添加稳定绑定；把 protocol version 从 6 递增为 7，确保旧 companion 被 readiness 门禁拒绝。
- [ ] **Step 2: 接通 companion dispatch**：在 `CompanionFeatures::allows_tool` 让 profile 走宿主 identity 读取路径；新增 `ToolFamily::Identity`、`dispatch_identity_tool`、`spawn_user_profile`，只发 token，不允许 Agent 传 user id、token 或查询词。
- [ ] **Step 3: 接通 broker wire**：新增 `BrokerUserProfileRequest` 和 `BrokerMessage::UserProfile`，为 socket/in-process 两种 backend 使用现有 `round_trip`；在 listener `serve_one`/`process_immediate` 分发，复用 token 失效和取消边界。
- [ ] **Step 4: 接入账户资料源**：在 `commands/iyw_account_profile.rs` 增加 `DbUserProfileAccess`，调用账户模块新增的身份专用 core（复用登录/续期但跳过积分请求）；只映射 `logged_in`、`name`/`nick_name` 到显示名、组织名，绝不序列化 user id、phone、points、avatar 或 token。远端认证/网络失败返回 `profile_unavailable`，不伪造资料。
- [ ] **Step 5: 两种运行模式装配**：在 `lib.rs` 和 `iyw_claw_server.rs` 构造 listener 时注入同一数据库连接的 `DbUserProfileAccess`，确认桌面和 server 使用相同字段白名单。
- [ ] **Step 6: 网关读取调用**：确认 catalog `read` 返回 profile 的公开 schema，`invoke` 仍先按 feature/权限校验，再经 companion dispatch；成功结果不得经过 `public_text` 暴露内部账户字段。
- [ ] **Step 7: 静态验证**：检查 protocol version、schema 名称、stable ID 和两处 listener 构造点全量一致；执行 `git diff --check` 和 JSON 解析，不运行 Cargo build/test。

### Task 3: 修正记忆召回结果和短查询语义

**Files:**
- Modify: `iyw-claw/src-tauri/src/user_memory/recall_types.rs`
- Modify: `iyw-claw/src-tauri/src/user_memory/recall_status.rs`
- Modify: `iyw-claw/src-tauri/src/user_memory/recall_result.rs`
- Modify: `iyw-claw/src-tauri/src/user_memory/recall_fallback.rs`
- Modify: `iyw-claw/src-tauri/src/user_memory/recall_fts.rs`
- Modify: `iyw-claw/src-tauri/src/acp/delegation/companion.rs` (`render_memory_recall_result`)
- Modify: `iyw-claw/src-tauri/src/acp/delegation/tool_schema.json`

**Interfaces:**
- `UserMemoryRecallResult` 保留既有 `status`、`abstained`、`reason_codes`，新增 `resultState`：`matched | no_evidence | unavailable`。
- `memory_recall` schema 描述明确：空结果表示没有证据，不表示事实为假；profile 查询不走此工具。

- [ ] **Step 1: 增加结果状态字段**：在 `recall_types.rs` 定义 serde 枚举/字符串字段；更新 `empty_result`、`complete_index_result`、fallback 成功/失败路径，只有有条目才为 `matched`，索引可用但无条目为 `no_evidence`，超时/源不可用/索引失败为 `unavailable`。
- [ ] **Step 2: 保留旧诊断字段**：继续输出 `abstained` 和现有 reason codes 供旧诊断使用，但 `render_memory_recall_result` 依据 `resultState` 文案化，不能把 `recall_abstained` 解释为用户没有该事实。
- [ ] **Step 3: 调整短查询**：trigram lane 对少于 3 个字符只记录内部 `short_query_not_applicable`，不再把 `fts_trigram_query_too_short` 作为失败原因；unicode/exact/alias/source fallback 仍按现有上限执行，禁止放开 SQL 注入或无限扫描。
- [ ] **Step 4: 同步工具说明**：更新 `memory_recall` 描述和 gateway Skill，明确姓名/昵称/称呼走 profile，历史记忆走 recall；append/propose 的安全边界与调用方式统一为宿主能力。
- [ ] **Step 5: 静态前后对比**：逐个检查 disabled、timeout、stale index、empty match、short query 和 transport error 的结果字段；运行 `git diff --check`，不新增测试夹具。

### Task 4: 退役 `self-improving` 并迁移旧安装

**Files:**
- Delete: `skill/self-improving/`（整个目录）
- Modify: `skill/experts.toml`
- Modify: `iyw-claw/src-tauri/experts/experts.toml`
- Modify: `iyw-claw/src-tauri/src/commands/experts.rs`
- Modify: `iyw-claw/src-tauri/experts/skills/iyw-capability-gateway/SKILL.md`

**Interfaces:**
- `RETIRED_BUNDLED_EXPERT_IDS: [&str; 1] = ["self-improving"]` 作为显式迁移 tombstone。
- 迁移只允许 `managed_link_is_owned` 或 `managed_copy_is_owned` 的 central/link/copy 被清理；用户修改或市场覆盖路径保持不动。

- [ ] **Step 1: 先删除清单引用**：从两个 `experts.toml` 移除 `self-improving`，从 `commands/experts.rs` 移除 `SELF_IMPROVING_BUNDLE`、`bundled_skill_dir` match arm 和所有单独嵌入引用。
- [ ] **Step 2: 写迁移函数**：在 central reconcile 前遍历 retired IDs，检查 central path、manifest pending_user_review、各 Agent global skill dirs；仅删除 iyw-claw 自有 link/copy，保留用户目录和市场 marker，清理 manifest 中已退役且未保留的受管条目。
- [ ] **Step 3: 保留运行状态**：迁移函数不得访问或删除 `~/.iyw-claw/self-improving/heartbeat-state.md`、`reflections.md`；日志只记录 skill id、managed path 类型和计数。
- [ ] **Step 4: 保持发布版本可验证**：保留当前未发布工作树版本 `0.0.18`，不在本任务内猜测或递增 SemVer；按 `skill/README.md` 在发布前用 `v0.0.18` 与清单逐项校验，并重新核对每个剩余 expert 都有目录。
- [ ] **Step 5: 静态迁移检查**：搜索两个仓库确保不再出现 `self-improving` 的清单/嵌入/Skill 文件引用（允许 retired tombstone 和运行状态保护代码）；确认领域 Skill ID 未变化。

### Task 5: 修正任务成果的当前会话 lineage

**Files:**
- Modify: `iyw-claw/src-tauri/src/db/service/task_artifact_service.rs`
- Modify: `iyw-claw/src/components/layout/aux-panel-artifacts-tab.tsx`
- Modify: `iyw-claw/src-tauri/src/acp/delegation/listener.rs`

**Interfaces:**
- `conversation_scope_ids(conn, current_id) -> Result<Vec<i32>, DbError>` 返回当前会话、全部祖先和当前会话的全部后代，不包含兄弟分支。
- `TaskArtifactsTab` 的 `scope` 保持用户选择；没有 conversation ID 时 current 查询返回空状态，不隐式改成 all。

- [ ] **Step 1: 记录当前行为**：确认前端 current/all 两个请求参数和后端 `conversation_tree_ids` 的现状；确认成果登记仍允许首轮 prompt 后动态解析会话，不能在 lease 建立时快照空 ID。
- [ ] **Step 2: 实现 lineage 查询**：保留现有后代遍历为 `conversation_descendant_ids`；新增带 visited 集合和合理深度上限的祖先遍历，从当前 ID 向 `parent_id` 收集，再合并当前节点及后代，去重后用于 `ConversationId.is_in`。
- [ ] **Step 3: 保持范围隔离**：不要把 folder_id 加入 current 查询；兄弟会话仍只能在 all scope 出现；无效 ID 继续返回 `invalid_input`。
- [ ] **Step 4: 移除前端静默回退**：删除 `effectiveScope = scope === "current" && !conversationId ? "all" : scope`，让 current 保持 current；toolbar 仍允许用户主动选择 all，current 无 ID 时显示 current empty 状态。
- [ ] **Step 5: 增强可观测性**：登记日志同时记录 resolved conversation id 和 accepted 数；列表日志记录 requested conversation id、scope id count 和 returned count，不记录路径内容。
- [ ] **Step 6: 前后对比检查**：静态确认父会话、当前会话、子会话、兄弟会话和 folder all 的筛选关系；检查 `CurrentReplyArtifacts` 仍使用 current scope，不因 UI 改动变成 folder scope。

### Task 6: 统一收尾验证与交付审计

**Files:**
- Modify only task files above; do not alter existing unrelated dirty files.

- [ ] **Step 1: 搜索闭环审计**：逐个稳定 ID 检查 registry、tool schema、metadata、feature gate、listener dispatch 是否覆盖；检查 profile 字段白名单和 memory resultState 序列化名称。
- [ ] **Step 2: 前后行为矩阵**：用代码级样本记录网关中文/英文查询、profile logged_out/unavailable、memory short/no-evidence/unavailable、artifact current lineage/all scope 的预期结果，并与实现逐项对照。
- [ ] **Step 3: 静态门禁**：运行 `git diff --check`；运行 JSON 解析、TOML 解析和 `cargo fmt --check`（只格式检查，不编译）；检查 Rust 文件函数/文件规模和新增日志敏感字段。
- [ ] **Step 4: 工作区隔离审计**：执行 `git diff --cached --name-status` 和 `git status --short`，确认没有暂存或回退 `auto_continuation.rs`、`connection.rs`、前端上下文/i18n、`.superpowers/` 等无关变更。
- [ ] **Step 5: 真实验证边界报告**：如未运行桌面构建、Cargo tests 或 E2E，最终明确列出；不以静态检查替代运行时验证，不声称完整通过。
- [ ] **Step 6: 分段提交**：按 Task 1/2、Task 3/5、Task 4 分组只暂存本任务文件，提交信息使用 `<type>(scope): <中文动词开头的摘要>`；不 push、不改远端。
