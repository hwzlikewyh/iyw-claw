# 可选插件运行时与 MCP Apps 宿主实施计划

> 使用 `executing-plans` 按任务串行执行；每个任务完成验证和代码审查后才能进入下一项。

**Goal:** 在不增大原生安装包业务内容的前提下，让 Fusion 审核签名插件支持按需安装、
HostGateway 热注册、本地 MCP 懒启动、通用 MCP Apps Widget 和跨 Agent 明确降级。

**Architecture:** 保留现有 v1 Skill/Connector 与三工具内置网关。v2 插件增加 runtime、
capability、app、permission 和 activation 描述；PluginRegistry 发布不可变快照，Router 按
SessionAuthority 路由，Supervisor 管理 stdio MCP，AppHost 管理隔离 Widget。

**Repositories:**

- Claw：`F:\projects\iyw\.worktrees\plugin-runtime-architecture`
- Fusion：Task 1 创建 `F:\projects\iyw\.worktrees\fusion-plugin-runtime-architecture`
- Cowart/IYW 包：Task 6 再确定独立 checkout，不提前修改上游仓库。

## Global Constraints

- Claw 遵循 `AGENTS.md`：不新增测试文件，不默认运行 Cargo/pnpm 桌面构建；执行静态审查、
  格式、JSON/TOML、`git diff --check` 和可运行的聚焦检查。
- Fusion 遵循 `AGENT.md` 与 `docs/relay-proxy-development-plan.md`：公共 contract、SQL、
  domain/application/adapter 同步，运行相关 Go tests 和 build。
- v1 parser 与 v2 parser 分离；旧包、旧数据库和旧客户端不能被静默升级为新语义。
- 本地可执行 v2 包只允许官方 publisher 和专用插件签名；哈希不能替代签名。
- 不通过 shell 启动插件，不运行 `npm install`、`npx`、`uvx` 或包内安装脚本。
- HostGateway 与 NativeAgent 对同一组件/Agent 互斥；不支持 MCP 的 Agent 不展示幽灵工具。
- 插件程序、plugin-data、workspace 成果物理分离；卸载默认只删除程序和注册状态。
- 每次修改前记录：状态所有者、直接调用方、失败路径、回滚方法、验证方式。
- 不在 catalog/registry/DB 锁内等待网络、子进程或 MCP I/O。
- 每个阶段使用精确路径暂存；不 push、不改远端，除非用户另行要求。

## Task 0：冻结实现基线和跨仓 worktree

**Files:**

- Add: 本计划文件
- Read only: Claw/Fusion 当前指导文件、插件/MCP/Skill Watcher/签名实现

- [x] 合并当前 Claw `main`，确认设计 worktree 除设计文档外干净。
- [x] 创建 Fusion 独立分支 `feat/plugin-runtime-architecture` 和独立 worktree。
- [x] 记录两仓 HEAD、status、已有 v1 parser、DB 表、install-plan、签名器和回滚入口。
- [x] 确认 central Skill Watcher 使用 shared mutation guard，后续不新增第二套文件 watcher。
- [x] 提交本实施计划，验证每份计划/设计文件不超过 300 行。

## Task 1：建立 v2 manifest 与专用签名 contract

**Fusion Files:**

- Modify: `internal/domain/skill/plugin.go`
- Modify: `internal/application/skill/plugin_manifest.go`
- Modify: `internal/application/skill/plugin_components.go`
- Modify: `internal/application/skill/plugin_upload.go`
- Modify: `internal/adapter/mysql/skill_entities.go`
- Modify: `internal/adapter/mysql/skill_plugin_manifest*.go`
- Add: `scripts/mysql/052_extend_plugin_runtime_manifest.sql`
- Modify: `docs/swagger/openapi.yaml`

**Claw Files:**

- Modify: `src-tauri/src/commands/skill_market/plugin_types.rs`
- Modify: `plugin_manifest.rs`, `plugin_components.rs`, `types.rs`
- Add: `src-tauri/src/commands/skill_market/plugin_signature.rs`

