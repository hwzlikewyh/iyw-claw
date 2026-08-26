# 插件安全、分期与验证

本文件是 [主设计](../2026-08-26-plugin-runtime-architecture-design.md) 的安全与实施分册。

## 10. Agent 兼容策略

不以 Agent 名称静态宣称兼容。每个 session 的有效模式由以下条件共同决定：

```text
客户端编译支持
AND Fusion capability policy 允许
AND 用户本地启用
AND Agent initialize 实际声明 HTTP MCP
AND adapter 确实交付 MCP 给模型
```

| 运行结果 | UI/Agent 行为 |
| --- | --- |
| HostGateway 可用 | Skill、能力调用和 Widget 完整可用 |
| 只有 stdio/native MCP | 仅显式 NativeAgent 组件可用，通常需要新 session |
| MCP 配置被拒绝或丢弃 | Host-only UI、Skill 指引、成果文件 |
| runtime 能力未知 | fail closed，显示“尚未验证” |

当前已知硬限制：

- OpenClaw 拒绝非空 `mcpServers`；
- Pi 的 ACP adapter 接收后丢弃 MCP；
- 28 个 Registry Agent 当前定义为 ACP-only，必须逐个实测后才可开放；
- Cursor、DeepSeek 等即使编译期标记 MCP capable，仍需 initialize 与远端 policy 双重验证；
- Native MCP 配置写入不等于当前 session 已获得工具。

## 11. 安全模型

### 11.1 制品信任

Fusion 为可执行插件生成独立签名，不复用 Tauri updater、Agent binary 或 runtime tool 的
密钥。签名覆盖 domain-separated 的：

```text
plugin schema + slug + version + publisher + object SHA-256 + manifest digest
```

客户端编译内置当前及过渡公钥，校验 key id、签名、hash、slug/version 和 install-plan
一致性。签名无效时不能提供“仍然安装”按钮。

### 11.2 本地代码的真实边界

首版本地 Node/Python 进程没有强 OS 沙箱。即使宿主只传少量参数，恶意本地代码理论上仍
可读取用户权限范围内的文件或联网。因此 permissions UI 不能误导为操作系统强隔离。
首版必须依赖官方审核、专用签名、固定自包含代码和最小环境；第三方可执行插件必须等
OS sandbox 或独立容器方案完成后再开放。

### 11.3 Widget

- 不注入 Tauri globals、应用 Bearer、ACP bearer、Cookie 或用户完整路径；
- 不允许 Widget 任意 fetch 宿主 `/api`；
- 网络域名、frame 域名、资源域名和权限均取 manifest ceiling 与用户 grant 的交集；
- `tools/call` 自动绑定当前 plugin/version/connector，禁止跨插件 server 名；
- `ui/message` 必须标记来源，且不能伪装成既有用户消息；
- 限制 App HTML、单条 postMessage、调用参数、返回值和频率；
- 关闭前发送 teardown，写入操作进行中时提示可能影响。

### 11.4 文件和路径

- runtime entrypoint、schema、asset 必须 canonicalize 后仍位于版本目录；
- workspace permission 使用宿主解析后的 canonical path，不信任 Widget 或模型传入路径；
- 插件程序目录只读语义，写操作只允许 plugin-data 或获授权 workspace 子路径；
- server 模式不能把服务端绝对路径暴露给远程浏览器。

## 12. 分期实施与逐阶段后果评估

每个阶段开始前必须重新读取当前分支代码，并生成一份“拟修改文件、状态所有者、调用方、
失败模式、回滚方式、验证方式”检查表。任何公共 contract、数据库、依赖、CI 或根配置改动
仍需用户再次确认。

### 阶段 0：冻结基线与契约样本

只增加审计样本和设计验证，不改行为：

- 固定一个 v1 Skill-only、v1 Skill+Connector 和 Cowart v2 fixture；
- 记录现有 install/upgrade/uninstall、catalog 和 Agent config 前后状态；
- 记录桌面/server 的 BuiltinMcpService 与 ToolCall event 基线。

