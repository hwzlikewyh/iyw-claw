# Agent 新对话启动延迟治理设计

## 背景

桌面安装态在新建 Codex 对话时，用户从发起连接到首个 prompt 真正进入
Agent 的等待时间可超过 50 秒。现有日志将这段时间拆成了两个主要阶段：

- ACP 子进程冷启动和 `initialize`，一次异常样本约 44 秒。
- `session/new` 内部的 Skill、模型和账户刷新，一次异常样本约 12 秒。

同一时间还观察到两个桌面实例并发启动、`working_dir=None` 的无效连接、
Skill runtime 环境迁移持续失败，以及安装目录缺少 `iyw-claw-mcp` sidecar。
这些问题会放大冷启动耗时，或者让已经完成的启动无法正常工作。

## 目标

一次性完成以下治理：

1. 阻止 Windows 上两个桌面实例同时越过 single-instance 初始化窗口。
2. 阻止新会话以空 working directory 发起 ACP 连接。
3. 避免每次连接重复执行没有变化的 Skill reconcile，并恢复残留 runtime backup。
4. 确保 generic 和带版本号的 `iyw-claw-mcp` 都真实进入桌面安装包。
5. 让同一兼容 Agent 的多个逻辑 session 复用一个已初始化的 ACP Host。
6. 避免 `codex-acp` 每个新 session 强制刷新 Skills、模型和账户。
7. 输出完整、可关联的分段耗时日志。

## 非目标

- 本轮不迁移 Node、Codex、Skill runtime 或其他运行时到 NVMe。
- 不在安装目录内直接修改受校验的 managed component。
- 不自动发布 Fusion managed component、桌面安装包或 GitHub Release。
- 不改变 ACP 协议、数据库 schema 或前后端公共 API。

## 方案比较

### 方案 A：只做局部优化

修复重复实例、空目录连接、reconcile、sidecar 和日志。改动最小，但每个新会话
仍需重新 spawn 和 initialize，无法消除最大的一段等待。

### 方案 B：一次性预热进程池

后台启动若干未绑定 session 的进程，新对话领取后再补充池。它不要求共享
transport，但会维持多份 Node/Codex 进程，池失效、容量和抢占策略复杂。

### 方案 C：共享 ACP Host

一个 ACP transport 承载多个逻辑 session。应用后台预热默认 Agent，新对话只做
`session/new` 或 `session/load`。该方案直接消除重复 initialize，并能与局部优化组合。

采用方案 C。共享能力按 Agent 显式开启，首批仅开启已验证支持多 session 的
Codex ACP；未开启的 Agent 继续使用原有的一 session 一进程路径。

## 架构

### Runtime Host

新增 `AgentRuntimeHost`，只负责进程级资源：

- Agent 子进程及其生命周期。
- ACP transport、`initialize` 结果和初始化中的共享 future。
- Host key、创建时间、最后活动时间和健康状态。
- 当前挂载的 logical session 集合。

Host key 至少包含 Agent 类型、managed component 版本、可执行入口、进程级环境
指纹和影响进程行为的配置版本。working directory、conversation id、mode 和模型
选择属于 session 级状态，不进入 Host key。

`AgentRuntimeHostRegistry` 保证同一个 key 最多存在一个初始化任务。并发连接要么
复用 ready host，要么等待同一个 initializing host，不能各自 spawn。

### Logical Session

每个对话继续拥有独立的：

- iyw-claw `connection_id` 和 ACP session id。
- `SessionState`、命令 channel、事件序列和最近事件缓冲。
- owner window、emitter、cwd、终端、权限和 MCP 状态。
- prompt、cancel、mode、config、模型和 usage 生命周期。

共享 transport 收到 session update 后，必须按 ACP session id 定向到对应 handler；
不允许通过“当前连接”之类的全局状态路由。断开 logical session 只卸载 handler 和
session 资源，最后一个 session 离开后 Host 进入 idle，而不是立即退出。

### 生命周期与回退

- 应用核心初始化完成后，后台预热默认且启用的 Agent。
- 预热受现有空闲 Agent 配额约束；用户连接优先于后台预热。
- Host 崩溃后，所有挂载 session 收到明确断开事件，registry 移除该 Host。
- 下一次连接创建新 Host，不在未知执行结果下自动重放 prompt。
- Agent 版本、凭据、进程环境或配置改变时，把旧 Host 标为 stale；不再接受新
  session，已有 turn 结束后退出。
- 提供环境变量 kill switch，使共享模式可整体退回原有连接路径。

## 启动竞态

Windows 入口在构建 Tauri App 和注册 single-instance 插件前获取命名 startup
gate mutex。第二个实例等待第一个实例完成隐藏窗口建立后再继续，随后由现有
single-instance 插件完成参数转发并退出。

等待必须有上限并记录 wait duration。句柄通过 RAII 释放；异常退出由 Windows
自动释放 mutex。非 Windows 平台保持原行为。

## Working Directory 门禁

前端现有延迟连接和 scratch directory 创建逻辑继续保留。后端作为最终边界：

