# IYW 可选插件运行时与 MCP Apps 宿主设计

日期：2026-08-26

状态：待用户校对，未获批准前不得修改业务代码

设计分支：`design/plugin-runtime-architecture`

审计基线：`iyw-claw@418640809f2b66d1ac304cc012fd3f093b9192ef`

补充复核：`main@6d1dc04cba7e4eb703db200cf73afd7d4043c950`

## 文档结构

本设计按单文件 300 行上限拆分：

- 本文件：决策、验收标准、范围、当前实现审计与方案选择；
- [目标架构](plugin-runtime-architecture/architecture.md)：Registry、Supervisor、Router、动态能力目录与 App Host；
- [清单与生命周期](plugin-runtime-architecture/manifest-and-lifecycle.md)：v2 manifest、持久化、安装、升级、禁用与卸载；
- [安全、分期与验证](plugin-runtime-architecture/security-rollout-validation.md)：Agent 兼容、安全模型、逐阶段影响和验收矩阵。

## 1. 决策摘要

`iyw-claw` 不把 Cowart 或后续具体业务插件打进原生安装包。原生安装包只提供通用的
插件安装、可信校验、动态能力路由、本地运行时管理和 MCP Apps 安全宿主。Cowart 作为
第一个可选插件，由用户需要时从 Fusion 市场下载，确认权限后安装，第一次真正调用时
才启动本地 MCP；空闲、禁用、升级、卸载或会话结束后按租约回收。

后续插件统一使用同一套组件模型：

- `skill`：给 Agent 的工作流与触发规则；
- `connector`：远程 MCP 或本地 MCP 服务声明；
- `runtime`：受宿主管理的 Node、Python 或受审二进制入口；
- `capability`：可被稳定发现、读取和调用的插件能力；
- `app`：由 MCP Apps 宿主渲染的交互界面；
- `binding`：Skill 与 Connector 的依赖关系，继续兼容现有 v1 契约。

插件默认通过 `HostGateway` 接入 Agent。既有
`search_iyw_capabilities -> read_iyw_capability -> invoke_iyw_capability`
三工具保持稳定，当前会话不因为安装新插件而增加顶层工具。插件能力在网关内部动态
出现，因此支持热注册，不要求重写或重启每个 Agent 的原生 MCP 配置。

`NativeAgent` 只作为少数插件的显式兼容模式。同一组件在同一 Agent 上只能选择
`HostGateway` 或 `NativeAgent` 之一，禁止双重投影。OpenClaw、Pi 或运行时未声明 HTTP
MCP 能力的 Agent 只能使用 Host-only UI、Skill 指引和成果文件，不得向用户宣称模型
能够调用插件工具。

首版安全边界固定为：

1. 本地可执行 runtime 和 Widget 只允许 Fusion 官方审核、使用专用插件密钥签名的市场包；
2. 不开放任意 Git 仓库、本地目录或未签名可执行插件；
3. 插件包必须自包含，运行时禁止 `npm install`、`npx`、`uvx` 或其它隐式联网安装；
4. Widget 永远不能获得应用 Bearer、ACP bearer、broker token 或 Tauri API；
5. 用户数据、项目成果和插件程序目录物理分离，普通卸载不得删除前两者。

## 2. 验收标准

设计实现完成后必须满足以下结果：

1. 未安装 Cowart 时，原生安装目录和安装包中不存在 Cowart、tldraw、Cowart MCP、
   Cowart Widget 或 Cowart Skills。
2. 用户明确需要 Cowart 时，当前会话能够发现市场中的插件，展示本地代码、文件、网络、
   Widget 和 Agent 范围，只有用户确认后才下载和安装。
3. 安装完成不启动 Cowart MCP；第一次读取运行时契约或调用 Cowart 能力时单飞启动，
   并发首次调用只能产生一个对应运行实例。
4. 支持 HostGateway 的已运行 Agent 会话无需重连即可重新搜索并调用刚安装的能力；
   不支持 HostGateway 的会话显示明确降级状态。
5. Cowart Widget 在桌面和 server/browser 两种运行模式下使用同一套安全 bridge，支持
   `tools/call`、`ui/message`、resize、inline/fullscreen 和关闭清理。
6. 插件升级不会切换正在使用的 Widget 或调用实例；新调用使用新版本，旧版本在租约
   归零后回收。
7. 插件禁用或卸载立即阻止新调用，撤销 Widget lease，等待有界 drain 后停止 runtime；
   项目 `canvas/`、插件用户数据和已登记成果默认保留。
8. 插件包哈希正确但签名缺失、无效、过期或权限摘要不一致时仍必须拒绝安装。
9. v1 Skill/Connector 插件继续按现有行为安装、升级、启用、禁用和卸载；v2 的引入不得
   强迫现有包增加 runtime 或 app。
10. 每个实施阶段在修改前具有明确影响清单、回滚条件和聚焦验证；前一阶段未闭环时不得
    继续扩大公共 contract、数据库或运行时范围。

## 3. 范围与非目标

### 3.1 本设计覆盖

