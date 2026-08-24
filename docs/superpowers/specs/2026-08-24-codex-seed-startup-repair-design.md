# Codex 内置种子与 ACP 启动闭环修复设计

## 状态

已完成日志、源码和安装包种子调用链审计，等待用户确认规格后实施。

## 问题结论

`v0.1.111` 的 Codex 安装与启动存在两个独立的 Windows 错误：

1. `AgentDistribution::Npx.package` 保存的是带版本 npm spec，例如
   `@agentclientprotocol/codex-acp@1.4.0`。Windows 直接 Node 启动逻辑却把它
   当纯包名拼入 `node_modules`，查找了不存在的
   `@agentclientprotocol/codex-acp@1.4.0/package.json`。解析失败后又静默回退
   `.cmd` shim，因此 ACP Host 仍在 `initialize` 前以 exit code 1 退出。
2. 删除 Codex runtime 后，安装包 seed 会被读取和解压，但 seed probe 通过
   `cmd.exe /D /S /C` 传递了错误的转义引号。`cmd.exe` 把包含 `\"` 的完整文本
   当作命令名，约 50 ms 返回 exit code 1。导入逻辑随后删除刚激活的 runtime，
   把静态 `InvalidInput` 记成空 detail，并回退 Version Center 下载。

新会话和恢复会话共享同一个 ACP Host initialize，所以它们表现为同一种失败；
`connection not found` 是 Host 失败并从连接表清理后的次生前端日志。

## 目标

- Codex 继续是唯一内置的 Agent runtime；不把其他 Agent 加入安装包。
- Windows Codex ACP 启动统一使用受管 Node 执行真实 JavaScript entrypoint。
- 删除或损坏本地 Codex runtime 后，优先从当前安装包 seed 离线恢复。
- seed 只有在 staging 校验和运行探测通过后才能替换 active runtime。
- seed 不可用或验证失败时继续使用现有 Version Center 下载回退。
- 新会话和历史会话恢复均能完成 Host initialize，再进入各自的
  `session/new` 或 `session/load` 路径。
- 日志能区分 package spec、entrypoint、seed stage、probe、activate 和在线回退，
  且不记录 token、环境变量值、完整用户路径或协议 payload。

## 非目标

- 不改变 ACP 协议、会话数据库 schema、模型或供应商 contract。
- 不修改其他 Npx、Binary 或 Uvx Agent 的安装来源和打包范围。
- 不取消 Version Center，也不允许 seed 绕过版本、平台、摘要或库存校验。
- 不在通用 `sacp-tokio` 层引入 Windows shell 兼容逻辑。
- 不把运行时写回安装目录；安装包资源始终只读，运行时仍落在受管 data 目录。

## 方案选择

### 方案 A：统一直接 Node entrypoint，并在 staging 上 probe（采用）

在 npm runtime 模块统一解析带版本 spec，严格读取 package manifest 中与命令同名的
`bin` entry。Windows 启动和 seed probe 都使用 `node.exe <entrypoint>`。seed 先在
staging 完成身份、平台依赖和 `--version` 探测，再原子激活和写库存。

该方案消除 `.cmd` 的两条故障路径，并保留现有库存与在线回退模型。

### 方案 B：仅修正两处 `cmd.exe` 引号

改动较小，但运行路径仍依赖 npm shim、Windows shell 引号和 PATH。启动与 seed probe
仍可能再次分叉，不采用。

### 方案 C：在 `sacp-tokio` 中通用支持 `.cmd`

会改变所有 Agent 的进程启动语义，并扩大协议 transport 的风险面；当前问题能在
Codex/npm runtime 边界内解决，不采用。

## 端到端流程

### 1. 发布构建

CI 继续按 target 生成 `node`、`git`、`uv`、`codex-acp` 四个 seed 组件，并执行：

1. 锁定版本和目标平台 npm optional dependency。
2. 生成文件清单、组件摘要和压缩归档摘要。
3. 校验 manifest、entrypoint 映射和归档内容。
4. 将当前 target 的只读 `runtime-seed` 作为 Tauri resource 打入安装包。
5. NSIS smoke 验证安装后的 seed manifest 和四个归档仍存在且摘要一致。

增加 Windows x64 seed 运行探测：解压 Codex seed 后，使用目标 Node 执行真实 JS
entrypoint 的 `--version`。该门禁不通过则不得上传桌面资产。

### 2. 首次安装与删除后恢复

启动门禁调用 runtime bootstrap 时按以下顺序处理 Codex：

1. 检查当前 active 版本、库存记录、package identity、平台 optional dependency、
   JavaScript entrypoint 和 Node probe。
2. active 健康且版本不低于 seed 时零解压、零覆盖。
3. active 缺失或损坏时，把 seed 解压到唯一 staging 目录。
4. 在 staging 上验证 package name/version、平台依赖和 `node entrypoint --version`。
5. 全部通过后才把旧目录移到 trash、原子 rename staging、写 ready inventory 并激活。
6. 激活或库存写入失败时恢复旧目录；不存在旧目录时保持未安装状态。
7. seed 缺失、版本不匹配或校验失败时记录明确阶段，再进入 Version Center 回退。

