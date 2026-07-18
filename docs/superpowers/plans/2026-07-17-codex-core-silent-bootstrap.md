# Codex 强制内核静默初始化与安装包瘦身 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Codex 固定为 iyw-claw 不可替代的核心内核，在桌面和服务器模式中实现零点击、静默、可恢复的自动初始化，同时保证已就绪启动无网络请求、无安装任务、低延迟进入工作区，并缩小桌面安装包。

**Architecture:** 后端新增进程级 `CodexBootstrapCoordinator` 作为唯一状态源，使用本地就绪指纹完成快速判断，并通过 single-flight、有限重试、崩溃恢复 journal、staging 安装和原子切换准备托管 Node 与 Codex ACP runtime。前端只读取快照、订阅事件和显示统一的“正在准备内核”状态，不再自行串联 Agent 检测、npm 安装或 OfficeCLI。桌面、内嵌 Web 服务和独立服务器复用同一协调器；远程 Web 客户端只调用服务器，由服务器所在主机准备内核。

**Tech Stack:** Rust 2021、Tauri 2、Axum、Tokio、SeaORM、SQLite、Next.js 16、React 19、TypeScript strict、Vitest、next-intl、PowerShell、NSIS。

## 审阅摘要

- 本轮只做 Codex 强制核心、静默初始化、启动性能和安装包瘦身；不做内置浏览器。
- 普通桌面用户不点击初始化、不执行命令、不手动下载；启动后由后端自动检测、准备和修复。
- 正常界面只显示“正在准备内核”和一句通用说明，不显示 Codex/Node/npm/版本/下载日志。
- 已就绪启动只走本地指纹，零网络、零安装、零版本子进程；损坏在首次 spawn 时自动修复并只重试一次。
- 主窗口前只保留严格必要状态，窗口创建后立即归还 UI 事件循环；重任务单并发、后台执行、日志限流。
- Windows 包保留 Node、MinGit、MCP、WebView2 bootstrapper，移除 uv/uvx；安装包目标至少缩小 20MiB。
- macOS/Linux 桌面缺 Node 时应用静默准备；独立服务器使用宿主 Node，官方 Docker 已预装。
- 共 11 个顺序任务；只有最终性能、恢复、双模式构建和安装包验收全部通过才允许发布。

## Global Constraints

- Codex 是强制核心内核；其他 Agent 可以并存，但不能替代 Codex。
- Codex 不可禁用、不可卸载；缺失、版本不符或文件损坏时必须自动修复，不得自动降级到其他 Agent。
- 初始化必须零点击、零配置、零手动下载；应用启动后自动检测并静默补齐。
- 不提供“一键初始化”按钮，也不要求用户执行命令、选择安装路径或配置环境变量。
- 唯一允许阻塞等待用户的场景是账号授权、操作系统权限确认，或有限自动恢复全部失败后的必要操作。
- 正常准备过程的简体中文标题固定为“正在准备内核”。
- 常规初始化界面不得出现 Codex、Node、npm、codex-acp、adapter、下载源、版本号、安装步骤或原始命令输出。
- 技术组件名、版本、下载 URL、校验结果和底层错误只能进入内部日志及用户主动打开的诊断详情。
- 不显示连续成功提示，不显示伪造百分比，不在准备完成后弹成功 toast。
- 准备界面延迟 300ms 显示；300ms 内完成时不得闪烁弹窗。
- 应用外壳首次可见不超过 300ms。
- 本地就绪检测 P95 不超过 200ms。
- Codex 已就绪且本地账户会话有效时，进入可交互工作区 P95 不超过 500ms。
- Codex 已就绪启动时网络请求为 0，安装任务为 0，不执行完整 Agent preflight，不启动版本子进程。
- 快速指纹不递归扫描 `node_modules`；首次 Codex spawn 若确认是 command/module 缺失或 runtime 损坏，必须自动作废 stamp、修复并只重试 spawn 一次。
- 零网络验收使用“本地已就绪、未启用聊天渠道/远程工作区/联网自动化”的标准配置；用户明确启用的持续联网业务不计入，但 Codex、账户、可选工具同步和更新检查在启动阶段仍必须为 0。
- UI 主线程单次 long task 小于 50ms；文件扫描、校验、解压和安装均在 Rust 后台任务或 `spawn_blocking` 中执行。
- 每个进程同时只允许一个下载/校验/解压/npm install 重任务；Node 与 Codex 顺序准备，可选 Agent 复用同一 heavy-work semaphore，避免首次启动争抢 CPU、磁盘和带宽。
- 下载必须流式写盘，禁止将归档整体读入内存；npm stdout/stderr 经过 secret redaction、单行 8KiB 与总保留 64KiB 上限，诊断写入最多每秒 4 批，原始逐行输出不得广播到前端。
- 同一进程的多个窗口只能产生一个 Codex 准备任务；独立服务器的多个浏览器客户端同样只能产生一个任务。
- 自动准备最多执行 3 次尝试，瞬时故障的退避固定为 1 秒、4 秒；磁盘不足、权限拒绝、平台不支持不做无意义重试。
- 崩溃、中途退出、断网和磁盘不足不得留下可被识别为“已就绪”的半安装状态。
- 内核初始化不得自动重启；新安装和自动修复正常路径重启次数固定为 0。既有存储设置若需要重启，继续走独立设置流程，不由 bootstrap 触发。
- 可选 Agent 不在首次启动时批量下载；用户首次选择并连接时由后端静默按需准备。
- OfficeCLI、Agent Reach、OpenCLI、uv/uvx 和其他非核心工具不进入启动关键路径。
- Windows 标准桌面安装包继续包含托管 Node、MinGit、`iyw-claw-mcp` 和 WebView2 bootstrapper，优先保证 Codex 首次可用。
- macOS 与 Linux 桌面版缺少 Node 时由应用按固定清单静默准备；独立服务器由宿主提供 Node，官方 Docker 镜像继续预装 Node。
- 普通桌面用户任何平台都不需要自行下载 Node 或 Codex；独立服务器缺少宿主 Node 时只返回产品级运行环境错误和诊断，不提供下载链接或“一键下载”按钮，Docker 用户不触发此分支。
- 标准桌面安装包移除 `uv`、`uvx` sidecar；需要 Python Agent 或联网工具时由应用静默按需下载到私有缓存。
- WebView2 继续使用 `embedBootstrapper`，不得为了减少约 1.8MB 而改成不可靠的 `skip`。
- Rust release profile 固定使用 `codegen-units = 1`、`lto = "thin"`、`opt-level = 3`、`strip = "symbols"`。
- 静态资源去重和 Monaco 裁剪不在本计划实施范围内，待安装包瘦身数据稳定后单独灰度。
- 内置浏览器不在本计划实施范围内；下文“远程 Web 客户端”只指项目现有的 server 访问方式，不新增浏览器窗口、浏览器控件或网页自动化能力。
- 桌面 `setup` 在主窗口创建前只允许执行：数据根解析与环境固定、SQLite 打开/迁移/待恢复应用、Agent storage 根激活、系统代理与首窗外观快照加载、核心 managed state 构造；不得启动下载、子进程、目录扫描或远程请求。
- 数据库就绪后必须立即创建主窗口；持久化日志级别、托盘、delegation 配置与 listener、聊天渠道、内嵌 Web 服务、automation、GC、缓存清理和本地分发维护全部移到窗口创建后。
- 窗口创建后的服务按优先级后台启动：Codex fast path/prepare 最先，交互服务其次，纯维护任务等待 Codex `Ready` 后再延迟 2 秒；任何阶段不得在 Tauri UI 线程执行阻塞 I/O。
- 不增加新的运行时依赖；Node 归档继续使用官方固定版本和 SHA-256，ZIP 复用现有 `async_zip`，`.tar.gz` 复用现有 `flate2` 与 `tar`，下载复用现有 `reqwest`。
- 不记录 access token、refresh token、密码或 provider key；错误日志必须经过现有 secret redaction。
- 新增函数不超过 50 行，新增/拆出的源码文件不超过 300 行，嵌套不超过 3 层，位置参数不超过 3 个；超过 3 个输入统一使用 request struct。
- 既有超限文件只保留 adapter/wiring，不新增业务职责且行数不得净增加；Task 4 必须把 `src-tauri/src/lib.rs` 的 startup orchestration 移入 `desktop_startup/`，使该文件净减少至少 350 行；Task 2/9 必须把 `commands/acp.rs` 的 runtime preparation/activation 移入新模块。巨型 command registry 和其他历史模块的全量拆分不与本次内核初始化混做，以免扩大 IPC 回归面。
- 当前只创建本任务文档；实施前必须由用户明确批准。未经再次明确要求，不执行 commit、push、merge、rebase 或其他 Git 历史操作。

---

## Target User Flow

### 已就绪启动

1. Rust 进程启动并挂载协调器。
2. 协调器只读取少量固定文件并验证路径边界，不运行 `node --version`、`npm`、`codex` 或网络探测。
3. 前端读取本地账户快照和 Codex 快照。
4. 快照为 `ready` 时直接进入工作区；不显示准备界面。

### 首次启动或自动修复

1. 应用外壳立即渲染，工作区暂时设为 `inert`。
2. 后端 single-flight 任务验证托管 Node、准备固定版本 Codex ACP runtime、同步 provider overlay 与本地凭证。
3. 任务超过 300ms 时，界面只显示“正在准备内核”和通用说明。
4. 准备完成后直接解除工作区阻塞，不显示成功 toast。
5. 若需要登录，账户授权界面继续负责用户交互；内核文件准备与登录可并行，凭证投射在授权完成后补做。
6. 有限自动恢复全部失败后，才显示“重试”和“查看诊断”。

### 远程 Web 客户端

1. 浏览器只调用远程 iyw-claw-server 的 API。
2. 内核检测、下载、解压、安装与日志全部发生在服务器主机。
3. 浏览器本地不得下载 Node、Codex runtime 或任何 sidecar。

---

## Public State Contract

