# Codeg 能力对齐任务清单

日期：2026-08-19
对比基线：Codeg 发布版 `v0.26.2`（tag `881a2d0610e8feb437d3a4cb48e9671db407d2be`）
对比项目：iyw-claw `feat/memory-recall-m0-m2`
补充基线：Codeg tag 后 `main` 已合入的 Qoder 专用集成（当前审计到 `d3b9151e66c115bc65eaf0099b2837bca2d38fb5`）

## 使用说明

本文件取代 `CODEG-GAP-TASKS-2026-08-18.md` 作为最新快照；旧文件保留作历史记录。只记录 Codeg 已有、但 iyw-claw 尚未完整接入，或尚未完成运行时/生产实证的能力。

状态标记：`[ ]` 未完成，`[~]` 部分完成，`[x]` 已完成或不再是待办。

明确排除：

- Skill 相关改动，本轮不修改 Skill。
- MCP HTTP、transport、delegation 相关改动，属于另一条并行任务。
- 不对并发脏工作树做整文件回退、广泛暂存、提交或推送。

## 已确认范围

### v0.26.2 正式发布内容

以下发布说明已在 Codeg `v0.26.2` Release 中核对；其中未列为“已完成”的部分均在后续任务中拆解：

- 状态栏快捷操作菜单（十个入口，远端工作区和宠物为桌面端专属）。
- 侧栏会话通过“添加到会话”写入当前输入框的 @ 提及徽章。
- @ 面板与输入框几何对齐，随输入框、分栏和页面切换移动。
- 连接期间输入 `/` 立即显示加载态，命令到达后原地替换列表。
- Hermes `0.20.4`、CodeBuddy `2.137.1`、Kimi Code `0.37.2`、Grok `1.0.5`、DeepSeek Harness `0.5.0`。
- 修复响应永久卡住、新会话覆盖旧历史、消息落入错误会话、Codex 标题不同步、Grok 用量错误、美元金额/多行公式渲染、隐藏 Office 文件自动预览。

### tag 后 main 的增量

- Qoder 已在 Codeg `main` 中作为专用内置 Agent（版本 `1.1.23`）接入。
- iyw-claw 当前只有受信 registry 中的 Qoder `0.2.14`，尚未具备 Codeg 同等的专用 parser、配置、历史和版本切换集成；该差异单独列在 P1。

### Agent SDK 规模基线

- iyw-claw 当前源码不是“只支持 12 个”：已有 13 个内置 Agent，加上 28 个受信 Registry identity，共 41 个静态身份；其中受信身份仍主要依赖编译期定义。
- Codeg tag 后 `main` 将 Qoder 纳入第 14 个内置 Agent，并提供用户 custom registry；后续接入必须保持动态扩展能力，同时不绕过本地启动安全闸门。
- 本清单把“静态身份数量”与“实际可安装、可启动、可恢复的运行时能力”分开验收，不能用数量或源码存在代替端到端可用性。

## P0：会话正确性与生产实证

- [ ] **实时连接改为一对多路由并向所有界面 fan-out**
  - 现状：`src/contexts/acp-connections-context.tsx` 仍按 `Map<connectionId, contextKey>` 路由，同一智能体被会话页签、任务转写或分屏同时观察时可能互相抢连接。
  - 目标：按 connection、conversation、surface 建立一对多订阅；每个 surface 保留独立序号去重，所有订阅者都能收到增量和 `TurnComplete`。
  - 验收：两个以上界面同时观察同一智能体时，增量不丢、不重复；关闭一个界面不影响其他界面；重连后序号和终态可恢复。
  - 参考：`src/contexts/acp-connections-context.tsx`、`src/lib/api.ts`、`src-tauri/src/acp/manager.rs`。

- [ ] **补齐错过 TurnComplete 的 liveness probe 和连接失效结算**
  - 现状：`acpTouchConnection()` 的返回值被吞掉，prompting/connecting 状态没有主动核对；错过结束通知时仍可能永久显示“响应中”。
  - 目标：触碰连接后检查连接是否仍存活；在超时、连接消失或序号断裂时调用 `markConnectionGone`，主动向后端核对并结算当前回合。
  - 验收：断开、重连、后台任务转写和页面切换均能在有限时间内进入成功/失败/取消终态；停止按钮不再是唯一恢复手段。
  - 参考：`src/lib/api.ts`、`src/contexts/acp-connections-context.tsx`、`src-tauri/src/acp/connection.rs`、`src-tauri/src/acp/prompt_stall.rs`。