- [x] 定义显式 v1/v2 domain 类型；v2 `.iyw-plugin.json` 为权威清单，首版仅允许 HostGateway 并拒绝 native manifest。
- [x] 增加严格 typed runtime/connector/capability/app/permission/activation/routing 结构。
- [x] capability stable ID、schema 文件、entrypoint、resource URI 和组件引用全量校验。
- [x] Fusion 组件表增加 `component_config_json`，保留旧列和旧读取路径。
- [x] install-plan/download metadata 增加 artifact size、plugin signature、key id、manifest digest。
- [x] 使用独立插件公私钥与签名配置，不复用 App/Agent/Tool 密钥。
- [x] Claw 先校验 size/hash，再校验签名和 canonical manifest；无效时 fail closed。
- [x] v1 fixture 行为不变；v2 被旧客户端明确判为 incompatible。
- [x] 运行 Fusion plugin/domain/mysql tests、`go test ./...`、`go build ./...`、OpenAPI parse。
- [x] 运行 Claw rustfmt/JSON/YAML/ESLint 静态检查和 `git diff --check`，完成阶段代码审查。

Task 1 验证说明：Fusion 聚焦测试、OpenAPI parse 和 build 通过；全量 `go test ./...` 已执行，
仅命中未由本分支修改的 migration 045 既有字符串断言失败。Claw 按仓库规则未运行 Cargo
test/check/build，已完成 rustfmt、Cargo metadata、Prettier、ESLint、JSON/YAML 和调用链静态审查。

## Task 2：增加 PluginRegistry、激活和权限状态

**Claw Files:**

- Add: `src-tauri/src/db/migration/*plugin_runtime*.rs`
- Add: `src-tauri/src/db/entities/plugin_activation_policy.rs`
- Add: `plugin_permission_grant.rs`, `plugin_app_instance.rs`
- Add: `src-tauri/src/db/service/plugin_*_service.rs`
- Add: `src-tauri/src/plugin_runtime/registry.rs`
- Modify: `src-tauri/src/app_state.rs`, `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/skill_market/plugin_install*.rs`

- [x] 扩展 installation trust/schema/signature/permission/reconcile 字段并迁移 v1 默认值。
- [x] 建立 component activation、permission grant、app instance 三类独立状态。
- [x] Registry 从 DB/current pointer 建不可变 snapshot、generation 和 digest。
- [x] 安装成功最后发布 generation；发布失败写 repair state，snapshot 保持 fail closed。
- [x] 启动恢复识别残留 staging/trash、缺 current、DB/目录漂移并 fail closed。
- [x] Skill Watcher 只在 shared mutation guard 结束后 reconcile，重复调用保持幂等。
- [x] 卸载不触碰 plugin-data、workspace canvas 和历史 app instance。
- [x] 执行 migration/调用链静态审查、`git diff --check`，请求阶段代码审查。

Task 2 验证说明：SQLite migration 使用同一 `DatabaseTransaction` 并在重入时跳过已存在列，
两轮阶段审查发现的迁移原子性、权限缩减复用、Registry 完成门禁和卸载 suspend 问题均已
修复。按 Claw 仓库规则未运行 Cargo test/check/build；完成定向 rustfmt、Cargo metadata、
文件上限、状态发布/回滚/路径信任静态审查和 `git diff --check`。

## Task 3：实现可信本地 MCP Supervisor 与 Router

**Claw Files:**

- Modify: `src-tauri/Cargo.toml`, `Cargo.lock`
- Add: `src-tauri/src/plugin_runtime/supervisor.rs`
- Add: `process.rs`, `mcp_client.rs`, `router.rs`, `types.rs`, `mod.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/bin_targets/iyw_claw_server.rs`

- [x] 评估并只启用所需 `rmcp` client/stdio feature，不引入平行 MCP SDK。
- [x] 运行实例键固定为 plugin/version/connector/workspace，默认不跨 workspace 共享。
- [x] 使用受管解释器绝对路径和包内 canonical entrypoint，传最小环境。
- [x] single-flight 启动，完成 initialize 和 live tools/resources schema digest 校验。
- [x] 实现并发上限、超时、取消、effectMayHaveOccurred、idle TTL、drain、quarantine。
- [x] 升级后新调用使用新版本；旧 lease 归零后才停止和清理旧版本。
- [x] 应用退出、禁用、卸载、会话撤销不泄漏进程或 pending call。
- [x] 静态检查锁顺序、资源释放和敏感日志，完成阶段代码审查。

Task 3 验证说明：复用 `rmcp 1.3.0`，`cargo tree -e features` 确认 client feature 已启用；
完成 rustfmt、Cargo metadata、diff 和调用链静态审查。按 Claw 仓库规则未运行 Cargo
check/test/build，未执行真实插件 MCP probe；运行时 initialize、live schema、进程退出和
跨平台 Node/Python 路径仍需在 Task 6 纵向切片与安装客户端环境验证。

## Task 4：接入动态 HostGateway 与按需安装确认

**Claw Files:**