后端对前端只暴露产品级状态，不暴露底层安装阶段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexBootstrapPhase {
    Checking,
    Preparing,
    Ready,
    ActionRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexBootstrapReason {
    NetworkUnavailable,
    DiskFull,
    PermissionDenied,
    IntegrityMismatch,
    RuntimeUnavailable,
    UnsupportedPlatform,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexBootstrapSnapshot {
    pub phase: CodexBootstrapPhase,
    pub attempt: u8,
    pub reason: Option<CodexBootstrapReason>,
    pub can_retry: bool,
    pub generation: u64,
}
```

内部控制枚举固定为：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapTrigger {
    Startup,
    FrontendMount,
    ManualRetry,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalStage {
    PreparingNode,
    PreparingCodex,
    Activating,
    WritingStamp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeRuntimeKind {
    Managed,
    Host,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairReason {
    StampMissing,
    StampInvalid,
    VersionMismatch,
    PlatformMismatch,
    NodeMissing,
    NodeLocatorMismatch,
    CommandMissing,
    PackageHashMismatch,
    UnsafePath,
}

#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("network unavailable")]
    NetworkUnavailable,
    #[error("disk is full")]
    DiskFull,
    #[error("permission denied")]
    PermissionDenied,
    #[error("integrity mismatch")]
    IntegrityMismatch,
    #[error("unsupported platform")]
    UnsupportedPlatform,
    #[error("required host runtime is unavailable")]
    HostRuntimeUnavailable,
    #[error("unsafe runtime path")]
    UnsafePath,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("background task failed")]
    Join(#[from] tokio::task::JoinError),
    #[error("agent preparation failed")]
    AgentPreparation,
}
```

底层错误在构造 `BootstrapError` 前写入经过 secret redaction 的 tracing；公开枚举本身不携带原始字符串。

固定 transport 名称：

```text
codex_bootstrap_get_state
codex_bootstrap_start
codex_bootstrap_retry
app://codex-bootstrap
```

`reason` 只用于选择本地化用户提示；原始错误不进入快照。

## Runtime Files

所有路径均位于当前 `AgentStoragePaths` 根目录下：

```text
runtime/
  node/
    current.json                  # 仅 desktop managed 模式
    <version>/<platform>/         # 仅 desktop managed 模式
  npm/
    codex-acp/<registry-version>/<platform>/
  bootstrap/
    codex-ready.v1.json
    codex-journal.v1.json
  staging/
    node-<uuid>/
    npm-codex-acp-<uuid>/
  trash/
    node/
    npm/
```

就绪指纹只保存非敏感数据：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexReadyStamp {
    pub schema_version: u8,
    pub registry_id: String,
    pub registry_version: String,
    pub platform: String,
    pub node_runtime_kind: NodeRuntimeKind,
    pub node_version: String,
    pub node_locator_sha256: String,
    pub command_relative_path: String,
    pub package_json_sha256: String,
}
```

`node_locator_sha256` 是 node/npm 两个 canonical path 经过长度前缀编码后的 SHA-256，不保存明文宿主路径。`Managed` 模式要求两个路径都位于私有 `runtime/node`；`Host` 模式只允许 `server-runtime`，每次启动用纯文件系统 PATH 解析得到路径后比较该指纹，绝不依据 stamp 删除、覆盖或移动宿主文件。

journal 只在运行中的准备或切换阶段存在；写入采用同目录临时文件、flush、rename：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexBootstrapJournal {
    pub schema_version: u8,
    pub generation: u64,
    pub target_registry_version: String,
    pub attempt: u8,
    pub stage: JournalStage,
    pub after_activation_stage: Option<JournalStage>,
    pub staging_relative_path: String,
    pub final_relative_path: String,
    pub previous_relative_path: Option<String>,
}
```

---

## File Map

### 新增

- `src-tauri/src/codex_bootstrap/mod.rs`：模块出口与共享常量。
- `src-tauri/src/codex_bootstrap/model.rs`：公开快照、就绪 stamp、journal 和错误分类。
- `src-tauri/src/codex_bootstrap/fingerprint.rs`：纯本地快速就绪判断。
- `src-tauri/src/codex_bootstrap/managed_node_install.rs`：托管 Node 缺失时的固定版本下载、校验与原子恢复。
- `src-tauri/src/codex_bootstrap/coordinator.rs`：single-flight、有限重试、journal 恢复和事件发布。
- `src-tauri/src/codex_bootstrap/tests.rs`：协调器与恢复测试。
- `src-tauri/src/commands/codex_bootstrap.rs`：Tauri command 与共享 core。
- `src-tauri/src/web/handlers/codex_bootstrap.rs`：Axum handler。
- `src-tauri/src/acp/runtime_prepare.rs`：可选 Agent 的按需 single-flight 准备。
- `src-tauri/src/acp/runtime_activation.rs`：staging/final/previous 原子切换与 recorder 接口。
- `src-tauri/src/acp/runtime_work_limiter.rs`：Codex 与可选 Agent 共享的单重任务 semaphore。
- `src-tauri/src/startup_maintenance.rs`：应用版本 marker 与就绪后本地分发维护。
- `src-tauri/src/desktop_startup/mod.rs`：桌面启动模块出口和小型 orchestration。
- `src-tauri/src/desktop_startup/pre_window.rs`：数据根、数据库、Agent storage、代理和首窗外观前置条件。
- `src-tauri/src/desktop_startup/post_window.rs`：首窗后的分层后台服务启动与失败隔离。
- `src-tauri/src/desktop_startup/metrics.rs`：单调时钟阶段 recorder 与无敏感信息 tracing。
- `src-tauri/src/desktop_startup/tests.rs`：首窗前后偏序、零网络和失败隔离测试。
- `src/components/account/startup-codex-gate.test.tsx`：零点击门禁行为测试。
- `src/components/account/account-profile-panel.test.tsx`：启动阶段远程头像零请求测试。
- `src/components/settings/acp-agent-settings.test.tsx`：Codex 强制策略 UI 回归测试。
- `src/components/settings/internet-tools-settings.test.tsx`：联网工具懒准备测试。
- `src/contexts/acp-connections-context.test.tsx`：可选 Agent 首次连接准备测试。
- `src/lib/codex-bootstrap.ts`：前端状态类型、订阅与快照收敛。
- `scripts/verify-desktop-bundle.ps1`：安装包内容和体积验收。
- `scripts/verify-codex-bootstrap.ps1`：Windows 端到端恢复验收。
- `src-tauri/src/commands/startup_metrics.rs`：桌面启动性能本地 tracing 入口。
- `src/lib/startup-metrics.ts`：Paint/interactive/long-task 指标采集。

### 修改

- `src-tauri/src/lib.rs`
- `src-tauri/src/app_state.rs`
- `src-tauri/src/bin_targets/iyw_claw_server.rs`
- `src-tauri/src/web/mod.rs`
- `src-tauri/src/web/router.rs`
- `src-tauri/src/web/handlers/mod.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/commands/acp.rs`
- `src-tauri/src/acp/mod.rs`
- `src-tauri/src/acp/error.rs`
- `src-tauri/src/acp/npm_runtime.rs`
- `src-tauri/src/process/managed_node.rs`
- `src-tauri/src/process.rs`
- `src-tauri/src/db/service/agent_setting_service.rs`
- `src-tauri/src/commands/iyw_account.rs`
- `src/components/account/startup-codex-gate.tsx`
- `src/components/settings/acp-agent-settings.tsx`
- `src/contexts/acp-connections-context.tsx`
- `src/lib/api.ts`
- `src/lib/tauri.ts`
- `src/lib/types.ts`
- `src/i18n/messages/en.json`
- `src/i18n/messages/zh-CN.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/build.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/scripts/prepare-sidecars.mjs`
- `src-tauri/src/acp/binary_cache.rs`
- `src-tauri/src/commands/internet_tools.rs`
- `.github/workflows/release-tauri.yml`

---

### Task 1: 状态模型与快速就绪指纹

**Files:**
- Create: `src-tauri/src/codex_bootstrap/mod.rs`
- Create: `src-tauri/src/codex_bootstrap/model.rs`
- Create: `src-tauri/src/codex_bootstrap/fingerprint.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `CodexBootstrapSnapshot`、`CodexReadyStamp`、`CodexBootstrapJournal`。
- Produces: `check_codex_ready(request: ReadyCheckRequest<'_>) -> ReadyCheck`。
- Consumes: `registry::get_agent_meta(AgentType::Codex)`、`npm_runtime::private_npm_prefix`、desktop managed Node `current.json` 或 server host PATH 快照。

- [ ] **Step 1: 写失败测试，固定就绪判断的最小文件集合**

```rust
#[test]
fn ready_check_accepts_matching_stamp_without_spawning_a_process() {
    let fixture = ReadyFixture::matching_codex();
    assert_eq!(
        check_codex_ready(fixture.ready_request(NodeRuntimeKind::Managed)),
        ReadyCheck::Ready(fixture.expected_stamp())
    );
    assert_eq!(fixture.process_probe_count(), 0);
    assert_eq!(fixture.network_probe_count(), 0);
}

#[test]
fn ready_check_rejects_missing_command_or_changed_package_hash() {
    let fixture = ReadyFixture::matching_codex();
    fixture.remove_codex_command();
    assert!(matches!(
        check_codex_ready(fixture.ready_request(NodeRuntimeKind::Managed)),
        ReadyCheck::Repair(RepairReason::CommandMissing)
    ));
}

#[test]
fn stamp_cannot_escape_private_storage() {
    let fixture = ReadyFixture::with_command_path("../../system/codex-acp.cmd");
    assert!(matches!(
        check_codex_ready(fixture.ready_request(NodeRuntimeKind::Managed)),
        ReadyCheck::Repair(RepairReason::UnsafePath)
    ));
}

#[test]
fn host_node_ready_check_uses_path_files_without_version_process() {
    let fixture = ReadyFixture::matching_host_codex();
    assert!(matches!(
        check_codex_ready(fixture.host_ready_request()),
        ReadyCheck::Ready(_)
    ));
    assert!(!fixture.paths().managed_node_current().exists());
    assert_eq!(fixture.process_probe_count(), 0);
}
```

- [ ] **Step 2: 运行测试并确认模块尚不存在**

Run from `src-tauri/`:

```powershell
cargo test --lib codex_bootstrap::fingerprint --features test-utils
```

Expected: FAIL because `codex_bootstrap` and `check_codex_ready` do not exist.

- [ ] **Step 3: 实现公开模型和纯文件系统就绪判断**

`check_codex_ready` 必须按固定顺序执行：

1. 读取不超过 16KiB 的 `codex-ready.v1.json`。
2. 校验 `schema_version == 1`、`registry_id == "codex-acp"`、registry 目标版本和当前平台。
3. `Managed`：读取不超过 16KiB 的 `current.json`，校验版本、平台和私有路径边界；`Host`：只从 request 的 PATH 快照按平台规则解析 node/npm 普通文件，禁止读取 `current.json`。
4. 对 node/npm canonical path 做长度前缀编码并计算 locator SHA-256；校验 kind 与 stamp 一致。Host 路径只用于执行和比较，不进入公开快照或日志。
5. 验证 Codex command 和目标包 `package.json` 是私有目录内的普通文件。
6. 计算单个目标 `package.json` 的 SHA-256，并与 stamp 比较。
7. 不遍历 `node_modules`，不哈希 Node 可执行文件，不执行子进程，不访问数据库，不访问网络。

固定返回类型：

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyCheck {
    Ready(CodexReadyStamp),
    Repair(RepairReason),
}

pub struct ReadyCheckRequest<'a> {
    pub paths: &'a AgentStoragePaths,
    pub node_kind: NodeRuntimeKind,
    pub host_path: Option<&'a OsStr>,
}

pub fn check_codex_ready(request: ReadyCheckRequest<'_>) -> ReadyCheck;
```

- [ ] **Step 4: 将文件校验封装为后台调用**

```rust
let check = tokio::task::spawn_blocking(move || {
    check_codex_ready(ReadyCheckRequest {
        paths: &paths,
        node_kind,
        host_path: host_path.as_deref(),
    })
})
    .await
    .map_err(BootstrapError::Join)?;
```

不得从 React 主线程枚举 runtime 目录。

- [ ] **Step 5: 运行定向测试与格式检查**

```powershell
cargo test --lib codex_bootstrap::fingerprint --features test-utils
cargo fmt --check
git diff --check -- src-tauri/src/codex_bootstrap src-tauri/src/lib.rs
```

Expected: tests PASS; both checks exit 0. Do not commit.

---

### Task 2: 托管 Node 自动修复与显式 npm 路径

**Files:**
- Create: `src-tauri/src/codex_bootstrap/managed_node_install.rs`
- Create: `src-tauri/src/acp/runtime_activation.rs`
- Create: `src-tauri/src/acp/runtime_work_limiter.rs`
- Modify: `src-tauri/src/process/managed_node.rs`
- Modify: `src-tauri/src/process.rs`
- Modify: `src-tauri/src/commands/acp.rs`
- Modify: `src-tauri/src/acp/npm_runtime.rs`

**Interfaces:**
- Produces: `ensure_managed_node_ready(request: NodeProvisionRequest<'_>) -> Result<ManagedNodeRuntime, BootstrapError>`。
- Produces: `ManagedNodeRuntime { kind: NodeRuntimeKind, node: PathBuf, npm: PathBuf, version: String, platform: String }`。
- Changes: `run_npm_streaming` 与 `verify_private_npm_package_version` 都接收包含显式 `npm_program: &Path` 的 request struct，禁止回退到裸 `npm` 或进程 PATH。
- Changes: `acp_prepare_npx_agent_core` 改为单一 `AcpPrepareNpxAgentRequest` 参数，不保留多位置参数入口。
- Produces: `RuntimeWorkLimiter`，全进程只创建一个、permit 数固定为 1。

permit 只在顶层 workflow（Codex coordinator、optional-agent coordinator、OfficeCLI/Internet Tools first-use）获取；`ensure_managed_node_ready`、`acp_prepare_npx_agent_core` 和 `activate_runtime_directory` 假定调用者已经持有 permit，禁止嵌套 acquire 造成自锁。

```rust
#[derive(Clone)]
pub struct RuntimeWorkLimiter {
    semaphore: Arc<Semaphore>,
}

impl RuntimeWorkLimiter {
    pub fn new() -> Self;
    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit, BootstrapError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedNodeRuntime {
    pub kind: NodeRuntimeKind,
    pub node: PathBuf,
    pub npm: PathBuf,
    pub version: String,
    pub platform: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeProvisionPolicy {
    DesktopManaged,
    HostOnly,
}

pub struct NodeProvisionRequest<'a> {
    pub paths: &'a AgentStoragePaths,
    pub policy: NodeProvisionPolicy,
    pub fetcher: &'a dyn RuntimeFetcher,
    pub host_path: Option<&'a OsStr>,
    pub force_reinstall: bool,
    pub activation_recorder: &'a dyn RuntimeActivationRecorder,
}

#[async_trait]
pub trait RuntimeFetcher: Send + Sync {
    async fn download_to(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
    ) -> Result<(), BootstrapError>;
}

pub async fn ensure_managed_node_ready(
    request: NodeProvisionRequest<'_>,
) -> Result<ManagedNodeRuntime, BootstrapError>;

pub struct AcpPrepareNpxAgentRequest<'a> {
    pub agent_type: AgentType,
    pub registry_version: Option<String>,
    pub version_override: Option<String>,
    pub clean_first: bool,
    pub task_id: &'a str,
    pub db: &'a AppDatabase,
    pub emitter: &'a EventEmitter,
    pub managed_node: &'a ManagedNodeRuntime,
    pub activation_recorder: &'a dyn RuntimeActivationRecorder,
    pub output_sink: &'a dyn NpmOutputSink,
}

pub(crate) async fn acp_prepare_npx_agent_core(
    request: AcpPrepareNpxAgentRequest<'_>,
) -> Result<String, AcpError>;
```

固定 desktop manifest：

```rust
struct NodeArchiveSpec {
    os: &'static str,
    arch: &'static str,
    version: &'static str,
    platform: &'static str,
    url: &'static str,
    sha256: &'static str,
}

const NODE_DESKTOP_TARGETS: &[NodeArchiveSpec] = &[
    NodeArchiveSpec {
        os: "windows",
        arch: "x86_64",
        version: "24.0.0",
        platform: "win-x64",
        url: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-win-x64.zip",
        sha256: "3d0fff80c87bb9a8d7f49f2f27832aa34a1477d137af46f5b14df5498be81304",
    },
    NodeArchiveSpec {
        os: "windows",
        arch: "aarch64",
        version: "24.0.0",
        platform: "win-arm64",
        url: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-win-arm64.zip",
        sha256: "03b6676f4872fbe4645113de8e23da834a7c1464045369f2b7a374bf482a5e12",
    },
    NodeArchiveSpec {
        os: "windows",
        arch: "x86",
        version: "22.23.1",
        platform: "win-x86",
        url: "https://nodejs.org/dist/v22.23.1/node-v22.23.1-win-x86.zip",
        sha256: "e298b368aad86c571447a3650db3ce19063373ffd39d6d73d014a5d9ad31dc62",
    },
    NodeArchiveSpec {
        os: "macos",
        arch: "x86_64",
        version: "24.0.0",
        platform: "darwin-x64",
        url: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-darwin-x64.tar.gz",
        sha256: "f716b3ce14a7e37a6cbf97c9de10d444d7da07ef833cd8da81dd944d111e6a4a",
    },
    NodeArchiveSpec {
        os: "macos",
        arch: "aarch64",
        version: "24.0.0",
        platform: "darwin-arm64",
        url: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-darwin-arm64.tar.gz",
        sha256: "194e2f3dd3ec8c2adcaa713ed40f44c5ca38467880e160974ceac1659be60121",
    },
    NodeArchiveSpec {
        os: "linux",
        arch: "x86_64",
        version: "24.0.0",
        platform: "linux-x64",
        url: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-linux-x64.tar.gz",
        sha256: "b760ed6de40c35a25eb011b3cf5943d35d7a76f0c8c331d5a801e10925826cb3",
    },
    NodeArchiveSpec {
        os: "linux",
        arch: "aarch64",
        version: "24.0.0",
        platform: "linux-arm64",
        url: "https://nodejs.org/dist/v24.0.0/node-v24.0.0-linux-arm64.tar.gz",
        sha256: "4104136ddd3d2f167d799f1b21bac72ccf500d80c24be849195f831df6371b83",
    },
];
```

实际实现按 `target_os + target_arch` 选择条目，不能只按 `arch` 取第一个匹配项。Windows 使用现有 zip 解压路径，macOS/Linux 使用现有 `flate2 + tar`，不新增 crate。

- [ ] **Step 1: 写失败测试，覆盖已有、缺失、损坏和中断恢复**

```rust
#[tokio::test]
async fn existing_managed_node_returns_without_network() {
    let fixture = ManagedNodeFixture::ready();
    let runtime = ensure_managed_node_ready(
        fixture.request(NodeProvisionPolicy::DesktopManaged),
    )
        .await
        .unwrap();
    assert!(runtime.npm.is_file());
    assert_eq!(fixture.fetch_count(), 0);
}

#[tokio::test]
async fn missing_node_downloads_once_and_activates_atomically() {
    let fixture = ManagedNodeFixture::missing_with_valid_archive();
    let runtime = ensure_managed_node_ready(
        fixture.request(NodeProvisionPolicy::DesktopManaged),
    )
        .await
        .unwrap();
    assert_eq!(fixture.fetch_count(), 1);
    assert!(runtime.node.is_file());
    assert!(!fixture.has_staging_entries());
}

#[tokio::test]
async fn checksum_failure_never_replaces_current_runtime() {
    let fixture = ManagedNodeFixture::ready_with_corrupt_update();
    let before = fixture.current_json();
    assert!(ensure_managed_node_ready(
        fixture.request(NodeProvisionPolicy::DesktopManaged),
    )
        .await
        .is_err());
    assert_eq!(fixture.current_json(), before);
    assert_eq!(fixture.fetch_count(), 2);
}

#[tokio::test]
async fn host_only_uses_path_runtime_without_download() {
    let fixture = ManagedNodeFixture::host_ready();
    let runtime = ensure_managed_node_ready(fixture.host_request())
        .await
        .unwrap();
    assert_eq!(runtime.kind, NodeRuntimeKind::Host);
    assert_eq!(fixture.fetch_count(), 0);
    assert_eq!(fixture.version_process_count(), 1);
}

#[tokio::test]
async fn host_only_missing_node_is_action_required_without_retry() {
    let fixture = ManagedNodeFixture::host_missing();
    assert!(matches!(
        ensure_managed_node_ready(fixture.host_request()).await,
        Err(BootstrapError::HostRuntimeUnavailable)
    ));
    assert_eq!(fixture.fetch_count(), 0);
}

#[tokio::test]
async fn runtime_work_limiter_allows_only_one_heavy_task() {
    let limiter = RuntimeWorkLimiter::new();
    let tracker = HeavyWorkTracker::default();
    futures::future::join_all((0..4).map(|_| tracker.run(&limiter))).await;
    assert_eq!(tracker.max_active(), 1);
}
```

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --lib managed_node_install --features test-utils
```

Expected: FAIL because `ensure_managed_node_ready` does not exist.

- [ ] **Step 3: 实现 Node 下载、校验、解压和原子切换**

- 已有 `current.json` 和文件完整时立即返回，不访问网络。
- 下载写入 `runtime/staging/node-<uuid>/archive.zip.part`。
- 下载完成后先校验固定 SHA-256，再解压到 staging。
- 验证 `node.exe` 与 `npm.cmd` 存在；fast path 不运行版本子进程。
- Windows x86 的 `current.json` 平台值 `win-x86` 必须被路径解析器接受；macOS/Linux 验证 `bin/node` 与 `bin/npm` 的普通文件和 executable bit。
- 将旧目标 rename 到 `runtime/trash/node/`，再将 staging rename 到最终目录。
- `current.json` 最后写入；失败时恢复旧目录。
- 下载连接超时 15 秒、总超时 5 分钟、响应体最大 128MiB。
- `HostOnly` 不创建 managed Node 目录、不下载：从 request 的 PATH 快照解析 node/npm，首次或 locator 变化时只运行一次 `node --version`，要求退出码为 0 且输出可解析为 semver，并返回 `NodeRuntimeKind::Host`；缺失或版本探测无效直接映射 `HostRuntimeUnavailable`，不做 3 次网络重试。
- `force_reinstall=true` 只对 `DesktopManaged` 生效：始终下载到全新 staging、校验并原子切换，不先删除当前 runtime；`HostOnly` 收到该标志时返回 `HostRuntimeUnavailable`，绝不改动宿主 Node。
- Desktop Node 切换也必须调用 `activate_runtime_directory` 和 request 中的 recorder；不得自行 rename。普通设置页测试可用 `NoopActivationRecorder`，Codex coordinator 使用 Task 3 的 journal recorder。
- 清理超过 24 小时且未被 journal 引用的 Node staging。

- [ ] **Step 4: 改造 npm 安装为显式程序路径**

```rust
struct NpmStreamingRequest<'a> {
    npm_program: &'a Path,
    args: &'a [OsString],
    output_sink: &'a dyn NpmOutputSink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpmStream {
    Stdout,
    Stderr,
}

pub trait NpmOutputSink: Send + Sync {
    fn push(&self, stream: NpmStream, line: &str);
    fn finish(&self);
}

struct NpmOutputSinkState {
    stderr_tail: String,
    pending_redacted: String,
    last_flush: Instant,
}

pub struct RedactedNpmOutputSink {
    state: Mutex<NpmOutputSinkState>,
}

impl RedactedNpmOutputSink {
    pub fn new() -> Self;
}

struct NpmRunResult {
    success: bool,
    stderr_tail: String,
}

async fn run_npm_streaming(
    request: NpmStreamingRequest<'_>,
) -> Result<NpmRunResult, AcpError>;
```

生产 sink 先走现有 secret redaction，再按单行 8KiB 截断、累计保留最后 64KiB stderr，并以最多每秒 4 批写 tracing；它不持有 `EventEmitter`，因此 npm 输出不可能进入 Tauri/WebSocket 公共事件。测试 sink 注入 10,000 行输出，断言内存上限、批次数和 secret 不出现在日志。

npm child 固定启用 `kill_on_drop(true)`；取消、应用退出或 task panic 时先终止并 wait child，再交由 journal/staging 恢复逻辑处理目录，禁止留下继续写私有 runtime 的孤儿安装进程。

原子切换接口固定为：

```rust
pub struct RuntimeActivationPlan {
    pub staging: PathBuf,
    pub final_path: PathBuf,
    pub previous_path: PathBuf,
}

pub trait RuntimeActivationRecorder: Send + Sync {
    fn before_activate(
        &self,
        plan: &RuntimeActivationPlan,
    ) -> Result<(), AcpError>;

    fn after_commit(
        &self,
        plan: &RuntimeActivationPlan,
    ) -> Result<(), AcpError>;
}

pub fn activate_runtime_directory(
    plan: RuntimeActivationPlan,
    recorder: &dyn RuntimeActivationRecorder,
) -> Result<(), AcpError>;

pub struct NoopActivationRecorder;

impl RuntimeActivationRecorder for NoopActivationRecorder {
    fn before_activate(&self, _: &RuntimeActivationPlan) -> Result<(), AcpError> {
        Ok(())
    }

    fn after_commit(&self, _: &RuntimeActivationPlan) -> Result<(), AcpError> {
        Ok(())
    }
}
```

`install_private_npm_package` 必须改为 request struct，包含 `&ManagedNodeRuntime`、packages、required commands、`&dyn RuntimeActivationRecorder` 和 `&dyn NpmOutputSink`。现有设置页/可选 Agent 传 `NoopActivationRecorder`；Codex coordinator 传 Task 3 的 journal recorder。不得在异步 runtime 已启动后修改进程级 `PATH`。

安装后的 `npm list --global` 版本验证必须复用同一个 `ManagedNodeRuntime.npm`；测试将 PATH 清空仍应完成 install/verify，证明没有隐藏的裸 `node`、`npm` 或 `npx` 查找。

- [ ] **Step 5: 固定桌面和服务器行为**

- Windows 安装版优先读取安装器已经写入的托管 Node。
- macOS/Linux Tauri 桌面版优先使用已验证私有 Node，缺失时按上述 manifest 静默准备。
- Docker 继续使用 `node:24-bookworm-slim` 中的系统 Node。
- 独立 server runtime 只使用宿主 PATH；若无 Node，进入 `ActionRequired`，由服务器管理员处理。Web 客户端本地不下载。

- [ ] **Step 6: 运行定向验证**

```powershell
cargo test --lib managed_node --features test-utils
cargo test --lib managed_node_install --features test-utils
cargo test --lib runtime_work_limiter --features test-utils
cargo test --lib npm_runtime --features test-utils
cargo clippy --lib --features test-utils -- -D warnings
```

Expected: all commands exit 0.

---

### Task 3: CodexBootstrapCoordinator、single-flight 与崩溃恢复

**Files:**
- Create: `src-tauri/src/codex_bootstrap/coordinator.rs`
- Create: `src-tauri/src/codex_bootstrap/tests.rs`
- Modify: `src-tauri/src/codex_bootstrap/mod.rs`
- Modify: `src-tauri/src/commands/acp.rs`
- Modify: `src-tauri/src/acp/agent_storage_work.rs`

**Interfaces:**
- Produces: `CodexBootstrapCoordinator::new`、`snapshot`、`start`、`retry`。
- Produces: `CODEX_BOOTSTRAP_EVENT = "app://codex-bootstrap"`。
- Consumes: Task 1 fingerprint、Task 2 Node runtime、现有 `acp_prepare_npx_agent_core`。

固定接口：

```rust
#[derive(Clone)]
pub struct CodexBootstrapCoordinator {
    inner: Arc<CoordinatorInner>,
}

pub struct CodexJournalActivationRecorder {
    paths: AgentStoragePaths,
    generation: u64,
    attempt: u8,
    after_commit_stage: JournalStage,
}

pub struct CodexBootstrapConfig {
    pub db: DatabaseConnection,
    pub emitter: EventEmitter,
    pub paths: AgentStoragePaths,
    pub node_policy: NodeProvisionPolicy,
    pub host_path: Option<OsString>,
    pub work_limiter: RuntimeWorkLimiter,
}

impl CodexBootstrapCoordinator {
    pub fn new(config: CodexBootstrapConfig) -> Self;

    pub async fn snapshot(&self) -> CodexBootstrapSnapshot;
    pub async fn start(&self, trigger: BootstrapTrigger) -> CodexBootstrapSnapshot;
    pub async fn retry(&self) -> CodexBootstrapSnapshot;
    pub async fn managed_node(&self) -> Result<ManagedNodeRuntime, BootstrapError>;
    pub async fn invalidate_and_repair(
        &self,
        cause: CodexLaunchFailure,
    ) -> CodexBootstrapSnapshot;
    pub async fn wait_for_terminal(
        &self,
        generation: u64,
    ) -> CodexBootstrapSnapshot;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexLaunchFailure {
    CommandMissing,
    ModuleMissing,
    NodeRuntimeCorrupt,
}
```

`managed_node()` 只在当前 phase 为 `Ready` 时成功；Managed 从已验证的 stamp/current manifest 重建路径，Host 从协调器捕获的 PATH 快照重新解析并核对 locator hash；两者都不运行版本进程、不访问网络，也不触发第二套 Node 准备流程。

- [ ] **Step 1: 写失败测试，固定 single-flight、fast path、退避和恢复**

```rust
#[tokio::test]
async fn concurrent_starts_share_one_prepare_run() {
    let fixture = CoordinatorFixture::missing_codex();
    let calls = (0..20).map(|_| {
        fixture.coordinator().start(BootstrapTrigger::Startup)
    });
    futures::future::join_all(calls).await;
    fixture.wait_ready().await;
    assert_eq!(fixture.prepare_count(), 1);
}

#[tokio::test]
async fn ready_start_does_not_call_network_or_installer() {
    let fixture = CoordinatorFixture::ready();
    fixture.coordinator().start(BootstrapTrigger::Startup).await;
    fixture.wait_ready().await;
    assert_eq!(fixture.network_count(), 0);
    assert_eq!(fixture.prepare_count(), 0);
}

#[tokio::test]
async fn ready_stamp_repairs_missing_db_version_without_installing() {
    let fixture = CoordinatorFixture::ready_with_missing_db_version();
    fixture.coordinator().start(BootstrapTrigger::Startup).await;
    fixture.wait_ready().await;
    assert_eq!(fixture.db_installed_version().as_deref(), Some("1.1.4"));
    assert_eq!(fixture.prepare_count(), 0);
}

#[tokio::test]
async fn stale_journal_removes_only_its_private_staging_directory() {
    let fixture = CoordinatorFixture::with_interrupted_journal();
    fixture.coordinator().start(BootstrapTrigger::Startup).await;
    fixture.wait_ready().await;
    assert!(!fixture.interrupted_staging().exists());
    assert!(fixture.unrelated_staging().exists());
}

#[tokio::test]
async fn activating_journal_restores_previous_runtime_after_crash() {
    let fixture = CoordinatorFixture::crashed_after_old_runtime_moved();
    fixture.coordinator().start(BootstrapTrigger::Startup).await;
    fixture.wait_ready().await;
    assert!(fixture.previous_runtime_restored());
    assert!(!fixture.has_half_activated_runtime());
}

#[tokio::test]
async fn disk_full_stops_without_retry_loop() {
    let fixture = CoordinatorFixture::disk_full();
    fixture.coordinator().start(BootstrapTrigger::Startup).await;
    let snapshot = fixture.wait_action_required().await;
    assert_eq!(snapshot.attempt, 1);
    assert_eq!(snapshot.reason, Some(CodexBootstrapReason::DiskFull));
}

#[tokio::test(start_paused = true)]
async fn transient_network_failure_uses_exact_retry_schedule() {
    let fixture = CoordinatorFixture::network_unavailable();
    fixture.coordinator().start(BootstrapTrigger::Startup).await;
    fixture.wait_for_attempt(1).await;
    tokio::time::advance(Duration::from_millis(999)).await;
    assert_eq!(fixture.attempt_count(), 1);
    tokio::time::advance(Duration::from_millis(1)).await;
    fixture.wait_for_attempt(2).await;
    tokio::time::advance(Duration::from_secs(4)).await;
    let snapshot = fixture.wait_action_required().await;
    assert_eq!(fixture.attempt_count(), 3);
    assert_eq!(snapshot.reason, Some(CodexBootstrapReason::NetworkUnavailable));
}

#[tokio::test]
async fn permission_and_host_runtime_failures_do_not_retry() {
    for fixture in [
        CoordinatorFixture::permission_denied(),
        CoordinatorFixture::host_runtime_unavailable(),
    ] {
        fixture.coordinator().start(BootstrapTrigger::Startup).await;
        let snapshot = fixture.wait_action_required().await;
        assert_eq!(snapshot.attempt, 1);
    }
}
```

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --lib codex_bootstrap::tests --features test-utils
```

Expected: FAIL because coordinator types do not exist.

- [ ] **Step 3: 实现非阻塞 start 和 single-flight**

`start` 只获取短锁、决定是否 spawn、返回当前快照，不等待下载完成。锁内禁止文件、数据库和网络 I/O。

```rust
struct CoordinatorState {
    snapshot: CodexBootstrapSnapshot,
    running_generation: Option<u64>,
    next_generation: u64,
}
```

相同 generation 只能 spawn 一个任务；任务结束的 finally 路径必须清除 `running_generation`。

- [ ] **Step 4: 实现固定准备流水线**

1. 恢复并清理 stale journal。
2. 执行 Task 1 fast path；ready 时将 stamp 中版本与 SQLite `installed_version` 做一次轻量本地对齐，然后发布 `Ready`。记录已一致时不写数据库。
3. 获取共享 `RuntimeWorkLimiter` permit，获取后重新执行 fast path，避免等待期间已有任务完成却重复安装。
4. 写 journal，stage 为 `PreparingNode`。
5. 调用 Task 2 确保 Node。
6. 更新 journal，stage 为 `PreparingCodex`。
7. 使用 registry 固定版本调用重构后的 Codex npm prepare core。
8. 强制 provider overlay。
9. 本地账户 token 存在时同步 Codex 凭证；未登录不阻塞文件准备。
10. 重新计算并原子写入 `codex-ready.v1.json`。
11. 删除 journal，发布 `Ready` 和一次 `app://acp-agents-updated`；成功、失败和取消路径都释放 permit。

桌面构造 `CodexBootstrapConfig` 时固定 `DesktopManaged`；独立服务器固定 `HostOnly` 并在进程启动时捕获一次 PATH 为 `OsString`。fast path 与 repair 必须使用同一份 `node_policy + host_path`，禁止 fingerprint 认为 Host ready、prepare 却切换到 managed Node。

进入 rename 前先生成固定 trash 路径，将 staging/final/previous 三个相对路径及 `after_activation_stage` 写入 `Activating` journal 并 flush。恢复规则固定为：final 完整则完成切换、清理 previous 并进入记录的下一 stage；final 缺失而 previous 存在则 restore previous；final 与 previous 都不存在则重新准备；所有路径先 canonicalize 并验证位于私有 storage，禁止按 journal 删除范围外路径。非 `Activating` journal 的 `after_activation_stage` 必须为 `None`，无效组合按损坏 journal 处理且只清理已验证的 staging 路径。

`CodexJournalActivationRecorder` 在 `before_activate` 写入上述 `Activating` journal，在 `after_commit` 写入构造时指定的 `after_commit_stage`。Node 切换使用 `PreparingCodex`，Codex package 切换使用 `WritingStamp`；协调器分别传入 Task 2 的 Node request 和 Codex prepare request，不得绕过 `activate_runtime_directory` 自行 rename。

协调器为每次 prepare 创建一个 `RedactedNpmOutputSink` 并传入 `AcpPrepareNpxAgentRequest`；完成或失败路径都调用 `finish()`。公开 `CodexBootstrapSnapshot` 与 `app://codex-bootstrap` 仍只发送 phase/reason/generation，不发送 sink 内容。

普通缺失/版本变化传 `force_reinstall=false`；首次 Codex spawn 被分类为 `NodeRuntimeCorrupt` 时，Desktop repair 传 `force_reinstall=true` 并同时重装 Codex package，避免损坏但仍存在的 `node.exe`/`node` 被文件存在检查误判。Host 模式不尝试覆盖宿主文件，直接进入 `RuntimeUnavailable`。

- [ ] **Step 5: 实现有限重试和错误脱敏**

```rust
const MAX_ATTEMPTS: u8 = 3;
const RETRY_BACKOFFS: [Duration; 2] = [
    Duration::from_secs(1),
    Duration::from_secs(4),
];
```

- 网络超时、连接重置、临时文件占用：最多 3 次总尝试。
- checksum 不匹配：重新下载 1 次；再次不匹配进入 `IntegrityMismatch`。
- 磁盘不足、权限拒绝、平台不支持、server host Node 缺失或版本探测无效：第一次失败即 `ActionRequired`；host runtime 对外 reason 为 `RuntimeUnavailable`。
- snapshot 只含枚举 reason；完整错误经 redaction 后写 tracing。
- 手动 `retry` 开始新 generation，仍受相同上限约束。
- `invalidate_and_repair` 只接受上述三种本地 runtime 故障；认证、provider、工作目录和普通协议错误不得触发重装。

- [ ] **Step 6: 确认存储迁移互斥并运行验证**

准备任务写入阶段持有 `AgentStorageWorkGuard`；fast path 不持有。

```powershell
cargo test --lib codex_bootstrap --features test-utils
cargo test --lib agent_storage_work --features test-utils
cargo clippy --lib --features test-utils -- -D warnings
```

Expected: all commands exit 0.

---

### Task 4: Tauri、Axum、服务器与事件桥接入

**Files:**
- Create: `src-tauri/src/commands/codex_bootstrap.rs`
- Create: `src-tauri/src/web/handlers/codex_bootstrap.rs`
- Create: `src-tauri/src/desktop_startup/mod.rs`
- Create: `src-tauri/src/desktop_startup/pre_window.rs`
- Create: `src-tauri/src/desktop_startup/post_window.rs`
- Create: `src-tauri/src/desktop_startup/metrics.rs`
- Create: `src-tauri/src/desktop_startup/tests.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src-tauri/src/web/router.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/mod.rs`
- Modify: `src-tauri/src/bin_targets/iyw_claw_server.rs`

**Interfaces:**
- Produces Tauri commands and HTTP POST routes with identical names and response types。
- Adds `pub codex_bootstrap: CodexBootstrapCoordinator` to `AppState`。
- Adds `pub runtime_work_limiter: RuntimeWorkLimiter` to `AppState`；桌面 Tauri state、内嵌 Web `AppState` 和可选 Agent coordinator 必须 clone 同一实例。
- Produces `load_desktop_pre_window(request: DesktopPreWindowRequest<'_>) -> Result<DesktopPreWindow, AppCommandError>`。
- Produces `run_desktop_post_window(request: DesktopPostWindowRequest) -> Result<(), AppCommandError>`；该函数只由后台 task 调用。

```rust
pub struct DesktopPreWindowRequest<'a> {
    pub data_dir: &'a Path,
    pub app_version: &'a str,
    pub selected_agent_root: Option<&'a Path>,
    pub recorder: &'a dyn DesktopStartupRecorder,
}

pub struct DesktopPreWindow {
    pub database: AppDatabase,
    pub agent_paths: AgentStoragePaths,
}

pub struct DesktopPostWindowRequest {
    pub app: AppHandle,
    pub data_dir: PathBuf,
    pub codex_bootstrap: CodexBootstrapCoordinator,
    pub recorder: Arc<dyn DesktopStartupRecorder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupEvent {
    DatabaseReady,
    ShellCreated,
    CodexStarted,
    MaintenanceStarted,
}

pub trait DesktopStartupRecorder: Send + Sync {
    fn record(&self, event: StartupEvent);
}
```

生产实现使用进程入口创建的单调时钟，只向 tracing 写 `stage` 与 `elapsed_ms`；测试实现另外保存事件及 fake network/process 计数。不得记录路径、账号或环境变量值。

- [ ] **Step 1: 写失败的 Axum 集成测试**

```rust
#[tokio::test]
async fn bootstrap_routes_share_the_app_state_coordinator() {
    let app = test_router_with_ready_codex().await;
    let first = app.post("/api/codex_bootstrap_start").await;
    let second = app.post("/api/codex_bootstrap_get_state").await;
    first.assert_status_ok();
    second.assert_status_ok();
    second.assert_json_contains(&json!({ "phase": "ready" }));
}

#[tokio::test]
async fn repeated_web_clients_do_not_duplicate_prepare() {
    let fixture = test_router_with_missing_codex().await;
    let requests = (0..10).map(|_| {
        fixture.post("/api/codex_bootstrap_start")
    });
    futures::future::join_all(requests).await;
    fixture.wait_ready().await;
    assert_eq!(fixture.prepare_count(), 1);
}
```

- [ ] **Step 2: 运行测试并确认路由不存在**

```powershell
cargo test --lib codex_bootstrap --features test-utils
```

Expected: FAIL with missing route/handler symbols.

- [ ] **Step 3: 实现共享 core 与三个入口**

```rust
pub async fn codex_bootstrap_get_state_core(
    coordinator: &CodexBootstrapCoordinator,
) -> CodexBootstrapSnapshot;

pub async fn codex_bootstrap_start_core(
    coordinator: &CodexBootstrapCoordinator,
) -> CodexBootstrapSnapshot;

pub async fn codex_bootstrap_retry_core(
    coordinator: &CodexBootstrapCoordinator,
) -> CodexBootstrapSnapshot;
```

Tauri command 只注入 `State<'_, CodexBootstrapCoordinator>`；Axum handler 只读取 `Extension<Arc<AppState>>`。

- [ ] **Step 4: 在所有 AppState 构造点共享同一协调器**

必须更新 `AppState::new_for_test`、独立服务器、桌面内嵌 Web 服务和 Tauri managed state。桌面内嵌 Web 服务必须 clone Tauri 已管理实例，不能创建第二个协调器。

- [ ] **Step 5: 重排桌面首窗关键路径**

`src-tauri/src/lib.rs` 的 `setup` 固定为以下顺序，顺序不得交换：

1. 解析并固定 `IYW_CLAW_DATA_DIR`，保持既有 `IYW_CLAW_HOME` 冲突告警。
2. 只用一次 `block_on(load_desktop_pre_window(request))` 打开/迁移 SQLite、应用 pending restore、确保并加载 Agent storage、应用系统代理，并读取首窗外观快照。
3. `app.manage(database)`，构造并 manage Codex coordinator、共享 `AppState` 所需状态，以及 delegation broker/token/config 等所有 command 注入所需的纯内存状态；此步禁止数据库、文件、网络和子进程 I/O。
4. 创建 `main` window，立即调用 `post_window_setup`，记录 `shell_created` 阶段时间。
5. `spawn(run_desktop_post_window(request))` 后立即从 `setup` 返回。

窗口创建前明确禁止现有这些调用：

```text
logging::init::apply_persisted_level
windows::install_tray_icon
experts::ensure_central_experts_installed
managed_skills::reconcile_all_core
mcp_sync::reconcile_all_managed_mcp
internet_tools::bootstrap_core
gc_orphan_chat_dirs_core
ChatChannelManager::start_background
delegation_commands::apply_persisted_config
DelegationListener::run
lifecycle_subscriber_task
web::load_web_service_config
web::do_start_web_server_tauri
automation::run_automation_engine
```

`run_desktop_post_window` 的调度顺序固定为：

1. 连接 log hub emitter，并异步应用持久化日志级别与读取 tray locale；随后通过 `run_on_main_thread` 安装托盘，失败只记日志。
2. 立即 spawn `codex_bootstrap.start(Startup)`；fast path 和文件校验仍进入 `spawn_blocking`。
3. 启动 delegation 配置/listener/lifecycle、聊天渠道、内嵌 Web 服务、idle sweep 和 automation；每个子系统失败隔离，不改变 Codex 快照。
4. GC 与 ACP trash sweep 使用后台任务；Task 8 的本地分发维护等待 Codex `Ready` 后再延迟 2 秒。

不得仅把 `WebviewWindowBuilder` 提前、却继续在 `setup` 主线程执行后续 `block_on`；`setup` 必须在首窗创建后立即交还事件循环。

- [ ] **Step 6: 后端启动后自动 start**

- 桌面：首窗创建后由 `run_desktop_post_window` 第一个后台阶段调用 `start(Startup)`。
- 服务器：`Arc<AppState>` 构造完成后 spawn `start(Startup)`，不得等待 Codex 就绪才 bind HTTP listener。
- 前端仍调用 `start` 作为幂等恢复路径。
- 后端准备不等待前端挂载，不阻塞 Tauri window 创建或 Axum bind。

- [ ] **Step 7: 添加启动顺序回归测试**

`desktop_startup` 测试使用 recorder 固定以下偏序，不依赖源码字符串匹配：

```rust
assert_before(&events, StartupEvent::DatabaseReady, StartupEvent::ShellCreated);
assert_before(&events, StartupEvent::ShellCreated, StartupEvent::CodexStarted);
assert_before(&events, StartupEvent::CodexStarted, StartupEvent::MaintenanceStarted);
assert_eq!(recorder.pre_window_network_count(), 0);
assert_eq!(recorder.pre_window_process_count(), 0);
```

同时用 fake subsystem 验证 delegation 或 Web service 启动失败不会取消 Codex task，且 post-window runner 不在调用者线程执行阻塞 operation。

- [ ] **Step 8: 广播公开快照并运行双模式验证**

事件 payload 与 `CodexBootstrapSnapshot` 完全一致，不发送安装日志。

```powershell
cargo test --features test-utils codex_bootstrap
cargo test --features test-utils desktop_startup
cargo check
cargo check --no-default-features --features server-runtime --bin iyw-claw-server
cargo clippy --no-default-features --features server-runtime --bin iyw-claw-server --lib -- -D warnings
```

Expected: all commands exit 0.

---

### Task 5: 后端强制 Codex 核心策略

**Files:**
- Create: `src/components/settings/acp-agent-settings.test.tsx`
- Modify: `src-tauri/src/acp/error.rs`
- Modify: `src-tauri/src/commands/acp.rs`
- Modify: `src-tauri/src/db/service/agent_setting_service.rs`
- Modify: `src/components/settings/acp-agent-settings.tsx`

**Interfaces:**
- Produces: `AcpError::CoreAgentRequired`。
- Enforces: Codex `enabled == true` and uninstall rejection at backend boundary。
- Consumes: Task 3 coordinator for repair state。
- Changes: preference/update-refresh/uninstall core 全部使用 input/state request struct，删除既有 `clippy::too_many_arguments` 例外。

```rust
pub struct AcpAgentPreferencesInput {
    pub agent_type: AgentType,
    pub enabled: bool,
    pub env: BTreeMap<String, String>,
    pub config_json: Option<String>,
    pub opencode_auth_json: Option<String>,
    pub codex_auth_json: Option<String>,
    pub codex_config_toml: Option<String>,
}

pub struct AcpAgentPreferencesState<'a> {
    pub db: &'a AppDatabase,
    pub emitter: &'a EventEmitter,
}

pub struct AcpAgentPreferencesRefreshState<'a> {
    pub db: &'a AppDatabase,
    pub manager: &'a ConnectionManager,
    pub data_dir: &'a Path,
    pub emitter: &'a EventEmitter,
}

pub struct AcpUninstallRequest<'a> {
    pub agent_type: AgentType,
    pub task_id: &'a str,
    pub db: &'a AppDatabase,
    pub emitter: &'a EventEmitter,
}

pub(crate) async fn acp_update_agent_preferences_core(
    input: AcpAgentPreferencesInput,
    state: AcpAgentPreferencesState<'_>,
) -> Result<(), AcpError>;

pub(crate) async fn acp_update_agent_preferences_and_refresh(
    input: AcpAgentPreferencesInput,
    state: AcpAgentPreferencesRefreshState<'_>,
) -> Result<usize, AcpError>;

pub(crate) async fn acp_uninstall_agent_core(
    request: AcpUninstallRequest<'_>,
) -> Result<(), AcpError>;
```

- [ ] **Step 1: 写失败测试，证明策略不能被前端绕过**

```rust
#[tokio::test]
async fn codex_cannot_be_disabled_through_preferences_core() {
    let result = acp_update_agent_preferences_core(
        AcpAgentPreferencesInput {
            agent_type: AgentType::Codex,
            enabled: false,
            env: BTreeMap::new(),
            config_json: None,
            opencode_auth_json: None,
            codex_auth_json: None,
            codex_config_toml: None,
        },
        AcpAgentPreferencesState { db: &db, emitter: &emitter },
    )
    .await;
    assert!(matches!(result, Err(AcpError::CoreAgentRequired)));
}

#[tokio::test]
async fn codex_cannot_be_uninstalled_through_core() {
    let result = acp_uninstall_agent_core(AcpUninstallRequest {
        agent_type: AgentType::Codex,
        task_id: "test",
        db: &db,
        emitter: &emitter,
    })
    .await;
    assert!(matches!(result, Err(AcpError::CoreAgentRequired)));
}
```

- [ ] **Step 2: 运行测试并确认当前行为允许操作**

```powershell
cargo test --lib core_agent_policy --features test-utils
```

Expected: FAIL until policy guards exist.

- [ ] **Step 3: 在后端入口强制策略**

- `default_enabled(AgentType::Codex)` 固定返回 true。
- preferences/env/config 更新仍允许，但 `enabled=false` 返回 `CoreAgentRequired`。
- uninstall 和 clear-runtime 操作对 Codex 返回 `CoreAgentRequired`。
- 数据库发现历史 `enabled=false` 时，列表查询和启动修复将其归一化为 true。
- Codex 缺失时连接返回“内核正在准备”或最终错误，不选择其他 Agent。

- [ ] **Step 4: 收敛设置 UI**

Codex 卡片不显示启用开关和卸载动作，显示不可交互的“核心内核”标识；provider、模型和诊断配置继续保留。即使 UI 回归，后端测试仍必须阻止禁用和卸载。

- [ ] **Step 5: 运行前后端定向测试**

```powershell
cargo test --lib core_agent_policy --features test-utils
pnpm test -- src/components/settings/acp-agent-settings.test.tsx
```

Expected: policy tests PASS; Codex controls are absent and other Agent controls remain.

---

### Task 6: 零点击 StartupCodexGate 与统一用户文案

**Files:**
- Create: `src/lib/codex-bootstrap.ts`
- Create: `src/components/account/startup-codex-gate.test.tsx`
- Modify: `src/components/account/startup-codex-gate.tsx`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/lib/types.ts`
- Modify: `src/i18n/messages/en.json`
- Modify: `src/i18n/messages/zh-CN.json`

**Interfaces:**
- Produces: `codexBootstrapGetState`、`codexBootstrapStart`、`codexBootstrapRetry`。
- Subscribes: `app://codex-bootstrap` through `platform.subscribe`。
- Removes: direct use of `acpListAgents`、`acpDetectAgentLocalVersion`、`acpPrepareNpxAgent` and `officecliBootstrap` from StartupCodexGate。

- [ ] **Step 1: 写失败测试，固定零点击和 300ms 防闪烁**

```tsx
it("starts automatically and does not show a dialog before 300ms", async () => {
  vi.useFakeTimers()
  mockStart({ phase: "preparing", attempt: 1, reason: null })
  render(
    <StartupCodexGate>
      <Workspace />
    </StartupCodexGate>
  )
  expect(codexBootstrapStart).toHaveBeenCalledTimes(1)
  expect(screen.queryByText("正在准备内核")).not.toBeInTheDocument()
  await vi.advanceTimersByTimeAsync(300)
  expect(screen.getByText("正在准备内核")).toBeInTheDocument()
})

it("ready fast path never flashes preparation UI", async () => {
  vi.useFakeTimers()
  mockStart({ phase: "ready", attempt: 0, reason: null })
  render(
    <StartupCodexGate>
      <Workspace />
    </StartupCodexGate>
  )
  await vi.advanceTimersByTimeAsync(500)
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
})

it("normal UI never reveals implementation names", async () => {
  mockPreparing()
  render(
    <StartupCodexGate>
      <Workspace />
    </StartupCodexGate>
  )
  await vi.advanceTimersByTimeAsync(300)
  expect(screen.queryByText(/Codex|Node|npm|adapter|版本|下载源/i))
    .not.toBeInTheDocument()
})
```

- [ ] **Step 2: 写失败测试，固定最终失败才出现操作**

```tsx
it("shows retry only after action_required", async () => {
  render(
    <StartupCodexGate>
      <Workspace />
    </StartupCodexGate>
  )
  emitSnapshot({ phase: "preparing", attempt: 3, reason: null })
  expect(screen.queryByRole("button", { name: "重试" }))
    .not.toBeInTheDocument()
  emitSnapshot({
    phase: "action_required",
    attempt: 3,
    reason: "network_unavailable",
  })
  expect(await screen.findByRole("button", { name: "重试" })).toBeEnabled()
})
```

- [ ] **Step 3: 运行测试并确认当前组件失败**

```powershell
pnpm test -- src/components/account/startup-codex-gate.test.tsx
```

Expected: FAIL because current gate performs frontend detection/install and shows immediately.

- [ ] **Step 4: 将 gate 改成快照消费者**

- 组件挂载立即调用 `codexBootstrapStart()`。
- 先订阅事件，再读取快照；Web reconnect 后重新读取快照。
- children 从首次渲染就挂载，未 ready 时外层设置 `inert`；不得因 phase 变化反复卸载整个工作区 provider tree。
- `checking`/`preparing` 超过 300ms 才打开不可关闭的 dialog。
- 只显示 spinner、标题和一句通用描述；删除 `Progress` 假进度。
- `ready` 直接关闭 dialog、解除 inert，不显示 toast。
- `action_required` 显示“重试”和“查看诊断”；诊断复用现有日志查看能力。
- unmount 时清理 timer、事件订阅和 reconnect handler。

- [ ] **Step 5: 固定简体中文文案**

```json
"StartupCodex": {
  "preparingTitle": "正在准备内核",
  "preparingDescription": "首次使用需要一点时间，请稍候。",
  "errorTitle": "内核准备未完成",
  "errorDescription": "自动准备没有完成，请重试或查看诊断。",
  "retry": "重试",
  "diagnostics": "查看诊断",
  "coreBadge": "核心内核"
}
```

当前两种语言的 `preparingTitle` / `preparingDescription` 固定如下：

| Locale | preparingTitle | preparingDescription |
| --- | --- | --- |
| `zh-CN` | 正在准备内核 | 首次使用需要一点时间，请稍候。 |
| `en` | Preparing the core | First use may take a moment. Please wait. |

当前两种语言的错误与操作文案固定如下：

| Locale | errorTitle | errorDescription | retry | diagnostics | coreBadge |
| --- | --- | --- | --- | --- | --- |
| `zh-CN` | 内核准备未完成 | 自动准备没有完成，请重试或查看诊断。 | 重试 | 查看诊断 | 核心内核 |
| `en` | Core preparation was not completed | Automatic preparation did not finish. Retry or view diagnostics. | Retry | View diagnostics | Core |

`reason` 对应描述也只使用产品语言：

| Reason | `zh-CN` | `en` |
| --- | --- | --- |
| `network_unavailable` | 网络暂时不可用，请检查连接后重试。 | The network is temporarily unavailable. Check the connection and retry. |
| `disk_full` | 存储空间不足，请释放空间后重试。 | There is not enough storage space. Free some space and retry. |
| `permission_denied` | 系统权限不足，请完成权限确认后重试。 | System permission is required. Complete the permission request and retry. |
| `integrity_mismatch` | 内核文件校验未通过，请重试。 | Core file verification failed. Retry. |
| `runtime_unavailable` | 当前运行环境未准备好，请查看诊断。 | The runtime environment is not ready. View diagnostics. |
| `unsupported_platform` | 当前系统暂不支持此内核。 | This core is not supported on the current system. |
| `internal` | 自动准备没有完成，请重试或查看诊断。 | Automatic preparation did not finish. Retry or view diagnostics. |

删除旧的 checking/installing 分阶段 key。禁止任一语言的正常标题包含 Codex、Node、npm 或 download。

- [ ] **Step 6: 运行 UI、类型与文案扫描**

```powershell
pnpm test -- src/components/account/startup-codex-gate.test.tsx
pnpm eslint src/components/account/startup-codex-gate.tsx src/lib/codex-bootstrap.ts
pnpm exec tsc --noEmit
rg -n '"(checkingTitle|installingTitle|installingDescription)"' src/i18n/messages
```

Expected: tests/lint/typecheck exit 0; final `rg` returns no matches.

---

### Task 7: 本地账户快照，消除已登录启动网络请求

**Files:**
- Create: `src/components/account/account-profile-panel.test.tsx`
- Modify: `src-tauri/src/commands/iyw_account.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/router.rs`
- Modify: `src-tauri/src/web/handlers/iyw_account.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/components/account/account-profile-panel.tsx`
- Modify: `src/components/layout/sidebar-account-settings.tsx`

**Interfaces:**
- Changes: `StoredSession` 增加 `profile: Option<IywAccountProfile>`。
- Changes: `iyw_account_get_profile_core` 只读 SQLite，不联网。
- Produces: `iyw_account_refresh_profile_core` 供用户显式刷新或账户页使用。

- [ ] **Step 1: 写失败测试，证明启动 profile 查询不得联网**

```rust
#[tokio::test]
async fn cached_profile_is_returned_without_http_request() {
    let fixture = AccountFixture::logged_in_with_cached_profile();
    let profile = iyw_account_get_profile_core(&fixture.db).await.unwrap();
    assert!(profile.logged_in);
    assert_eq!(fixture.http_request_count(), 0);
}

#[tokio::test]
async fn legacy_token_without_profile_stays_logged_in_locally() {
    let fixture = AccountFixture::legacy_token_only();
    let profile = iyw_account_get_profile_core(&fixture.db).await.unwrap();
    assert!(profile.logged_in);
    assert_eq!(fixture.http_request_count(), 0);
}
```

- [ ] **Step 2: 运行测试并确认当前实现访问远程 profile**

```powershell
cargo test --lib iyw_account::tests --features test-utils
```

Expected: FAIL because `iyw_account_get_profile_core` calls `fetch_profile_with_token`.

- [ ] **Step 3: 持久化登录结果并本地读取**

- 密码登录和微信登录先获取 profile，再将 token 与 profile 一次写入 `StoredSession`。
- `iyw_account_get_profile_core` 有缓存时直接返回。
- 遗留 token 没有 profile 时返回 `logged_in=true` 的最小 profile，不在启动路径补网络。
- 启动阶段信任本地 session；后续业务请求若收到明确的 401/认证失效，再清除本地 session、凭证并进入登录界面，不为“预先确认”发请求。
- 登出清空 token 与 profile，并继续清理现有 Agent 凭证。
- 序列化保持 `#[serde(default)]`，旧数据无需数据库 migration。

- [ ] **Step 4: 新增显式远程刷新入口**

```rust
pub async fn iyw_account_refresh_profile_core(
    conn: &DatabaseConnection,
) -> Result<IywAccountProfile, AppCommandError>;
```

它读取本地 token、访问远程、更新缓存。只有账户页显式刷新、登录完成或业务 API 返回认证失败时调用；workspace 启动 effect 不调用。

- [ ] **Step 5: 阻止侧边栏启动时加载远程头像**

侧边栏头像接口固定为：

```tsx
export function AccountAvatar({
  profile,
  className,
  allowRemoteImage = false,
}: {
  profile: IywAccountProfile | null
  className?: string
  allowRemoteImage?: boolean
})
```

- 侧边栏保持 `allowRemoteImage={false}`，只显示本地首字母 fallback。
- 账户 dialog 已由用户主动打开时，`AccountProfilePanel` 传 `allowRemoteImage`。
- 登录态默认头像 URL 同样不得在侧边栏自动加载。

对应测试：

```tsx
it("does not load a remote avatar in the startup sidebar", () => {
  render(<AccountAvatar profile={loggedInProfile} />)
  expect(screen.queryByRole("img")).not.toBeInTheDocument()
})
```

- [ ] **Step 6: 运行账户与启动回归**

```powershell
cargo test --lib iyw_account::tests --features test-utils
pnpm test -- src/components/account
```

Expected: all tests PASS;已登录启动 profile 查询的 mock HTTP count 为 0。

---

### Task 8: 将非核心工具与分发维护移出启动关键路径

**Files:**
- Create: `src/components/settings/internet-tools-settings.test.tsx`
- Create: `src-tauri/src/startup_maintenance.rs`
- Modify: `src/components/account/startup-codex-gate.tsx`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/bin_targets/iyw_claw_server.rs`
- Modify: `src-tauri/src/commands/experts.rs`
- Modify: `src-tauri/src/commands/managed_skills.rs`
- Modify: `src-tauri/src/commands/mcp_sync.rs`
- Modify: `src-tauri/src/commands/internet_tools.rs`
- Modify: `src-tauri/src/commands/internet_tools/bootstrap.rs`
- Modify: `src-tauri/src/commands/office_tools.rs`
- Modify: `src/components/settings/internet-tools-settings.tsx`
- Modify: `src/components/files/office-preview.tsx`

**Interfaces:**
- Removes all startup calls to `officecli_bootstrap_core` and `internet_tools::bootstrap_core`。
- Replaces unconditional expert/skill/MCP distribution work with `PostReadyMaintenanceCoordinator` and an app-version marker。
- Adds `pub post_ready_maintenance: PostReadyMaintenanceCoordinator` to `AppState` and all constructors。
- Retains explicit first-use bootstrap behind Office preview and Internet Tools actions。
- Consumes the process-wide `RuntimeWorkLimiter` for OfficeCLI/Internet Tools download and extraction。

- [ ] **Step 1: 写静态失败测试，锁定启动模块不得引用非核心 bootstrap**

```rust
#[test]
fn startup_source_has_no_optional_tool_bootstrap() {
    let desktop = include_str!("lib.rs");
    let server = include_str!("bin_targets/iyw_claw_server.rs");
    assert!(!desktop.contains("internet_tools::bootstrap_core"));
    assert!(!server.contains("internet_tools::bootstrap_core"));
    assert!(!desktop.contains("ensure_central_experts_installed().await"));
    assert!(!server.contains("ensure_central_experts_installed().await"));
}
```

前端测试同时增加：

```tsx
expect(officecliBootstrap).not.toHaveBeenCalled()
```

- [ ] **Step 2: 运行测试并确认当前启动链失败**

```powershell
cargo test --lib startup_source_has_no_optional_tool_bootstrap --features test-utils
pnpm test -- src/components/account/startup-codex-gate.test.tsx
```

Expected: FAIL because desktop/server start Internet Tools and gate starts OfficeCLI.

- [ ] **Step 3: 删除启动期调用，保留首次使用自动准备**

- 从 StartupCodexGate 删除全部 OfficeCLI refs。
- 从桌面和服务器 bundled experts 后台任务删除 Internet Tools bootstrap 分支。
- Office preview 首次真正打开 Office 文件时调用既有 `officecliBootstrap`，仍使用私有目录和 single-flight。
- 用户第一次启用或使用联网能力时调用既有 `bootstrap_core`；仅打开设置页不触发下载，也不要求用户去外部下载安装。
- 已有完成 marker 时首次使用检查只读 marker，不联网。
- OfficeCLI 与 Internet Tools 在确认 marker 缺失后获取共享 heavy-work permit，获取后再次检查 marker，再执行下载/解压；等待 permit 期间 UI 仍只显示通用准备状态。

- [ ] **Step 4: 将本地分发工作改成版本化就绪后维护**

```rust
#[derive(Clone)]
pub struct PostReadyMaintenanceCoordinator {
    inner: Arc<MaintenanceInner>,
}

impl PostReadyMaintenanceCoordinator {
    pub async fn schedule_after_core_ready(&self);
    pub async fn ensure_before_first_agent_connect(&self);
}
```

- marker 固定为 `runtime/bootstrap/maintenance-<CARGO_PKG_VERSION>.json`。
- marker 匹配时只做一次 `is_file`，不扫描 skills、不写数据库。
- marker 缺失时等待 Codex `Ready`，再延迟 2 秒执行 bundled experts、managed skills 与 managed MCP 的纯本地 reconcile。
- 首个 Agent connect 早于延迟任务时，复用同一个 single-flight 并等待本地维护完成。
- reconcile 全部成功后原子写 marker；失败不写 marker，下次启动或连接重试。
- 应用升级改变版本时自然生成新 marker；旧 marker 保留一个版本后由本地维护清理。
- 该协调器不得调用 OfficeCLI、Internet Tools、npm、uv 或任何 HTTP client。

- [ ] **Step 5: 确保失败隔离**

OfficeCLI、Internet Tools 或 post-ready 本地维护失败不得改变 `CodexBootstrapSnapshot`，不得阻止聊天、文件浏览和 Codex 会话；本地维护失败只记录诊断并在下一触发点重试。

- [ ] **Step 6: 运行定向验证**

```powershell
cargo test --lib startup_source_has_no_optional_tool_bootstrap --features test-utils
cargo test --lib startup_maintenance --features test-utils
pnpm test -- src/components/account/startup-codex-gate.test.tsx
pnpm test -- src/components/settings/internet-tools-settings.test.tsx
```

Expected: all tests PASS.

---

### Task 9: 可选 Agent 首次选择时静默按需准备

**Files:**
- Create: `src-tauri/src/acp/runtime_prepare.rs`
- Create: `src/contexts/acp-connections-context.test.tsx`
- Modify: `src-tauri/src/acp/mod.rs`
- Modify: `src-tauri/src/commands/acp.rs`
- Modify: `src-tauri/src/web/handlers/acp.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/mod.rs`
- Modify: `src-tauri/src/bin_targets/iyw_claw_server.rs`
- Modify: `src/contexts/acp-connections-context.tsx`
- Modify: `src/i18n/messages/en.json`
- Modify: `src/i18n/messages/zh-CN.json`

**Interfaces:**
- Produces: `AgentRuntimePreparationCoordinator`，按 `AgentType` single-flight。
- Produces: `ensure_agent_ready(request: AgentRuntimePreparationRequest<'_>)`。
- Adds `pub runtime_preparation: AgentRuntimePreparationCoordinator` to `AppState` and all constructors。
- Changes: desktop and Web `acp_connect` share `acp_connect_core`。

```rust
#[derive(Clone)]
pub struct AgentRuntimePreparationCoordinator {
    inner: Arc<RuntimePreparationInner>,
}

impl AgentRuntimePreparationCoordinator {
    pub fn new(work_limiter: RuntimeWorkLimiter) -> Self;

    pub async fn ensure_agent_ready(
        &self,
        request: AgentRuntimePreparationRequest<'_>,
    ) -> Result<(), AcpError>;
}

pub struct AgentRuntimePreparationRequest<'a> {
    pub agent_type: AgentType,
    pub task_id: String,
    pub db: &'a AppDatabase,
    pub emitter: &'a EventEmitter,
    pub managed_node: ManagedNodeRuntime,
    pub output_sink: &'a dyn NpmOutputSink,
}
```

- [ ] **Step 1: 写失败测试，固定首次连接自动准备**

```rust
#[tokio::test]
async fn first_optional_agent_connect_prepares_then_spawns() {
    let fixture = ConnectFixture::missing_npx_agent(AgentType::Gemini);
    let id = fixture.connect(AgentType::Gemini).await.unwrap();
    assert!(!id.is_empty());
    assert_eq!(fixture.prepare_count(AgentType::Gemini), 1);
    assert_eq!(fixture.spawn_count(AgentType::Gemini), 1);
}

#[tokio::test]
async fn concurrent_optional_connects_share_preparation() {
    let fixture = ConnectFixture::missing_npx_agent(AgentType::Gemini);
    let calls = (0..5).map(|_| fixture.connect(AgentType::Gemini));
    let results = futures::future::join_all(calls).await;
    assert!(results.iter().all(Result::is_ok));
    assert_eq!(fixture.prepare_count(AgentType::Gemini), 1);
}

#[tokio::test]
async fn repairable_codex_spawn_failure_repairs_and_retries_once() {
    let fixture = ConnectFixture::codex_with_missing_nested_module();
    let id = fixture.connect(AgentType::Codex).await.unwrap();
    assert!(!id.is_empty());
    assert_eq!(fixture.prepare_count(AgentType::Codex), 1);
    assert_eq!(fixture.spawn_count(AgentType::Codex), 2);
}

#[tokio::test]
async fn corrupt_desktop_node_is_replaced_before_single_spawn_retry() {
    let fixture = ConnectFixture::codex_with_corrupt_managed_node();
    fixture.connect(AgentType::Codex).await.unwrap();
    assert_eq!(fixture.node_force_reinstall_count(), 1);
    assert_eq!(fixture.spawn_count(AgentType::Codex), 2);
}

#[tokio::test]
async fn corrupt_host_node_is_never_overwritten_or_retried_in_a_loop() {
    let fixture = ConnectFixture::codex_with_corrupt_host_node();
    assert!(fixture.connect(AgentType::Codex).await.is_err());
    assert_eq!(fixture.host_node_write_count(), 0);
    assert_eq!(fixture.spawn_count(AgentType::Codex), 1);
}
```

- [ ] **Step 2: 运行测试并确认当前 guard 返回 SdkNotInstalled**

```powershell
cargo test --lib runtime_prepare --features test-utils
```

Expected: FAIL; current `acp_connect` refuses to install.

- [ ] **Step 3: 实现按 distribution 的准备策略**

```rust
let registry_version = meta.registry_version().map(str::to_owned);
match meta.distribution {
    AgentDistribution::Npx { .. } => {
        prepare_registered_package(&request, registry_version, false).await
    }
    AgentDistribution::Binary { .. } => {
        acp_download_agent_binary_core(
            request.agent_type,
            None,
            request.task_id,
            request.db,
            request.emitter,
        )
        .await
    }
    AgentDistribution::Uvx { .. } => {
        prepare_registered_package(&request, registry_version, true).await
    }
}

async fn prepare_registered_package(
    request: &AgentRuntimePreparationRequest<'_>,
    registry_version: Option<String>,
    needs_uv: bool,
) -> Result<(), AcpError> {
    if needs_uv {
        acp_install_uv_tool_core(
            request.task_id.clone(),
            request.emitter,
        )
        .await?;
    }
    let recorder = NoopActivationRecorder;
    acp_prepare_npx_agent_core(AcpPrepareNpxAgentRequest {
        agent_type: request.agent_type,
        registry_version,
        version_override: None,
        clean_first: false,
        task_id: &request.task_id,
        db: request.db,
        emitter: request.emitter,
        managed_node: &request.managed_node,
        activation_recorder: &recorder,
        output_sink: request.output_sink,
    })
    .await
    .map(|_| ())
}
```

- Codex 不走 optional 分支，始终由 `CodexBootstrapCoordinator` 保证。
- 可选 Npx Agent 准备前，`acp_connect_core` 等待 Codex 当前 generation 进入 `Ready`，再调用 `managed_node()` 取得纯本地 runtime 快照并放入 request；不得另起 Node 下载或版本探测。
- 已就绪 Agent 只走本地 fast path，不访问网络。
- 每个 Agent 独立 single-flight；不同 Agent 可并行。
- 不同 Agent 可以并行完成纯本地 fast path，但真正的下载/解压/install 必须先获取共享 `RuntimeWorkLimiter`，获取后再次检查目标 Agent 是否已就绪；不得在持有 permit 时等待 Codex coordinator 的 terminal state。
- 准备失败后连接返回结构化错误，不切换到另一 Agent。

- [ ] **Step 4: 统一桌面和 Web 连接 core**

```rust
pub(crate) async fn acp_connect_core(
    params: AcpConnectCoreParams,
    state: AcpConnectCoreState<'_>,
) -> Result<String, AcpError>;

pub(crate) struct AcpConnectCoreParams {
    pub agent_type: AgentType,
    pub working_dir: Option<String>,
    pub session_id: Option<String>,
    pub preferred_mode_id: Option<String>,
    pub preferred_config_values: BTreeMap<String, String>,
    pub owner_window_label: String,
}

pub(crate) struct AcpConnectCoreState<'a> {
    pub db: &'a AppDatabase,
    pub manager: &'a ConnectionManager,
    pub data_dir: &'a Path,
    pub emitter: &'a EventEmitter,
    pub codex_bootstrap: &'a CodexBootstrapCoordinator,
    pub runtime_preparation: &'a AgentRuntimePreparationCoordinator,
    pub maintenance: &'a PostReadyMaintenanceCoordinator,
}
```

Tauri command 和 Axum handler 只负责组装参数，不能各自维护安装逻辑。

Codex spawn 失败时，`acp_connect_core` 只将 command missing、module not found、Node executable 无法启动三类错误映射为 `CodexLaunchFailure::{CommandMissing, ModuleMissing, NodeRuntimeCorrupt}`。它调用 `invalidate_and_repair`、等待该 generation 终态、重新构建 runtime env，并只再 spawn 一次；第二次失败直接返回结构化错误，禁止循环和其他 Agent fallback。

- [ ] **Step 5: 前端显示通用连接准备状态**

首次选择可选 Agent 后，连接区域显示“正在准备智能体”，不得显示包名、命令、下载 URL 或版本。连接成功后直接进入会话，不弹“安装成功”。

- [ ] **Step 6: 运行连接回归**

```powershell
cargo test --lib runtime_prepare --features test-utils
cargo test --features test-utils acp_connect
pnpm test -- src/contexts/acp-connections-context.test.tsx
```

Expected: first connect prepares once, concurrent clients deduplicate, ready connects do not install.

---

### Task 10: 移除 uv/uvx sidecar 并启用保守 release 优化

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/scripts/prepare-sidecars.mjs`
- Modify: `src-tauri/src/acp/binary_cache.rs`
- Modify: `src-tauri/src/commands/internet_tools.rs`
- Modify: `.github/workflows/release-tauri.yml`
- Create: `scripts/verify-desktop-bundle.ps1`

**Interfaces:**
- Desktop `externalBin` retains only `binaries/iyw-claw-mcp`。
- `binary_cache::ensure_uv_tool` remains the only uv acquisition path。
- Release verification checks only the mandatory MCP sidecar。

当前 Windows x64 生成物中，`uv.exe` 为 56,358,912 bytes（53.75MiB），`uvx.exe` 为 333,312 bytes（0.32MiB）。NSIS 压缩后收益不会与原始大小等额，因此验收采用至少 20MiB 的保守安装包下降目标。

- [ ] **Step 1: 先生成当前 Windows x64 baseline**

Run before Task 10 code changes:

```powershell
pnpm tauri:build:low-cpu-local
$baselineDir = Join-Path $env:TEMP "iyw-claw-bundle-baseline"
New-Item -ItemType Directory -Force $baselineDir | Out-Null
$installer = Get-ChildItem src-tauri/target/release/bundle/nsis/*-setup.exe |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1
Copy-Item $installer.FullName (Join-Path $baselineDir "before-setup.exe")
Get-Item (Join-Path $baselineDir "before-setup.exe") |
  Select-Object FullName, Length
```

Expected: baseline installer exists outside the repository and has non-zero length.

- [ ] **Step 2: 写失败的静态 bundle verifier**

`scripts/verify-desktop-bundle.ps1` 使用普通 PowerShell `throw`，不引入 Pester。它必须断言：

```powershell
$config.bundle.externalBin.Count -eq 1
$config.bundle.externalBin[0] -eq "binaries/iyw-claw-mcp"
$config.bundle.windows.webviewInstallMode.type -eq "embedBootstrapper"
$buildRs -notmatch '"uv", "uvx"'
$sidecarScript -notmatch "stageUvSidecars|--uv-only|UV_VERSION"
$releaseWorkflow -notmatch "for name in iyw-claw-mcp uv uvx"
```

- [ ] **Step 3: 运行 verifier 并确认当前配置失败**

```powershell
pwsh -File scripts/verify-desktop-bundle.ps1
```

Expected: non-zero exit because uv/uvx are still bundled.

- [ ] **Step 4: 从 Tauri 和构建脚本删除 bundled uv**

`tauri.conf.json`：

```json
"externalBin": ["binaries/iyw-claw-mcp"]
```

同时完成：

- `build.rs` placeholder 循环只处理 `iyw-claw-mcp`。
- `prepare-sidecars.mjs` 删除 uv manifest、下载、解压、`--uv-only` 和相关未使用 imports。
- release workflow 只验证 `iyw-claw-mcp`。
- 删除 `binary_cache::bundled_uv_tool_paths` 与 `seed_bundled_uv_tools`。
- Internet Tools 安装直接调用 `ensure_uv_tool`；首次使用时静默下载。
- 不删除工作区中现有二进制文件；它们是生成产物，由打包配置决定不再收入安装包。

- [ ] **Step 5: 添加 release profile**

```toml
[profile.release]
codegen-units = 1
lto = "thin"
opt-level = 3
strip = "symbols"
```

不启用 `panic = "abort"`，避免未经验证改变崩溃和 unwind 行为。

- [ ] **Step 6: 保留可靠性组件**

verifier 还必须确认：

- `webviewInstallMode.type == "embedBootstrapper"`。
- installer hook 仍嵌入托管 Node 和 MinGit。
- `iyw-claw-mcp` sidecar 非空。
- Codex registry 仍固定为当前受测版本。

- [ ] **Step 7: 构建 after 包并比较**

当同时传入 `-Before/-After` 时，verifier 除比较字节数，还要把 `-After` 以 NSIS silent 参数安装到唯一的 `%TEMP%\iyw-claw-bundle-audit-<uuid>`。脚本必须等待安装进程退出并检查 exit code，然后只在该绝对临时根内递归验证：Node、MinGit、`iyw-claw-mcp` 存在且非空，`uv.exe`/`uvx.exe` 不存在。验证后优先调用该临时安装自产生的 uninstaller `/S`；最终清理前先用 `GetFullPath` 确认目标父目录严格等于 `%TEMP%` 且名称带固定前缀，禁止删除用户真实安装目录。

```powershell
pnpm tauri:build:low-cpu-local
$baselineDir = Join-Path $env:TEMP "iyw-claw-bundle-baseline"
$installer = Get-ChildItem src-tauri/target/release/bundle/nsis/*-setup.exe |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1
Copy-Item $installer.FullName (Join-Path $baselineDir "after-setup.exe")
pwsh -File scripts/verify-desktop-bundle.ps1 -Before (Join-Path $baselineDir "before-setup.exe") -After (Join-Path $baselineDir "after-setup.exe")
```

Expected:

- after installer does not install `uv.exe` or `uvx.exe`。
- after installer remains installable and includes WebView2 bootstrapper、Node、Git、`iyw-claw-mcp`。
- Windows x64 installer is at least 20MiB smaller than baseline。

- [ ] **Step 8: 运行构建链验证**

```powershell
pnpm tauri:prepare-sidecars
cargo check --manifest-path src-tauri/Cargo.toml
git diff --check -- src-tauri/tauri.conf.json src-tauri/build.rs src-tauri/Cargo.toml src-tauri/scripts/prepare-sidecars.mjs .github/workflows/release-tauri.yml
```

Expected: all commands exit 0; staged sidecars list contains only mandatory MCP binary.

---

### Task 11: 性能、网络、恢复与发布验收

**Files:**
- Create: `src-tauri/src/commands/startup_metrics.rs`
- Create: `src/lib/startup-metrics.ts`
- Modify: `src-tauri/src/codex_bootstrap/tests.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/commands/iyw_account.rs`
- Modify: `src-tauri/src/desktop_startup/metrics.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/components/account/startup-codex-gate.test.tsx`
- Modify: `src/components/account/startup-codex-gate.tsx`
- Modify: `scripts/verify-desktop-bundle.ps1`
- Create: `scripts/verify-codex-bootstrap.ps1`

**Interfaces:**
- Produces a repeatable ready/missing/offline/corrupt/disk-full test matrix。
- Consumes all prior tasks。

- [ ] **Step 1: 添加 100 次 fast-path 基准测试**

```rust
#[test]
fn ready_fingerprint_p95_is_below_200ms() {
    let fixture = ReadyFixture::matching_codex();
    let mut samples = (0..100)
        .map(|_| {
            let started = Instant::now();
            assert!(matches!(
                check_codex_ready(fixture.ready_request(NodeRuntimeKind::Managed)),
                ReadyCheck::Ready(_)
            ));
            started.elapsed()
        })
        .collect::<Vec<_>>();
    samples.sort_unstable();
    assert!(samples[94] <= Duration::from_millis(200));
}
```

- [ ] **Step 2: 添加 ready 启动零网络、零安装断言**

测试 fixture 的 network client 和 runtime ops 均使用计数器：

```rust
assert_eq!(fixture.network_count(), 0);
assert_eq!(fixture.node_install_count(), 0);
assert_eq!(fixture.codex_install_count(), 0);
assert_eq!(fixture.version_process_count(), 0);
```

账户启动测试同时断言 profile HTTP count 为 0。

同一组断言分别运行 `DesktopManaged` 与 `HostOnly` ready fixture；Host fixture 提供固定 PATH 快照并且不创建 `runtime/node/current.json`，两者的 `version_process_count()` 都必须为 0。

- [ ] **Step 3: 添加前端延迟与状态收敛测试**

`DesktopStartupRecorder` 的生产实现从 `desktop::run()` 第一条语句创建的同一个 `Instant` 计算阶段时间，并固定输出：

```text
startup_stage stage=database_ready elapsed_ms=<u128>
startup_stage stage=shell_created elapsed_ms=<u128>
startup_stage stage=codex_started elapsed_ms=<u128>
startup_stage stage=maintenance_started elapsed_ms=<u128>
```

阶段日志不写路径、账号、版本、URL 或错误详情。`shell_created` 必须在 `WebviewWindowBuilder::build` 与 `post_window_setup` 成功后立即记录；PowerShell 的 `MainWindowHandle` 仍作为真正可见时间，二者同时保留，便于区分 Rust 初始化与 WebView 显示耗时。

桌面指标接口固定为一个 object 参数：

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupMetricReport {
    pub first_contentful_paint_ms: f64,
    pub workspace_interactive_ms: f64,
    pub max_long_task_ms: f64,
    pub external_resource_count: u32,
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub fn startup_report_metrics(report: StartupMetricReport) {
    tracing::info!(
        target: "startup_metrics",
        first_contentful_paint_ms = report.first_contentful_paint_ms,
        workspace_interactive_ms = report.workspace_interactive_ms,
        max_long_task_ms = report.max_long_task_ms,
        external_resource_count = report.external_resource_count,
        "frontend_ready"
    );
}
```

`src/lib/startup-metrics.ts` 在 Tauri 桌面环境采集一次并调用该命令；Web/remote 环境直接 no-op。不得发送 HTTP，不得写 localStorage，不得包含用户数据。

- 299ms 不显示 dialog。
- 300ms 且仍 preparing 时显示。
- ready 后 provider tree 不被卸载重建。
- 旧 generation 事件不能覆盖新 generation。
- reconnect 后以 `get_state` 快照收敛。
- action_required 才出现按钮。
- 使用 Paint Timing 读取 `first-contentful-paint`。
- 从 `performance.timeOrigin` 到 gate 进入 ready 记录 `workspace_interactive_ms`。
- 用 `PerformanceObserver({ type: "longtask", buffered: true })` 记录最大任务；不支持该 entry type 时记录 `0` 并在脚本报告中标注 unsupported，Windows WebView2 发布验收不允许 unsupported。
- 从 buffered Resource Timing 只统计 scheme 为 `http/https` 且 origin 不是当前应用 origin、localhost 或 loopback 的条目数量，记为 `external_resource_count`；只上报计数，不上报 URL。
- 前端测试 mock Performance API，断言只上报一次且正常准备 UI 不包含指标文本。

- [ ] **Step 4: 编写 Windows 端到端验证脚本**

`scripts/verify-codex-bootstrap.ps1` 使用独立临时数据根，逐项执行：

ready 零网络用例启动脚本自有的 loopback TCP 计数 listener，并只为被测子进程设置大小写两套 `HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY`；从进程启动观察到 `frontend_ready` 后 1 秒，断言 listener 接受连接数为 0 且前端 `external_resource_count == 0`。脚本退出前恢复自身环境；不得修改系统代理或防火墙。

只在 `test-gateway` feature 中提供本地 fixture 种子：

```rust
#[cfg(feature = "test-gateway")]
pub async fn seed_benchmark_session_from_env(
    conn: &DatabaseConnection,
) -> Result<(), AppCommandError> {
    if std::env::var_os("IYW_CLAW_TEST_SEED_ACCOUNT").as_deref()
        != Some(std::ffi::OsStr::new("1"))
    {
        return Ok(());
    }
    save_session(
        conn,
        &StoredSession {
            token: Some(IywAccountToken {
                access_token: "test-gateway-unusable-token".into(),
                refresh_token: String::new(),
                expiration: String::new(),
            }),
            profile: Some(IywAccountProfile {
                logged_in: true,
                name: Some("Startup Benchmark".into()),
                ..Default::default()
            }),
        },
    )
    .await
}
```

生产 feature 不编译该函数；测试 token 不访问远程服务、不打印日志、不用于发布构建。脚本先执行：

```powershell
pnpm build
pnpm tauri:prepare-sidecars
pnpm exec tauri build --features test-gateway --no-bundle
cargo build --manifest-path src-tauri/Cargo.toml --release --no-default-features --features server-runtime,test-gateway --bin iyw-claw-server
```

然后逐项执行：

1. 创建临时数据根，以 `IYW_CLAW_TEST_SEED_ACCOUNT=1` 启动第一次进程；从 `Start-Process -PassThru` 到 `MainWindowHandle` 非零记录 cold `shell_visible_ms` 并断言不超过 300ms，同时等待 `codex-ready.v1.json` 并确认无需点击；进程退出后立即移除该环境变量。
2. 在同一数据根第二次启动，确认日志出现 `ready_fast_path`，无 install attempt。
3. 删除临时数据根内的 Codex command，重启并确认自动恢复。
4. 写入 interrupted journal 和 staging，重启并确认只清理 journal 指向目录。
5. 使用另一个临时数据根启动 loopback `iyw-claw-server`，固定随机空闲端口与测试 token；将 `IYW_CLAW_NPM_REGISTRY` 指向未监听的本地端口，POST `/api/codex_bootstrap_start` 并轮询 `/api/codex_bootstrap_get_state`，确认最多 3 次尝试后 action_required。
6. 恢复 registry，POST `/api/codex_bootstrap_retry`，确认同一服务器进入 ready。
7. 清空该服务器测试根的 Codex runtime 后，并行发送 20 个 POST `/api/codex_bootstrap_start`，确认一个 generation 只有一个 prepare。
8. 使用已就绪 fixture 顺序启动 30 次；每次用 `Stopwatch` 从 `Start-Process -PassThru` 计时到该进程 `MainWindowHandle` 非零，再继续计时到同一进程日志中的 `startup_metrics frontend_ready`，分别记录 shell 与 process-to-workspace 时间；解析同一 PID 日志中的 `database_ready`、`shell_created`、`codex_started`，若顺序错误或任一阶段缺失立即失败。
9. 每次采样完成后只关闭该次 `Start-Process` 返回的 PID；排序后用索引 28（第 29 个样本）作为 30 次样本的 P95。
10. 输出 `database_ready_ms`、`shell_created_ms`、`shell_visible_ms`、`codex_started_ms`、`process_to_workspace_ms`、`first_contentful_paint_ms`、`workspace_interactive_ms`、`max_long_task_ms` 的 min/median/P95/max，以及 `external_resource_count`、proxy accepted connections 的总数，并对阈值做非零退出断言。

服务器请求统一使用：

```powershell
$headers = @{ Authorization = "Bearer $token" }
$body = '{}'
Invoke-RestMethod -Method Post -Headers $headers -ContentType 'application/json' -Body $body -Uri "$baseUrl/api/codex_bootstrap_start"
```

脚本不得修改用户真实数据根，不修改防火墙，不终止非脚本启动的进程。任何递归清理前必须用 `GetFullPath` 证明目标位于 `%TEMP%` 且目录名带 `iyw-claw-bootstrap-test-` 固定前缀；验证失败时宁可保留临时产物并退出，也不得扩大删除范围。

- [ ] **Step 5: 手工验收最终用户文案**

正常路径只允许看到：

```text
正在准备内核
首次使用需要一点时间，请稍候。
```

最终失败路径只允许看到产品级错误、重试和诊断入口。原始 npm 日志、URL、包名和版本只能在诊断视图中出现。

- [ ] **Step 6: 运行完整前端质量门禁**

```powershell
pnpm eslint .
pnpm test
pnpm build
```

Expected: all commands exit 0.

- [ ] **Step 7: 运行完整桌面 Rust 质量门禁**

Run from `src-tauri/`:

```powershell
cargo check
cargo test --features test-utils
cargo clippy --all-targets --features test-utils -- -D warnings
```

Expected: all commands exit 0.

- [ ] **Step 8: 运行服务器和 MCP 质量门禁**

Run from `src-tauri/`:

```powershell
cargo check --no-default-features --features server-runtime --bin iyw-claw-server
cargo test --no-default-features --features server-runtime --bin iyw-claw-server --lib
cargo clippy --no-default-features --features server-runtime --bin iyw-claw-server --lib -- -D warnings
cargo check --no-default-features --features mcp-runtime --bin iyw-claw-mcp
cargo clippy --no-default-features --features mcp-runtime --bin iyw-claw-mcp -- -D warnings
```

Expected: all commands exit 0.

- [ ] **Step 9: 运行安装包与启动验收**

```powershell
$baselineDir = Join-Path $env:TEMP "iyw-claw-bundle-baseline"
pwsh -File scripts/verify-desktop-bundle.ps1 -Before (Join-Path $baselineDir "before-setup.exe") -After (Join-Path $baselineDir "after-setup.exe")
pwsh -File scripts/verify-codex-bootstrap.ps1
```

Expected:

- 外壳显示不超过 300ms。
- ready fingerprint P95 不超过 200ms。
- ready 工作区 P95 不超过 500ms。
- ready 启动网络请求 0、安装任务 0。
- 多窗口一个 generation 只执行一次 prepare。
- 断网最多 3 次尝试。
- 半安装不产生 ready stamp。
- 新安装正常流程重启次数 0。
- 安装包至少缩小 20MiB。

- [ ] **Step 10: 最终 diff 和用户改动保护检查**

```powershell
git status --short
git diff --check
git diff --stat
```

Expected: 只包含本计划授权范围内的改动；实施前已存在的用户改动被完整保留。Do not commit.

---

## Rollout Order

1. 先执行 Tasks 1-4，建立后端状态源和不阻塞启动的准备能力。
2. 再执行 Tasks 5-7，强制核心策略并替换前端门禁、消除账户启动网络。
3. 再执行 Tasks 8-9，移出非核心任务并实现可选 Agent 按需准备。
4. 最后执行 Task 10 的打包配置和 CI 改动。
5. Task 11 全部通过后才允许发布。

Task 10 涉及根级打包配置和 CI；用户批准本任务文档后才获得实施授权。

## Rollback Boundaries

- 若 Task 4 接入失败，可回退 transport wiring，但保留纯模型和 fingerprint 代码。
- 若新 gate 出现回归，可临时恢复旧 gate 的阻塞外观，但不得恢复前端自行安装或 OfficeCLI 启动任务。
- 若按需可选 Agent 准备不稳定，可关闭 optional connect 自动准备；Codex 强制内核不受影响。
- 若 release profile 引发平台构建异常，可单独回退 profile；不得重新捆绑 uv/uvx 作为掩盖。
- 任一回滚都不得允许禁用、卸载或替代 Codex。

## Definition of Done

- Codex 在后端策略上不可禁用、不可卸载、不可被其他 Agent 自动替代。
- 干净安装无需点击初始化、无需运行命令、无需手动下载。
- 正常准备界面不泄露技术组件名，只显示统一产品文案。
- 已就绪启动不访问网络、不执行安装、不运行完整 preflight。
- 多窗口和多 Web 客户端不会重复准备。
- Node/Codex 损坏可自动恢复，失败不会产生半安装 ready 状态。
- OfficeCLI、Internet Tools、uv/uvx 不再占用启动关键路径。
- 可选 Agent 首次连接时在应用内静默准备。
- Windows 安装包保留可靠性组件并达到体积目标。
- 桌面、服务器、MCP、前端测试、lint、build 和恢复脚本全部通过。