- [ ] **防止新 session 覆盖旧 session 关联**
  - 现状：`conversation_service::update_external_id` 无条件覆盖 external id；智能体重连或加载失败后创建的新 session 可能让原会话从列表中消失。
  - 目标：实现事务型 `bind_external_id`，区分 continued-session 与新 session；冲突时保留旧记录的名称、时间、文件夹和转写，并将新 session 独立入库。
  - 验收：重连/加载失败后旧历史仍可从侧栏打开；新旧会话分别显示且不会互相覆盖；重复绑定具备幂等性。
  - 参考：`src-tauri/src/db/service/conversation_service.rs`、`src-tauri/src/acp/agent_input_resume.rs`、`src-tauri/src/acp/session_info.rs`。

- [ ] **发送前拒绝消息落入其他会话拥有的 session**
  - 现状：缺少 `(external_id, agent_type)` holder 冲突检查；绑定未成功前仍可能发送 prompt 或广播 `ConversationLinked`，导致消息写进错误转写。
  - 目标：绑定成功后才能发送 prompt；检测到 session 已属于其他会话时 fail closed，并返回可定位错误，不广播错误关联事件。
  - 验收：任何 holder 冲突都不会产生错误会话消息；重载转写后消息归属与发送前会话一致；错误包含 conversation/session 标识但不泄露凭据。
  - 参考：`src-tauri/src/acp/agent_input_lifecycle.rs`、`src-tauri/src/acp/agent_input_resume.rs`、`src-tauri/src/db/service/conversation_service.rs`。

- [ ] **完成 Fusion catalog、安装、启动和首轮 ACP 的逐 Agent 生产实证**
  - 现状：当前只有源码和 catalog 静态存在性结论，尚未逐 Agent 验证部署 catalog、resolve、平台 artifact、下载校验、本地入库、启动、登录和首轮 ACP。
  - 目标：至少覆盖 Windows 当前目标；记录 catalog revision、版本、platform、artifact、安装状态、启动/登录结果和失败原因。
  - 验收：结果表区分未发布、策略拒绝、包缺失、下载/校验失败、安装失败、启动失败、登录失败和已验证；不能用 HTTP 200 或源码存在代替端到端成功。
  - 参考：`src-tauri/src/acp/version_center/`、`src-tauri/src/acp/binary_cache.rs`、Fusion catalog/resolve 部署记录。

## P1：Agent 注册、身份和协议能力

- [ ] **实现 custom Agent registry CRUD**
  - 现状：Codeg 有 `custom_registry`、数据库持久化、添加/编辑/删除 API、hydrate 和设置界面；iyw-claw 主要依赖编译期 `trusted_agents` 白名单。
  - 目标：定义可持久化 distribution schema，提供添加、编辑、删除、校验、hydrate、启动失败回滚和前端设置入口。
  - 验收：新增 Agent 无需重新编译即可出现在列表并可安装/启动；删除后不可启动；非法命令、参数、环境变量和路径均被拒绝。
  - 参考：Codeg `src-tauri/src/acp/custom_registry.rs`、`src-tauri/src/commands/custom_agents.rs`；iyw-claw `src-tauri/src/acp/registry.rs`。

- [ ] **让 Agent 列表由动态 catalog 驱动**
  - 现状：已有 `project_catalog()`，但 `acp_list_agents_core()` 仍使用 `all_identity_agents()`；远端新增 Agent 不会自动进入选择器。
  - 目标：合并 catalog active/hidden/disabled、排序、版本和本地安装状态；本地受信定义继续作为启动安全闸门。
  - 验收：catalog 新增/隐藏/禁用 Agent 后刷新即可反映；无受信启动定义的 Agent 只能显示为不可启动，远端字段不能直接执行。
  - 参考：`src-tauri/src/commands/acp.rs`、`src-tauri/src/acp/trusted_agents/projection.rs`、`src-tauri/src/acp/remote_registry.rs`。

- [ ] **把稳定 `platform_id` 投影到 Agent 信息和前端**
  - 现状：后端 `TrustedAgentProjection` 已有 `platform_id`，但 `AcpAgentInfo`、设置项、安装/切换请求和缓存仍主要以 `registry_id` 为主。
  - 目标：Rust/TypeScript Agent 信息、安装记录、模型关联和设置选择统一使用稳定 Agent Platform ID；`registry_id` 只作为本地目录键，并兼容旧 payload。
  - 验收：Agent 改名、换 package 或排序后，安装记录和模型关联不漂移；同一 platform_id 在客户端、Fusion 和数据库中可追踪。
  - 参考：`src-tauri/src/acp/registry.rs`、`src-tauri/src/acp/version_center/`、`src/lib/types.ts`。