后果：无产品行为变化。若无法建立基线，不进入 schema 修改。

### 阶段 1：v2 manifest、Fusion contract 与签名

影响：Fusion domain/application/mysql/OpenAPI，Claw plugin types/manifest/install-plan/UI。

风险：放宽三清单要求可能误放旧包；新增 component 类型可能被旧客户端忽略；签名错误会
阻断所有 v2 安装。

门禁：

- v1 parser 完全独立，现有 fixture 字节级行为不变；
- v2 只以显式 `schemaVersion: 2` 进入新 parser；
- 旧客户端看到 v2 必须返回 client incompatible，不能当 v1 安装；
- 篡改 ZIP、manifest、schema、签名或 install-plan 任一字段均失败；
- 不包含 local runtime/app 的既有 v1 包不被强制签名迁移。

### 阶段 2：Registry、激活、权限与启动恢复

影响：Claw DB migration、Plugin installation service、AppState、market inventory。

风险：数据库迁移、状态重复、current pointer 漂移、卸载误删数据。

门禁：

- migration 可在含 v1 安装记录的数据库上前向执行；
- v1 安装记录被投影为原有全局行为，不自动开启 v2 runtime；
- 启动恢复能识别完整、缺指针、残留 staging、残留 trash 和 DB/目录不一致；
- 插件程序、plugin-data、workspace 成果删除范围分别验证；
- registry snapshot 更新不持有 DB transaction 跨 await。

### 阶段 3：Supervisor 与 Router，仅接 Cowart probe

影响：Cargo `rmcp` client/stdio feature 或经审核的最小 client、进程管理、日志和退出顺序。

风险：重复启动、僵尸进程、死锁、跨 workspace 状态泄露、取消后的副作用不明。

门禁：

- 100 个并发首次请求仍只有一个进程；
- 不同 workspace 默认不同实例；
- initialize/tools/list/schema mismatch 均阻止调用；
- 取消读调用安全结束，取消写调用返回 `effectMayHaveOccurred` 类状态；
- runtime crash、idle、禁用、升级、卸载和应用退出均无泄漏；
- 不在 catalog/registry/DB 锁内等待进程 I/O。

### 阶段 4：动态网关与按需安装确认

影响：Builtin capability catalog view、gateway handler、用户确认卡、市场搜索缓存。

风险：市场目录污染模型上下文、插件静默安装、当前 session 使用旧 catalog、权限绕过。

门禁：

- 顶层仍只暴露固定 search/read/invoke；
- 未安装插件只在明确、精确请求下返回 `install_required`；
- 没有用户确认不创建 staging、不写 DB、不启动进程；
- 安装成功返回新 digest，当前 session 重新 search 后可用；
- 每次 invoke 动态复核 session authority 和 permission revision；
- 不支持 HostGateway 的 Agent 得到明确不可用状态。

### 阶段 5：MCP Apps Host

影响：新 App event/DB、前端 adapter/renderer、sandbox proxy、资源 route、Tauri/server bridge。

风险：XSS、token 泄露、跨插件调用、刷新丢失、白屏、旧版本资源被删除。

门禁：

- 桌面和 server 都通过 sandbox origin/opaque-origin 验证；
- 错误 nonce、source、lease、跨 conversation、跨 plugin、过期 token 全部拒绝；
- Widget 不存在 Tauri API、应用 Bearer 和 Cookie；
- CSP 网络、frame 和资源域名逐项拒绝未声明目标；
- 刷新后从 `plugin_app_instance` 恢复，重新签发 lease；
- 插件升级期间旧 Widget 固定旧版本，版本目录不会提前清除；
- Cowart 已知中等宽度白屏场景纳入真实 WebView 回归。

### 阶段 6：NativeAgent 兼容与更多插件