- Add: `src-tauri/src/plugin_runtime/catalog.rs`
- Modify: `src-tauri/src/acp/builtin_mcp/capability.rs`, `gateway.rs`, `handler.rs`
- Modify: `src-tauri/src/acp/builtin_mcp/authority.rs`
- Modify: Skill Market API/UI 与用户确认卡相关文件

- [x] 保留静态 BuiltinCapabilityCatalog，新增独立 PluginCapabilityRegistry。
- [x] search/read 按 session authority、workspace、Agent、activation 和 grant 合并过滤。
- [x] 市场仅在明确插件安装意图时返回 `install_required`，不注入完整市场目录。
- [x] 增加固定宿主能力 `iyw.plugins.install.request.v1`，安装前停放并等待用户确认。
- [x] 用户拒绝不创建 staging/DB/runtime；批准后安装、授权当前 workspace 并返回新 catalog digest。
- [x] invoke 每次复核 permission revision 和 actual Agent HostGateway capability。
- [x] 当前会话重新 search 即可调用；不支持 Agent 不展示插件工具并返回稳定 unavailable reason。
- [x] 验证顶层仍只有固定 gateway 工具、无重复 native MCP，完成阶段静态审查。

Task 4 验证说明：仅扩展 Gateway 内部目录，顶层 `tools/list` 仍固定三项；已安装 v2 插件按
Registry/Agent/workspace/activation/permission 过滤，安装请求先读取 Fusion official v2 权威
版本，再复用 `ask_user_question` 停放确认。按 Claw 规则未运行 Cargo check/test/build；已完成
定向 rustfmt、Cargo metadata、diff 和静态调用链审查。真实用户点击确认、Fusion 网络安装、
当前 ACP session 的端到端重 search/invoke 仍需 Task 6/7 runtime 验证。

## Task 5：实现通用 MCP Apps Host

**Claw Files:**

- Add: Rust PluginApp launch/lease/resource handlers
- Add: `src/components/message/plugin-app*.tsx`
- Add: `src/lib/plugin-app-bridge*.ts`
- Modify: ACP event/types、runtime store、adapter、content renderer、web router、Tauri commands

- [ ] PluginRouter 创建并持久化无敏感字段的 PluginAppLaunch。
- [ ] 使用宿主自有 event/DB 恢复，不依赖 ACP adapter 保留第三方 app metadata。
- [ ] 实现不同源/opaque sandbox proxy、MessageChannel、nonce、source 和 lease 校验。
- [ ] CSP 为 manifest ceiling 与用户 grant 交集；不下放 Tauri/API/ACP token。
- [ ] 支持自身 tools/call、可见来源的 ui/message、resize、theme、inline/fullscreen。
- [ ] 限制 HTML、message、参数、结果和频率；关闭前 teardown 并撤销 lease。
- [ ] desktop/server 刷新、恢复、禁用、升级、卸载均显示确定状态。
- [ ] 做恶意 path/nonce/cross-plugin/expired lease 静态与可行运行 probe，阶段代码审查。

## Task 6：制作 Cowart IYW 插件纵向切片

- [ ] 以固定上游 commit 导入预构建 MCP/Widget，不执行安装脚本或依赖下载。
- [ ] 编写 v2 manifest、capability schema、Skills、权限和 HostGateway binding。
- [ ] 图片生成绑定现有 `iyw-image-workflows`，不假设所有 Agent 有 Codex imagegen。
- [ ] 明确 tldraw production license/key/domain；未满足不得发布正式市场版本。
- [ ] 验证打开、保存、选择、插图、HTML、Slides、刷新、升级、卸载和 canvas 保留。
- [ ] 验证中等宽度/DPR 白屏、剪贴板和 server 模式，不以 HTML 返回代替渲染成功。

## Task 7：Agent 兼容、NativeAgent 与完整收尾

- [ ] 逐 Agent 记录 initialize MCP capability、真实工具可见性和一次模型调用。
- [ ] 仅对明确需要者开放 NativeAgent；同一工具不得同时 HostGateway/native 出现。
- [ ] OpenClaw/Pi/ACP-only Agent 只显示 Host-only/Skill-only，不伪造工具兼容。
- [ ] 运行 Fusion 全量测试/build 与 Claw 允许的静态/聚焦验证。
- [ ] 分层报告 contract、artifact、runtime、Agent、Widget、installed-client 验证状态。
- [ ] 使用 `requesting-code-review` 做最终审查，修复 Critical/Important 问题。
- [ ] 使用 `finishing-a-development-branch` 汇总集成选项；不自动 merge 或 push。