- [ ] **完成 28 个受信 Agent 的 Provider/endpoint/model 环境注入**
  - 现状：多数 `allowed_env_names` 为空，provider overlay 主要覆盖原有内置 Agent。
  - 目标：逐 Agent 建立经审查的 provider、endpoint、model、credential 环境映射，区分固定、用户可配置和主机托管环境。
  - 验收：每个 Agent 进程只收到允许键；endpoint/model 真实生效；未知键被过滤并记录结构化日志；密钥不进入日志、快照或错误消息。
  - 参考：`src-tauri/src/acp/trusted_agents/`、`src-tauri/src/acp/provider_overlay.rs`、`src-tauri/src/acp/provider_overlay_formats.rs`。

- [ ] **补齐 28 个受信 Agent 的协议能力矩阵**
  - 现状：多数 Agent 声明 `ACP_ONLY`，MCP/resume/load 为 false；Cursor 支持 MCP，DeepSeek 支持完整 session，但声明尚未统一接入所有 UI 和会话闸门。
  - 目标：按真实协议版本确认 `mcp`、`resume`、`load`、取消、权限和 elicitation，并接入 session/new、resume/load、MCP 转发和 UI 能力判断；声明与初始化响应不一致时 fail closed。
  - 验收：能力为 false 时不发送对应字段或调用；能力为 true 时有协议级验证记录；每个 Agent 都有可审计矩阵。
  - 参考：`src-tauri/src/acp/capability_policy/`、`src-tauri/src/acp/connection.rs`、`src-tauri/src/acp/trusted_agents/`。

- [ ] **接入 DeepSeek Harness 中断回合时长回填**
  - 现状：DeepSeek `turn/end` 正常时长已解析，拆分后的 parser 尚未接入 Codeg 的 `backfill_turn_durations` 路径。
  - 目标：为缺失 `turn/end` 的中断回合保留真实已知时间，仅在现有规则满足时补齐估算时长。
  - 验收：正常回合时长不变；进程中断、损坏日志或缺失时间戳不产生负数或跨回合污染；列表和详情统计一致。
  - 参考：`src-tauri/src/parsers/deepseek/mod.rs`、`src-tauri/src/parsers/mod.rs`、`src-tauri/src/db/service/usage_service.rs`。

## P1：v0.26.2 前端工作流

- [ ] **在状态栏加入十入口快捷操作菜单**
  - 现状：打开文件夹、会话管理、导入本地会话、搜索、自动化、远端工作区和宠物等底层入口已有但分散；状态栏没有统一弹出菜单。尚未找到可直接复用的克隆仓库、项目启动器和待办入口，需要先补齐或映射到 iyw-claw 的等价工作流。
  - 目标：左下角状态栏按钮向上弹出十个入口：打开文件夹、克隆仓库、项目启动器、打开远端工作区、会话管理、导入本地会话、搜索、自动化、待办任务、显示宠物；沿用自动化失败数和待办待处理数徽章；远端工作区和宠物仅桌面端显示。
  - 验收：侧边栏折叠后十个入口仍可达；入口权限、徽章、桌面/网页端可见性与原入口一致；点击后只打开一个目标视图。
  - 参考：`src/components/layout/status-bar.tsx`、`src/components/layout/sidebar.tsx`、`src/components/layout/remote-workspace-dropdown.tsx`、`src/components/layout/folder-title-bar.tsx`。

- [ ] **支持从侧栏把会话 @ 进当前输入框**
  - 现状：侧栏会话右键菜单没有“添加到会话”；当前输入框只支持已有 @ 面板和文件树生成的引用徽章。
  - 目标：右键会话选择添加到会话，在光标处插入同一种 mention badge；重复点击幂等，引用包含稳定会话 ID。
  - 验收：光标位于文本中间时插入位置正确；点击两次不重复；切换会话、发送、恢复草稿后引用不串会话。
  - 参考：`src/components/conversations/sidebar-conversation-card.tsx`、`src/lib/session-attachment-events.ts`、`src/components/chat/composer/suggestion/adapters.ts`、`src/components/chat/message-input.tsx`。

