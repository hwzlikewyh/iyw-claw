# Cowart 插件启用与 MCP Apps 纵向接入实施计划

日期：2026-08-28

设计：`docs/superpowers/specs/2026-08-28-cowart-plugin-app-vertical-slice-design.md`

基线：`main@7323931e294cdd227b65f9826e5218bf478c8489`

工作分支：`fix/cowart-plugin-app-20260828`

## 实施原则

- 每个任务只修改列出的状态所有者；共享 contract、数据库和消息 meta 串行处理。
- 不碰原工作区的脏分支；全部代码在独立 worktree 实施。
- 按仓库 `AGENTS.md` 不新增或运行测试文件，使用静态检查和真实安装客户端验证。
- Cowart 在纵向验证完成前保持 Fusion disabled；失败时不重新上架。
- 不记录或输出 token、lease、nonce、完整权限 JSON、Widget HTML 和 canvas 内容。

## Task 1：临时隐藏 Cowart 市场版本

**外部状态：** Fusion Skill Market，skill id `351221104979378176`

1. GET 管理详情，确认 slug=`cowart`、版本=`0.1.28`、当前 active/ready。
2. 调用现有 `/admin/api/skills/set-disabled`，只设置该 skill `disabled=true`。
3. 使用普通用户目录请求验证 Cowart 不再返回；管理详情仍保留 artifact 和版本。
4. 不卸载本机 Cowart，不改本机 activation/grant，不删除 `canvas/`。

**验证：** 管理 envelope `code=1`；普通目录精确搜索结果为 0；其它市场项计数不变。

## Task 2：修复插件卡片和详情导航

**Files:**

- Modify: `src/components/skills/market/plugin-market-preview.tsx`
- Modify: `src/components/skills/skill-market-page.tsx`
- Modify: `src/components/skills/market/view.tsx`
- Modify: `src/hooks/use-skill-market.ts`

1. 将插件卡摘要改成覆盖整张内容区的 button，底部组件标签保持只读。
2. 把 navigation target 处理改为 requestId 状态机：列表已有 slug 时立即打开；否则应用
   market 标准过滤和精确查询，等待对应 request 完成后打开。
3. 去掉“必须先观察到 list.loading=true”这一前置条件；失败保留 target 和重试入口。
4. target 消费只发生在详情已打开或明确 not-found 后，旧请求不能覆盖新请求。

**验证：** 定向 ESLint/TypeScript；静态走查列表已加载、未加载、错误和连续点击四条路径。

## Task 3：提供可解释的插件可用状态

**Files:**

- Modify: `src-tauri/src/acp/builtin_mcp/plugin_catalog.rs`
- Modify: `src-tauri/src/acp/builtin_mcp/gateway.rs`
- Modify: `src-tauri/src/plugin_runtime/registry.rs`
- Modify: `src-tauri/src/plugin_runtime/supervisor.rs`（仅复用已有 quarantine 状态时需要）

1. 将 capability 可用性计算提取为单一决策函数，按固定优先级返回：
   `plugin_unavailable`、`connector_disabled`、`permission_pending`、
   `runtime_quarantined`、`available`。
2. search/read 返回 `unavailable_reason`，invoke 对不可用能力返回同一稳定 code 和原因。
3. unsupported Agent 继续 fail closed；若 capability 不展示，search 的宿主 enable 能力仍可按
   明确插件启用意图被发现。
4. catalog digest 继续包含 registry digest，不把 live runtime 噪声写入持久目录摘要。

**验证：** Rust fmt、Cargo metadata；逐条件静态验证 search/read/invoke 一致。

## Task 4：实现 workspace/Agent 显式授权

**Files:**

- Modify: `src-tauri/src/acp/builtin_mcp/gateway.rs`
- Modify: `src-tauri/src/acp/builtin_mcp/handler.rs`
- Modify: `src-tauri/src/db/service/plugin_runtime_state_service.rs`
- Modify: `src-tauri/src/plugin_runtime/registry.rs`

1. 新增固定 capability `iyw.plugins.enable.request.v1`，输入只有 `plugin_slug`。
2. Handler 从 trusted registry 读取版本、Connector 和 permission ceiling，使用现有
   `ask_user_question` 展示当前 workspace/Agent 与权限摘要。
3. 将 `approve_plugin` 改为 scope-safe upsert：
   - 保留安装默认的空 workspace disabled/pending 行；
   - 为每个 HostGateway Connector upsert 精确 workspace+agent activation；
   - 为精确 workspace+permission digest upsert granted permission；
   - 不覆盖其它 workspace/Agent，不把 grant 扩为 global。
4. transaction 成功后 reconcile registry；失败则 generation 不变、runtime 不启动。
5. 已安装正确版本只授权，不重复下载；未安装继续走现有 install request。