只有 HostGateway/Cowart 全链路稳定后再做。每个 Agent/插件组合必须有工具只出现一次、
新 session 生效提示、禁用/卸载不覆盖用户自有配置的验证。不得用静态 `supports_mcp=true`
替代真实运行测试。

## 13. Cowart 首个插件验收

Cowart IYW 包必须：

- 包含预构建、自包含 MCP 与 Widget，不在安装或启动时安装依赖；
- 使用受管 Node，不假设系统 PATH；
- 将项目目录作为经过宿主授权的参数传入，不把插件仓库作为 canvas 目录；
- 使用 HostGateway，不写 Codex/Claude/Gemini 等原生 MCP 配置；
- 将三个 Cowart Skill 发布到用户选择且支持 Skill 的 Agent；
- 没有可调用 MCP 的 Agent 仍可由用户手动打开宿主画布，但不能自动插入生成结果；
- 默认保存到 `<workspace>/canvas`，卸载保留；
- 图片生成优先绑定现有 `iyw-image-workflows`，不能把 Codex `imagegen` 当成所有 Agent
  都具备的能力；
- 正式发行前完成 tldraw production license 与 key/domain 策略；
- 覆盖打开、保存、选择、插图、HTML、Slides、刷新、升级、卸载和白屏回归。

## 14. 验证矩阵

### 14.1 静态与 contract

- Fusion 与 Claw 对 v1/v2 manifest 生成同一 canonical 结果；
- OpenAPI、Rust/TypeScript 镜像和数据库 entity 同步；
- 插件 component、capability ID、schema path、resource URI 和 permission 唯一性通过；
- `git diff --check`、格式化和聚焦 lint 通过。

### 14.2 安装可靠性

- 网络中断、短包、hash 错误、签名错误、磁盘满、rename 失败、DB 失败、Skill 发布失败；
- 安装、重装、升级、降级、权限扩大、禁用、卸载、启动恢复；
- 所有故障后最多保留一个可解释的 current 版本，最后有效版本不丢失。

### 14.3 Runtime

- cold start、warm reuse、并发 single-flight、不同 workspace、idle TTL；
- initialization timeout、stderr flood、协议乱码、schema mismatch、进程崩溃；
- 调用取消、会话断开、升级 drain、卸载 drain、应用退出。

### 14.4 Agent

- 每个内置 Agent记录 initialize capability、实际 tools/list 与真实一次调用；
- HostGateway、NativeAgent、Host-only 三种结果与 UI 文案一致；
- 已运行会话热安装、重启应用、恢复会话、Agent runtime 重启；
- 工具不会重复出现，unsupported Agent 不出现幽灵能力。

### 14.5 Widget 安全与可用性

- desktop Windows/macOS/Linux 与 server Chromium/WebKit 可用目标；
- CSP、sandbox、MessageChannel、nonce、大小/频率、跨实例攻击；
- inline/fullscreen、resize、theme、tools/call、ui/message、teardown；
- 页面刷新、历史恢复、插件禁用、升级和卸载；
- Cowart 宽度、DPR、剪贴板、文件保存和项目资源懒加载。

### 14.6 未验证不得声称兼容

静态声明、安装成功、MCP initialize、HTTP 200、Agent connected 和 Widget HTML 返回都不
等同端到端成功。最终报告必须区分：

- contract validation；
- artifact validation；
- runtime handshake；
- Agent tool visibility；
- model real invocation；
- Widget render/bridge；
- installed-client release verification。

## 15. 实施前审批点

本设计通过后仍需分阶段批准以下高影响操作：

1. Fusion 与 Claw 公共 plugin contract v2；
2. 两边数据库 migration；
3. Claw Cargo `rmcp` client/stdio feature 或新增 MCP client 依赖；
4. 专用插件签名密钥和发布流水线；
5. App event/DB 与前端 renderer；
6. Tauri/server sandbox 资源和安全策略；
7. Cowart/tldraw 生产许可；
8. CI、根配置、发布和远程 Git 操作。

在这些审批完成前，分支只保留本设计文档，不进行实现。