- `iyw-claw` 插件清单 v2、安装状态、权限和激活模型；
- Fusion Skill Market 对 v2 插件的校验、持久化、安装计划和制品签名；
- 本地 stdio MCP 的按需启动、复用、取消、诊断、回收和版本租约；
- 内置 HTTP MCP 三工具网关后的动态插件目录和路由；
- 桌面与 server/browser 共用的 MCP Apps sandbox 和 bridge；
- Codex、Claude、Gemini、OpenCode 等 Agent 的动态兼容判断和明确降级；
- Cowart 作为首个端到端验收插件。

### 3.2 首版不做

- 不允许用户直接安装 GitHub URL、本地目录或任意 npm/PyPI 包；
- 不建立第三方开发者签名、证书撤销或社区审核体系；
- 不为本地插件提供强 OS 沙箱。官方签名、代码审核和最小宿主 bridge 是首版可信边界；
- 不让插件定义 shell 字符串、安装脚本、服务管理命令或任意环境变量；
- 不把现有 remote Connector 全部迁入新 runtime；
- 不承诺 OpenClaw、Pi 和所有 Registry Agent 可以调用插件 MCP；
- 不在插件卸载中自动删除用户数据；
- 不在本设计中解决 Cowart 的 tldraw 商业授权。正式发布 Cowart 前必须单独完成许可证
  采购、key 注入或替代画布内核决策。

## 4. 当前实现审计

### 4.1 市场安装链

当前安装链已经具备可复用的基础，不应另建第二套下载器或插件目录：

```text
Skill Market UI
  -> skill_market::install_core
  -> Fusion POST /skills/install-plan
  -> Fusion POST /skills/download
  -> 校验 package size / content SHA-256 / object SHA-256
  -> 校验 ZIP 路径和插件三清单
  -> PluginStorageTransaction 暂存并切换 current.json
  -> 注册 managed MCP catalog 来源
  -> 写 plugin_installation / plugin_component_ownership
  -> 发布 Skills 到用户选择的 Agent
  -> 必要时 reconcile Agent 原生 MCP 配置
```

关键文件：

- `src-tauri/src/commands/skill_market/install.rs`：安装计划、下载和包校验入口；
- `plugin_install.rs`：插件目录、catalog、数据库和 Skill 发布的补偿事务；
- `plugin_storage.rs`：`staging -> versions/<version> -> current.json`；
- `plugin_install_rollback.rs`：失败后的目录、记录、catalog 和 Agent 配置回滚；
- `src-tauri/src/acp/skill_package.rs`：Zip Slip、符号链接、重复路径、文件数量和膨胀限制。

`main@6d1dc04` 新增 `commands/skill_watch.rs`，桌面和 server 都递归监听 central Skill
目录，500ms debounce 后调用现有 `reconcile_shared_market_skills`。v2 插件安装发布 Skill
时必须继续使用 shared-skill mutation guard，使 Watcher 只能在写入临界区结束后 reconcile；
不能再增加一套插件目录 Watcher，也不能把文件事件当作插件安装、权限或激活的权威状态。
PluginRegistry 只能在目录、数据库、catalog 和 Skill 安装事务成功后发布新 generation。

当前保护只包括 HTTPS 业务连接、包大小、内容哈希和对象哈希，没有插件发布签名字段，
也没有客户端插件签名验签。哈希能发现传输或存储篡改，但不能证明可执行代码来自获授权
发布者。增加本地 runtime 前，签名是强制门禁。

### 4.2 v1 插件契约

当前 v1 契约固定要求：

- `.codex-plugin/plugin.json`；
- `.claude-plugin/plugin.json`；
- `.iyw-plugin.json`；
- 可选 `.mcp.json`；
- `schemaVersion == 1`；
- `targets == ["codex", "claude_code"]`；
- 组件只允许 `skill` 和 `connector`；
- Connector 的 `serverKey` 必须与 `.mcp.json` 完全一致；
- Binding 只表达 `skillKey -> connectorKey`。

这套契约适合“可移植 Skill + 全局 Connector”，不能表达本地 runtime、Widget、权限、
激活方式、运行实例或用户数据。Cowart 上游包因此不能原样进入市场。

### 4.3 当前状态所有权

| 状态 | 当前所有者 | 已有保护 | 缺口 |
| --- | --- | --- | --- |
| 不可变包 | `plugins/<slug>/versions/<version>` | staging、backup、hash | 无发布签名、无打开版本租约 |
| 当前版本 | `plugins/<slug>/current.json` | 临时文件 rename | 进程崩溃后没有完整恢复扫描 |
| 安装记录 | `plugin_installation` | SQLite transaction | 一个 `status` 混合安装与可用性语义 |
| 组件归属 | `plugin_component_ownership` | owner+type+key 唯一 | 无配置、权限、激活和 runtime 信息 |
| Skill 发布 | central shared skills + Agent 目录 | marker、copy/link、回滚 | 与插件 runtime 生命周期无关联 |
| MCP catalog | `managed_mcp.catalog.v1` | source ownership、tombstone | 全局 enabled，缺 workspace/session 范围 |
| Agent MCP | 各 Agent 私有配置 | reconcile | 已运行 session 不会热加载 |