- [ ] **让 @ 面板与所属输入框对齐**
  - 现状：`SuggestionPopup` 仍固定 `w-80` 并按 caret 定位；缺少 composer owner、输入框宽度/左右边缘同步、`ResizeObserver` 和隐藏祖先状态跟踪。
  - 目标：@ 面板采用所属输入框的宽度和左右边缘并在其上方展开；随自动化/待办切换、侧栏折叠和分栏拖动重新定位，所属输入框隐藏时同步关闭或隐藏。
  - 验收：面板不漂移、不浮在新页面上；输入框 resize 后几何立即更新；名称和描述可使用完整横向空间且键盘/鼠标交互不回退。
  - 参考：`src/components/chat/composer/rich-composer.tsx`、`src/components/chat/composer/suggestion/suggestion-popup.tsx`、`src/components/chat/message-input.tsx`。

- [ ] **连接期间为 `/` 命令面板提供加载态**
  - 现状：ACP 已计算 `selectorsLoading`，但尚未透传至 `ChatInput`/`MessageInput`；没有命令源时输入 `/` 会立即关闭，命令数组更新也不会主动重跑触发检测。
  - 目标：连接期间输入 `/` 立即打开面板并显示加载行；命令到达后在原面板中替换为列表，无需用户再次输入。
  - 验收：connecting 状态下 `/` 始终有可见反馈；命令到达后保留面板位置、查询和键盘导航；连接失败或取消时退出加载态。
  - 参考：`src/hooks/use-connection-lifecycle.ts`、`src/components/conversations/conversation-detail-panel.tsx`、`src/components/chat/chat-input.tsx`、`src/components/chat/message-input.tsx`。

## P1：Agent 版本、标题与用量

- [ ] **同步内置 Agent 版本和安装源**
  - 现状：Hermes `0.19.0`（且仍走 uvx）、CodeBuddy `2.137.0`、Kimi Code `0.34.0`、Grok `1.0.4`；DeepSeek Harness 已为 `0.5.0`。
  - 目标：升级至 Hermes `0.20.4`、CodeBuddy `2.137.1`、Kimi Code `0.37.2`、Grok `1.0.5`；Hermes 按 Codeg 目标从 uvx 切到固定版本的 `hermes-agent@0.20.4` npm bridge，同时更新版本中心约束、缓存键和迁移策略。该 bridge 不是 Nous 官方包，切换前必须复核固定 tag/commit、安装脚本和供应链边界。
  - 验收：全新安装、升级和已有 profile 均解析到目标版本；旧版本不能绕过最低版本策略；启动 trace 能区分实际运行版本和 registry 版本。
  - 参考：`src-tauri/src/acp/registry/builtin_meta.rs`、`src-tauri/src/acp/npm_runtime.rs`、`src-tauri/src/acp/version_center/`。

- [ ] **接入 Qoder `1.1.23` 的专用集成**
  - 现状：iyw-claw 受信 registry 仅有 Qoder `0.2.14`，没有 tag 后 Codeg main 的专用 parser、配置、历史和版本切换能力。
  - 目标：评估并移植 Qoder 专用启动定义、配置导入、parser/history、版本中心和 UI 标识；保留 `platform_id` 稳定关联，并与受信包策略兼容。本任务不包含 Qoder Skill 和 MCP HTTP/transport/delegation 改动。
  - 验收：Qoder `1.1.23` 可安装、启动、恢复会话、解析标题/用量/工具事件；旧 Qoder 会话可读，新版本升级不覆盖历史。
  - 参考：Codeg tag 后 `main` 的 Qoder 专用实现；iyw-claw `src-tauri/src/acp/registry/`、`src-tauri/src/parsers/`、`src-tauri/src/acp/version_center/`。

- [x] **将 Codex 标题的聊天频道同步改为异步且基于最新标题重读**（2026-08-19）
  - 完成：读取 `~/.codex/session_index.jsonl` 并在 summary cache 外覆盖标题；首次列表在搜索/过滤前通过分块候选查询和逐行 CAS 回填未锁定记录，且不修改 `updated_at`。
  - 完成：Tauri 与 Web 共用同一列表 core；DB 与侧栏 upsert 内联完成，聊天频道同步 detached 执行并循环重读数据库，直到最后发送值与当前标题一致。
  - 保护：手动 `title_locked`、软删除、删除文件夹、loop、会话重绑定和并发自动刷新均由候选过滤及写入时 CAS 复验保护。
  - 验证边界：已完成定向格式、调用链、并发/错误路径和 diff 静态审查；按仓库规则未运行 Cargo check、测试、桌面构建或真实 Telegram 运行验证。
  - 参考：`src-tauri/src/commands/conversations.rs`、`src-tauri/src/commands/conversation_title.rs`、`src-tauri/src/parsers/codex.rs`、`src-tauri/src/db/service/codex_title_service.rs`、`src-tauri/src/web/handlers/conversations.rs`。