**验证：** 用本机只读 SQLite 快照记录批准前后行；拒绝路径零写入；批准后当前作用域
available、另一个 workspace 仍 unavailable。

## Task 5：建立 App launch broker 与可信 instance

**Files:**

- Add: `src-tauri/src/plugin_runtime/app_launch_broker.rs`
- Modify: `src-tauri/src/plugin_runtime/mod.rs`
- Modify: `src-tauri/src/plugin_runtime/app_host.rs`
- Modify: `src-tauri/src/plugin_runtime/router.rs`
- Modify: `src-tauri/src/plugin_runtime/types.rs`
- Modify: `src-tauri/src/plugin_runtime/global.rs`
- Modify: `src-tauri/src/acp/builtin_mcp/handler.rs`
- Modify: `src-tauri/src/acp/connection.rs`
- Modify: `src-tauri/src/acp/types.rs`（只在 meta 辅助函数不足时）

1. Router 在 capability 成功后根据 trusted manifest 查找唯一 app binding；MCP 返回值不能
   选择 app/resource/plugin/version/display mode。
2. Handler 为 app 调用建立短 TTL、一次性 pending launch ticket，绑定 connection、
   workspace、plugin/version、app、permission revision 和受限 launch payload。
3. CallToolResult 只携带不透明 ticket marker；不返回 HTML、路径、lease 或 nonce。
4. ACP ToolCall/Update 观察到 marker 后，以真实 connection、conversation 和 tool_call_id
   原子 claim ticket，调用 `PluginAppRegistry::create_persisted`。
5. 在现有 tool meta 下写入 `iyw-claw.plugin-app={instanceId}`；live event、snapshot 和历史
   tool_use 复用同一 meta，不新增平行消息存储。
6. ticket 超时、重复 claim、connection/workspace 不匹配全部拒绝；未被 ACP surface 回显的
   ticket 到期回收，不创建 instance。

**验证：** 静态验证 ticket 单次性、身份绑定、取消和大小上限；日志只含 ticket/instance
摘要。对真实 Codex ACP 调用确认 meta 最终落到可见 tool call。

## Task 6：增加 App 打开、恢复与 bridge API

**Files:**

- Add: `src-tauri/src/commands/plugin_apps.rs`
- Add: `src-tauri/src/web/handlers/plugin_apps.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src-tauri/src/web/router.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/plugin_runtime/app_host.rs`
- Modify: `src-tauri/src/plugin_runtime/router.rs`
- Modify: `src-tauri/src/db/service/plugin_app_instance_service.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/types.ts`

1. 提供桌面/Web 共用 core：open/restore、message、teardown；HTTP 只用 POST。
2. open 接受 instanceId、conversationId，后端读取持久 instance 并重新校验版本目录、app
   binding、workspace activation、permission digest 和 conversation 所有权。
3. 校验通过后从同一 plugin/version/connector `resources/read` 读取声明的 `ui://` HTML，
   签发新的短期 lease/nonce，并返回受限 launch DTO；不返回绝对路径。
4. message 复用 `authorize_message`，再按 manifest/grant 交集处理 ui/message、resize、
   clipboard、open-link 和本插件 tools/call；未知方法 fail closed。
5. teardown 撤销 lease、标记 instance inactive，并释放 runtime app 引用；重复调用幂等。
6. 恢复失败返回稳定状态：disabled、permission_changed、version_missing、
   runtime_unavailable、widget_unsupported。

**验证：** 桌面/Web handler 调用同一 core；所有输入有大小限制；HTTP 无 GET 副作用接口。

## Task 7：接入消息渲染和 inline/fullscreen

**Files:**

- Add: `src/components/message/plugin-app-content.tsx`
- Add: `src/components/message/plugin-app-fullscreen-dialog.tsx`
- Modify: `src/components/message/plugin-app-host.tsx`
- Modify: `src/lib/plugin-app-bridge.ts`
- Modify: `src/lib/adapters/ai-elements-adapter.ts`
- Modify: `src/components/message/content-parts-renderer.tsx`
- Modify: `src/components/message/assistant-turn-content.tsx`（仅分组策略需要）
- Modify: `src/i18n/messages/en.json`
- Modify: `src/i18n/messages/zh-CN.json`

1. 从 tool meta 生成显式 `plugin-app` adapted part，包含 instanceId 和 toolCallId，不解析
   MCP 任意 URL。
2. `PluginAppContent` 使用当前 conversationId 调 open/restore，覆盖 loading、ready、
   unsupported、disabled、permission changed 和 runtime error。
3. inline 使用固定响应式高度和稳定占位；Maximize2 图标按钮带 tooltip。
4. fullscreen Dialog 复用同一 instance 和 bridge；切换只改变展示容器，不创建第二 canvas。
5. `PluginAppHost` 把 bridge 消息送入宿主 API；close/unmount 发送 teardown，但 inline 与
   fullscreen 容器切换不得提前 teardown。