现有 `plugin_install_data::plugin_status` 还把“存在 binding”的插件直接记录为 `degraded`，
因为 Connector 默认未启用。v2 不继续扩展这个字符串；安装、激活、权限、运行和健康必须
是独立状态。

### 4.4 MCP 与 ACP 会话

当前 managed MCP 会把全局启用的 Connector 写入 8 个 Agent 的原生配置：Claude、
Codex、OpenCode、Gemini、Cline、Hermes、CodeBuddy、Kimi。Agent 的 `session/new`、
`load` 或 `resume` 使用当次组装的 MCP 列表；配置写入后，已运行 session 不会自动拥有
新工具。

主进程同时提供一个独立的 `BuiltinMcpService`：

- 监听 loopback 随机端口的 Streamable HTTP `/mcp`；
- 每个 ACP connection 签发独立 bearer；
- bearer 绑定 `connection_id + cwd + agent + feature snapshot`；
- 连接断开时撤销 bearer、broker token、MCP protocol session 和待处理调用；
- Agent 只看到三个固定工具：search、read、invoke。

这套会话 authority 正是动态插件应复用的安全边界。插件不得拿到 bearer 或 broker token；
PluginRouter 只接收宿主解析出的最小 `PluginCallContext`。

### 4.5 能力目录

现有 `CapabilityCatalog` 从编译期 companion schema 加载 40 个宿主工具，stable ID、intent
metadata、schema 和内部 tool name 都由静态 Rust 数组覆盖校验。`FeatureSnapshot` 在
MCP lease 签发时冻结。

动态插件不能直接追加到这张静态表，否则第三方代码会进入宿主高权限能力模型，并破坏
编译期覆盖校验。目标结构必须是：

```text
BuiltinCapabilityCatalog（继续静态）
          +
PluginCapabilityRegistry（安装/激活后动态快照）
          |
GatewayCatalogView（按当前 SessionAuthority 合并过滤）
```

### 4.6 前端与双运行时

当前 Rust `AcpEvent::ToolCall` 和前端 `ToolCallInfo` 会保留不透明 `meta`，但 UI 只渲染
普通工具卡。代码中没有 `ui://`、`mcpAppResourceUri`、MCP resource host 或 MCP Apps
bridge。现有 HTML/Office preview 的 iframe 和 capability proxy 可复用安全经验，但不能
直接作为插件宿主：它们没有 MCP Apps lifecycle、工具代理、消息注入和版本租约。

前端是 Next 静态导出；后端同时支持 Tauri 和独立 Axum server。因此 App Host 不能只
调用 Tauri API，也不能让 server 模式 iframe 直接访问后端 loopback。

### 4.7 Fusion 市场

Fusion 当前同样只识别 v1 的 Skill/Connector 组件，并强制 Codex、Claude 和 IYW 三份
manifest。插件组件和 binding 被归一化存表，安装计划返回服务端校验后的 manifest。

v2 必须同步修改 Fusion 领域模型、上传校验、数据库、install-plan 和 OpenAPI。客户端
不能先私自放宽 schema；否则服务端与客户端会对同一个包形成不同解释。

## 5. 方案比较

### 5.1 方案 A：继续扩展现有 Connector

给 Connector 增加 Widget 和启动信息，安装时继续写入每个 Agent 原生配置。

优点是复用代码最多。缺点是：

- 已运行会话不能热注册；
- 同一 MCP 容易同时经 native config 和内置网关重复出现；
- OpenClaw、Pi 无法使用；
- 生命周期、权限和 Widget 仍分散到各 Agent；
- 升级时无法为打开的 Widget 固定旧版本。

该方案只保留给未来明确需要 Agent 原生 OAuth 或原生工具搜索的兼容组件。

### 5.2 方案 B：宿主网关 + 按需 Plugin Runtime（采用）

插件安装到受管目录，PluginRegistry 读取签名 manifest；Agent 继续只连接内置 HTTP MCP。
网关在 search/read/invoke 内部按 session、workspace、Agent、权限和激活状态发现并调用插件，
PluginRuntimeSupervisor 在首次调用时才启动本地 MCP。

优点：

- 当前会话可热注册；
- 权限、日志、取消和回收统一；
- Widget 不依赖 ACP adapter 保留第三方 metadata；
- 同一插件可以服务多个支持 HostGateway 的 Agent；
- 不支持 MCP 的 Agent 可以明确降级而不伪造能力。

代价是需要新增可信运行时管理器、动态能力目录和通用 App Host。这是一次平台级改造，
但后续插件不再重复这些成本。

### 5.3 方案 C：每个插件自带服务与 iframe

插件自己启动端口、修改 Agent 配置并直接打开页面。

短期最快，但每个插件都会重复下载、端口、认证、SSRF、CSP、升级和回收逻辑，无法形成
统一的安装安全和跨 Agent 行为。该方案不采用。