- [ ] **修正 Grok 回合用量和仅时长会话统计**
  - 现状：Grok 将 `_meta.totalTokens`（上下文窗口占用）误作实际 input tokens，output/cache read/cache write 仍为 0，显示总量与真实回合消耗严重偏离；只有 duration 没有 usage 的会话也会丢时长汇总。
  - 目标：读取 `turn_completed.usage` 的 input/output/cache read/cache write，按回合累加；上下文圆环继续使用窗口占用；无 usage 时保留已知 duration。
  - 验收：四项用量之和等于 Grok 自报总数；上下文占用与账单用量分开；Codex/Grok 仅时长会话在列表和详情一致显示。
  - 参考：`src-tauri/src/parsers/grok/`、`src-tauri/src/acp/connection.rs`、`src-tauri/src/db/service/usage_service.rs`、`src/contexts/session-stats-context.tsx`。

## P2：渲染与文件工作区

- [ ] **修复美元金额与多行公式渲染**
  - 现状：正文 `$9.99` 至 `$19.99` 会被误识别为公式；多行 `$$…$$`、`\(…\)`、`\[…\]` 在列表或引用块中可能留下多余围栏并吞掉后半行。
  - 目标：美元金额保持普通文本；仅合法公式语法进入数学渲染；覆盖跨多行、列表和引用块的边界解析。
  - 验收：金额、单美元符号和转义文本原样显示；块级公式完整渲染且后续文本不丢失；流式增量和历史重载结果一致。
  - 参考：`src/components/ai-elements/streamdown-plugins.ts`、Markdown/Math remark/rehype 配置。

- [ ] **跳过隐藏文件和隐藏目录的 Office 自动预览**
  - 现状：`.git/`、`.tmp/` 和 macOS `._name.docx` 伴生文件会被自动打开并启动监听进程；文件树手动打开不受影响的行为尚未落实。
  - 目标：自动预览过滤路径中点开头目录、点开头文件和 `._` 伴生文件；保留用户从文件树手动打开的能力。
  - 验收：隐藏路径不会创建标签或 watcher；普通 Word/Excel/PowerPoint 仍自动预览；手动打开隐藏文件仍可预览并正确释放 watcher。
  - 参考：`src/contexts/workspace-context.tsx`、`src/lib/office-preview-prefs.ts`、`src-tauri/src/office_watch/`。

## 已完成或不列为待办的能力

- [x] DeepSeek Harness 已接入目标版本 `0.5.0`；仅“中断回合时长回填”仍在 P1。
- [x] 原有 13 个内置 Agent 和 28 个受信 Agent 的静态注册已存在；仍需完成动态 catalog、环境/协议矩阵和逐 Agent 生产实证。
- [x] Codex 原生标题、手动标题锁定、首次侧栏回填、详情覆盖和聊天频道后台最新值收敛已接入；运行时验证边界见上文。
- [x] 既有 Skill、MCP HTTP、transport、delegation 改动不属于本清单。

## 建议顺序

1. 先完成 P0 的实时 fan-out、liveness、session 绑定和消息归属保护，消除数据正确性风险。
2. 再完成动态 Agent registry、catalog、`platform_id` 和 28 个 Agent 的环境/协议矩阵。
3. 同步落地状态栏、会话 @、@/`/` 面板和 Agent 版本更新，优先验证 DeepSeek Harness 与 Grok 统计。
4. 最后移植 Qoder 专用集成、修复 Markdown/Office 渲染，并执行逐 Agent 的 Fusion 安装启动实证。

## 当前验证边界

本清单基于 Codeg `v0.26.2` Release、tag 后 `main`、iyw-claw 当前源码和定向静态检查生成。当前没有运行 Cargo check、单元/集成测试、桌面构建或逐 Agent 实际安装启动；这些层级不能标记为已验证。文档本身应通过 `git diff --check`，并使用路径限定的 `git status` 检查，避免把并发工作树中的其他文件误归入本任务。