6. Widget 继续使用 opaque sandbox、MessageChannel、nonce、source 和 CSP；不开放 Tauri
   globals、Cookie 或宿主 token。
7. `resources/list` 仅返回当前 app binding 声明的资源，`resources/read` 由后端统一路由；资源 `_meta.ui.csp` / `permissions` 只以
   manifest ceiling、用户 grant 与资源声明的交集传给 sandbox，未声明能力默认拒绝。

**验证：** Prettier、定向 ESLint/TypeScript、i18n；人工检查窄/中/宽窗口无重叠或布局跳动。

**当前证据：** 相关前端文件已通过仓库现有 Prettier、定向 ESLint 和 TypeScript；inline 与
fullscreen 使用同一 `PluginAppHost`/instance，未执行正式安装客户端视觉检查。

## Task 8：更新 Cowart 包与 Skills

**Files（独立临时包目录，来源固定为 Fusion 0.1.28 artifact）：**

- Modify: `.iyw-plugin.json`（只提升版本/必要 binding metadata）
- Modify: `skills/cowart-open-canvas/SKILL.md`
- Modify: `skills/cowart-image-gen/SKILL.md`
- Modify: `skills/cowart-image-edit/SKILL.md`
- Preserve: `mcp/generated/*` 与上游 pinned commit，除非 Widget contract 必须修复

1. 从 Fusion 当前 artifact 创建隔离 staging，不编辑安装目录。
2. Skills 只使用 stable IYW capabilities；删除原生 `mcp__cowart_mcp__*`、MCP resources 和
   “重开会话等待加载”路径。
3. render unavailable 且原因为 pending/disabled 时请求 `iyw.plugins.enable.request.v1`，批准
   后重新 search/read/invoke；其它原因直接报告。
4. 校验 tldraw production license/key/domain；无可审计结论则停止上架。
5. 生成下一 patch 版本，通过 Fusion v2 manifest、文件、artifact 和哈希校验。

**验证：** package file list、manifest digest、Skill 触发条件、artifact ready；不执行安装脚本。

**当前证据：** 隔离 staging `F:\projects\iyw\.tmp\cowart-v0.1.29` 的 source-expanded 与
package 共 32 个文件逐文件 SHA-256 一致；manifest/Skills 已升到 `0.1.29`，host permission
包含 `send-message`。未生成或上传正式 artifact，tldraw production license 尚未提供。

## Task 9：静态与真实纵向验证

1. 运行 `cargo fmt --check`、`cargo metadata --no-deps`。
2. 扫描通用 Host/Gateway/API/renderer，禁止出现 `cowart` slug、tool name 或 resource URI
   特判；Cowart 字面量只能存在于独立验收包、发布操作和验收说明中。
3. 运行相关前端文件的 Prettier、ESLint/TypeScript 正常入口；运行 `git diff --check`。
4. 安装开发构建或下一正式包前备份本地数据库；不停止当前正常客户端直到替换窗口。
5. 用本机 pending Cowart 验证：拒绝、批准、同会话重新发现、runtime cold/warm 调用。
6. 验证真实 Widget 首帧非空、inline/fullscreen 同 instance、保存、刷新、历史恢复、图片、
   HTML、复制、下载、DPR 和中等宽度。
7. 验证另一个 workspace/Agent 不继承授权；权限摘要变化需重授权。
8. 验证禁用、升级、卸载、会话断开和应用退出无 runtime/lease 残留，canvas 数据保留。

**停止条件：** 任一层只有静态/HTTP 证据而没有真实 UI 证据时，不声称纵向完成、不重新上架。

**已完成的非 UI 证据：** 隔离 Cowart runtime 真实 JSON-RPC smoke 通过：initialize、13 个
tools、1 个 `ui://` resource、`text/html;profile=mcp-app` 资源读取（6.7 MB）及 render tool
结构化结果均正常。真实 Windows 安装客户端的 Widget 首帧、保存/刷新和跨作用域验证仍未完成。

## Task 10：提交、发布与重新上架

1. 精确暂存任务文件，静态审查 staged diff，提交 feature branch。
2. 推送双远程并独立验证 ref；合并 main 前复核并发提交，不 force-push。
3. 触发下一 patch 客户端 release，等待必需平台、`latest.json` 和 GitHub 正式 Release。
4. 按五平台矩阵上传 Fusion、逐项核对 ready/size/SHA-256，发布并验证双 updater。
5. 在已安装正式客户端重复 Cowart 安装/授权/Widget 纵向验证。
6. 发布 Cowart 下一 patch artifact，最后取消 disabled；普通账号验证目录与首次使用。
7. 清理仅由本任务创建的下载/staging 目录，保留数据库备份和审计记录。