- 新 session 的 `working_dir` 为 `None`、空白或不存在时直接返回可识别错误。
- 恢复 session 可使用请求路径；若调用方省略，则只能使用数据库中已持久化且
  验证存在的路径，不能回退到进程 cwd。
- Tauri 和 Web `/acp_connect` 入口调用同一验证函数。
- 拒绝发生在 Skill reconcile、凭据同步和进程 spawn 之前。

## Skill Reconcile

把启动前 reconcile 拆成“读取 revision”和“执行 reconcile”两个阶段。进程内缓存
记录 bundle revision、managed Skill revision、目标 Agent、目标目录指纹和上次成功
结果。key 未变化时跳过磁盘全量 hash 和重复写 manifest。

以下事件主动失效：Skill 安装、卸载、更新、市场覆盖变化、Agent 启停或配置变化，
以及 managed component 切换。失败结果不缓存为成功。

对于 `.bundled-runtime-backup` 和当前 Skill 目录同时含同名 `.venv` 的状态：

- 不删除当前可用 runtime。
- 如果两侧内容指向同一已迁移环境，清理 stale backup。
- 如果无法证明等价，保留两侧并返回包含路径和恢复动作的错误，禁止覆盖。
- 所有 rename、restore 和 cleanup 都记录 Skill id、源、目标和结果。

## codex-acp 初始化

共享 Host 消除重复 `initialize`，但每个 `session/new` 仍会触发上游
`refreshSkills(forceReload=true)`、`model/list` 和 `account/read`。因此维护一个受控的
`codex-acp` managed component 版本：

- Skills 按目录 revision/fingerprint 刷新，revision 未变时复用解析结果。
- 模型列表和账户信息使用进程级并发去重缓存；短 TTL 到期后后台刷新。
- 显式配置变化、认证变化和 managed Skill revision 变化时主动失效。
- 首次请求和刷新失败保持原错误语义，不用陈旧缓存伪造成功。

补丁必须进入可审计源码和 Fusion managed component 发布链，不修改用户安装目录。
本地实现和校验完成后，生产上传、catalog 切换与发布另行执行。

## Sidecar 打包

`prepare-sidecars.mjs` 在 copy 前后验证 Cargo 输出、generic staged file 和带版本号
staged file 均存在且大于零字节。bundle 后增加验证脚本，从实际 NSIS bundle 或
解包结果确认两个 sidecar 名称和非零大小；CI 使用该脚本作为发布门禁。

旧版本的零字节兼容占位不得被当前版本解析或复制。安装态探测日志必须同时输出
解析过的 sidecar 路径、版本和文件大小。

## 可观测性

每次连接生成 `startup_trace_id`，以下阶段记录 start、end、duration、结果、Agent、
Host key 摘要和 connection id：

1. request accepted / working directory validation
2. Skill revision check / reconcile
3. credential sync / runtime env build
4. Host lookup / wait / spawn
5. ACP initialize
6. session new 或 load
7. selector preference apply
8. first prompt dispatch

日志不得包含 token、完整环境变量、prompt 内容或个人路径；路径仅记录脱敏摘要。

## 并发和错误处理

- Registry 锁只保护 map 和状态迁移，不能跨进程 I/O 或 await 长时间持有。
- Host 初始化使用 single-flight；失败向所有等待者返回同一个根因并原子移除条目。
- session attach 失败只清理该 session，不影响 Host 上其他 session。
- Host stdout、协议关闭和 child exit 只能触发一次 shutdown fan-out。
- 取消 prompt 只发给目标 ACP session。
- 对外部已派发但结果未知的 prompt 不做自动重试。

## 验证与验收

遵循仓库规则，默认不新增或运行测试文件；完成前执行格式检查、静态调用链审查和
`git diff --check`，并逐项检查输入、输出、状态变化、错误路径、资源释放和并发影响。

静态验收：

- 同一 Host key 的并发新连接只有一个 spawn/initialize 路径。
- 两个 logical session 的状态、事件、cancel 和 disconnect 相互隔离。
- 新 session 空 working directory 在任何副作用前被拒绝。
- reconcile 的缓存 key、失效事件和失败不缓存逻辑闭环。
- 当前版本两个 sidecar 的 staging 和 bundle verifier 都拒绝空文件。
- 每个启动阶段都有同一个 trace id 和 duration。

安装态验收需要新构建和受控 managed component 后执行，不能由源代码检查替代：

- 同 Agent 连续创建多个对话只存在一个 ACP Host 进程。
- Host 已预热时，点击新对话不再出现 ACP initialize 等待。
- 同毫秒双击启动只保留一个桌面实例，没有 WebView2 创建失败残留。
- 实际安装目录存在并可运行 generic 和带版本号的 `iyw-claw-mcp`。
- 日志可分别量化 cold、in-flight wait、warm reuse 和 `session/new`。

## 发布边界

代码和本地 verifier 完成不等于安装态已经修复。最终发布必须依次完成：受控
`codex-acp` component 构建及签名、Fusion catalog 更新、桌面安装包构建、实际 NSIS
内容检查、安装升级和上述运行态验收。任何远程上传、catalog 切换、commit/push 或
Release 发布都按仓库授权规则单独确认。