不得出现“先替换 active，再 probe，失败后直接删除 active”的状态。

### 3. Agent 启动

Windows Npx Agent 构建启动命令时：

1. 从带版本 npm spec 提取纯 package name。
2. 读取 `<prefix>/node_modules/<package>/package.json`。
3. 只接受 `bin[command]`；不使用对象中的任意第一个 entry。
4. 拒绝绝对路径、`..`、缺失文件和非文件 entrypoint。
5. 使用受管 Node；仅保留现有开发/遗留环境的 PATH Node 兼容回退。
6. 安装态若 entrypoint 解析失败，返回明确启动错误，不静默回退 `.cmd`。

非 Windows 路径保持现有可执行 shim 行为，不改变其他平台的进程模型。

### 4. 新会话与恢复会话

Host initialize 成功后继续沿用现有分支：

- 新会话发送 `session/new`。
- 历史会话发送 `session/load`/resume，并保留原 external session id。
- Host initialize 失败只清理本次连接；durable input 保留等待后续恢复，不重复提交。
- 前端不得把清理后的 `connection not found` 作为根因覆盖真实 Host 错误。

### 5. 预热时序

安装态自动预热不得与 Codex runtime 修复并发：

- runtime 存在且健康时允许正常预热。
- runtime 缺失、正在 seed 导入或正在 Agent 存储写操作时跳过预热；由首次真实连接
  建立 Host，不产生错误重试风暴。
- seed 导入、手动安装、版本切换和卸载继续串行化；连接启动只能读取已激活目录。
- 不为了预热阻塞工作区显示；真实 seed 解压仍由现有启动门禁显示安装进度。

## 代码边界

- `src-tauri/src/acp/npm_runtime.rs`
  - 提供统一、受限的 npm package spec 解析和 Node entrypoint 解析。
- `src-tauri/src/acp/connection.rs`
  - Windows Npx 启动使用直接 Node entrypoint，并取消静默 `.cmd` 降级。
- `src-tauri/src/acp/version_center/installer/runtime_seed/codex.rs`
  - staging probe、激活顺序、旧版本恢复和健康 active 判断。
- `src-tauri/src/acp/version_center/installer/runtime_seed/mod.rs`
  - 输出可定位且脱敏的 seed 失败阶段和消息。
- `src-tauri/src/acp/manager.rs`、`src-tauri/src/lib.rs`
  - 仅在必要范围内约束预热与 Agent 存储写操作的时序。
- `src-tauri/scripts/prepare-runtime-seed.mjs`、验证脚本和 release workflow
  - 增加真实 Node entrypoint probe，保持四组件范围不变。

不修改当前工作区中与 Skill routing 描述长度相关的未提交文件。

## 错误与日志

seed 错误至少记录：组件、seed 版本、阶段、错误 code、静态错误 message、是否进入在线
回退。启动日志至少记录 Agent、启动方式 `direct_node`、package name、entrypoint 是否
解析成功和 Host initialize 结果。所有路径只记录文件名、平台或摘要，不记录完整用户
目录；不记录环境值、凭据或 JSON-RPC 内容。

## 验收标准

### 静态与 CI

- 带版本的 scoped/unscoped npm spec 均解析到正确 package name。
- manifest `bin` 缺失、命令不匹配、绝对路径和父目录穿越全部 fail closed。
- Rust 目标文件 `rustfmt --check`、Node 验证脚本语法、配置解析和 `git diff --check`
  通过。
- Windows x64 server/Tauri CI 编译通过。
- runtime seed 构建、归档校验、安装后资源校验和直接 Node `--version` probe 通过。

### 安装态

在 Windows x64 安装包上执行：

1. 正常升级保留现有健康 Codex，不重复解压或下载。
2. 删除受管 Codex runtime 和对应 active 状态后重启，无网络情况下从 seed 恢复。
3. 恢复日志出现 `Codex ACP imported and activated`，不出现在线 tarball 下载。
4. 新建对话完成 Host initialize 和 `session/new`，能得到 Agent 回复。
5. 打开已有对话完成 Host initialize 和 `session/load`，上下文连续且能继续发送。
6. 篡改 seed 或 entrypoint 时不替换健康旧 runtime，并明确回退 Version Center。
7. Version Center 回退失败时保留最后一个健康版本，不留下 active 指针指向缺失目录。

### 发布核验

- 发布 tag 指向包含本修复的版本提交。
- GitLab 与 GitHub `main` 和 tag 分别核验一致。
- Release 资产包含 Windows x64 NSIS、签名和 `latest.json`。
- 安装后的客户端版本、实际进程路径、seed manifest 和运行日志均对应同一发布版本。

## 风险控制

- package spec 解析只用于定位已安装目录，不改变 Version Center contract。
- staging probe 在激活前执行，避免失败探测破坏旧版本。
- 不将系统 PATH Node 误记为受管库存；安装态优先受管 Node。
- 不自动重放结果未知的 prompt；durable input 继续由现有 outbox 负责。
- 本地不编译或启动 Tauri 桌面端；真实构建和安装态验证由发布 CI 与安装环境完成。
