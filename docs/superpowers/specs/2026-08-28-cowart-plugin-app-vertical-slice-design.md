# Cowart 插件启用与 MCP Apps 纵向接入设计

日期：2026-08-28

状态：待用户校对，未确认本文前不修改业务代码

设计基线：`iyw-claw@7323931e294cdd227b65f9826e5218bf478c8489`

关联设计：`2026-08-26-plugin-runtime-architecture-design.md`

## 1. 决策摘要

Cowart 继续作为 Fusion 市场中的可选 v2 插件，不进入原生安装包。`iyw-claw` 保留固定的
`search_iyw_capabilities -> read_iyw_capability -> invoke_iyw_capability` 三工具入口；
`cowart-mcp` 由 HostGateway 在用户授权后按 workspace 懒启动，不作为
`mcp__cowart_mcp__*` 顶层工具暴露给 Agent。

本次同时修复四个连续断点：

1. 插件预览卡片不能可靠打开市场详情；
2. 市场安装后 Connector 保持关闭、权限保持 pending，但没有可达的启用入口；
3. Cowart Skills 仍要求寻找不存在的原生 MCP 工具，并把持久状态问题误报为需要新会话；
4. `PluginAppHost` 基础组件没有接入能力调用、消息内容和历史恢复链路。

Cowart 在全部验收完成前从 Fusion 市场隐藏。隐藏只阻止新的发现和安装，不删除既有安装、
插件数据或 workspace 中的 `canvas/` 成果。重新上架必须同时满足运行时、Widget、安全、
安装客户端和 tldraw 生产许可门禁。

## 2. 已验证现状

### 2.1 市场与安装

Fusion 中 Cowart `0.1.28` 是 ready 的 v2 插件，包含：

- `app/cowart-canvas`，资源 `ui://widget/cowart/canvas.html`；
- `capability/render-canvas`，stable ID
  `plugin.cowart.canvas.render-canvas.v1`；
- `connector/cowart-mcp` 与受管 Node runtime；
- `cowart-open-canvas`、`cowart-image-gen`、`cowart-image-edit` 三个 Skill。

本机日志证明下载、校验、插件记录和三个 Skill 投影均成功。SQLite 中安装记录为
`installed/trusted/ready`，但运行状态为：

```text
activation: component=cowart-mcp, scope=workspace,
            workspace_key="", requested_enabled=false
permission: scope=global, grant_state=pending,
            granted_permissions_json={}
app instances: 0
```

因此 capability 的 `unavailable` 不是“等待 MCP 自动加载”。目录代码只有在插件可用、
当前 workspace/Agent activation 已启用且 permission digest 已授权时才返回 available。
重新启动应用、Agent 或会话不会改变这些持久化状态。

### 2.2 Agent 路由

当前 `cowart-open-canvas` 指导 Agent 寻找
`mcp__cowart_mcp__render_cowart_canvas_widget`，与 HostGateway 架构冲突。Agent 随后虽然
发现 stable capability，却只得到无原因的 `unavailable`，无法区分待授权、Connector
关闭、插件损坏或 Agent 不支持。

### 2.3 Widget 宿主

后端已有 `PluginAppRegistry`、持久化 instance、lease、nonce 和 resource read 基础；前端
已有 `PluginAppHost` 与 sandbox bridge。但当前消息 renderer 没有引用 `PluginAppHost`，
后端没有从 plugin invoke 创建 app launch，`plugin_app_instance` 为空。HTML 能被 MCP
返回不等于界面已经渲染。

## 3. 验收标准

1. Cowart 插件预览卡整张可点击；无论技能列表是否已加载，都能打开同一市场详情。
2. 安装和启用继续分离。安装不静默授权可执行代码、网络、剪贴板或 workspace 写权限。
3. 已安装但未授权的 Cowart 首次被明确调用时，Agent 能识别待授权原因并展示宿主确认；
   拒绝后不启动 runtime、不创建 app instance、不改变 grant。
4. 用户批准后，只为当前 canonical workspace 与当前 Agent 启用 `cowart-mcp`，并保存与
   当前 manifest permission digest 匹配的 workspace grant；其它 workspace 不继承。
5. 同一会话重新 search 后 capability 变为 available，无需重启 Agent 或创建新会话。
6. 调用 `plugin.cowart.canvas.render-canvas.v1` 后，当前助手回复内显示真实 tldraw Widget；
   同一 instance 可进入和退出 fullscreen，不创建第二份 canvas。
7. 刷新、会话历史恢复或前端重挂载会重新签发 lease 并恢复同一 instance；不持久化
   lease token、nonce、Bearer、绝对插件路径或完整原始 MCP payload。
8. 权限摘要变化、插件禁用/卸载、版本资源缺失、runtime 崩溃和不支持 Widget 的浏览器
   都显示确定错误态，不白屏、不自动回退到旧本地 Web 服务。
9. 禁用或卸载立即阻止新调用、撤销 app lease 并有界回收 runtime；保留 plugin-data 和
   workspace `canvas/`。
10. Windows 正式安装客户端中完成真实安装、授权、Agent 调用、inline/fullscreen、保存、
    刷新恢复、升级、禁用和卸载验证后，Cowart 才能重新上架。

## 4. 范围与非目标

### 4.1 本次覆盖

- 插件预览卡与市场详情导航状态机；
- 插件 activation、permission grant 和具体 unavailable reason；
- 当前 workspace/Agent 的显式首次使用授权；
- Cowart 三个 Skill 的 HostGateway 路由说明；
- plugin invoke 到 app launch、resource、消息内容、inline/fullscreen 和恢复；
- Cowart 市场隐藏、更新包和重新上架门禁。

### 4.2 本次不做

- 不向 Agent 暴露动态顶层 `mcp__cowart_mcp__*` 工具；
- 不把通用 MCP 设置页改造成插件运行时管理中心；
- 不自动全局授权插件，不把安装时选择的 Agent 等同于 workspace runtime 授权；
- 不开放第三方未审核本地 runtime、安装脚本或运行时依赖下载；
- 不恢复 `scripts/start-canvas.sh` 作为正式产品路径；
- 不顺带实现 NativeAgent 双投影或其它插件的定制 Widget。

### 4.3 通用插件接入约束

Cowart 只是首个验收包，不得成为宿主代码中的特例。核心 Host、Gateway、数据库、API、
消息 adapter 和前端 renderer 禁止按 `cowart` slug、tool name 或 resource URI 分支。未来
v2 插件只通过 manifest 声明 runtime、connector、capability、app、permissions 和 binding，
即可复用同一安装、授权、ticket、instance、lease、resource、tools/call、恢复和卸载链路；
若一个新插件仍需要修改通用宿主才能显示其标准 MCP App，视为宿主 contract 不完整。

## 5. 插件状态与授权

### 5.1 状态分解

市场安装状态、运行授权和 live runtime 必须继续独立：

```text
not_installed
  -> installed + connector_disabled + permission_pending
  -> installed + enabled(workspace, agent) + permission_granted(workspace)
  -> runtime_starting -> runtime_ready
  -> app_active(instance lease)
```

`PluginCapabilityRegistry` 为不可用能力返回稳定原因：

- `plugin_unavailable`：安装指针、trust 或 reconcile 不可用；
- `connector_disabled`：当前 workspace/Agent 没有启用记录；
- `permission_pending`：当前 permission digest 没有 workspace grant；
- `runtime_quarantined`：运行时重复崩溃后被隔离；
- `unsupported_agent`：Agent 没有真实 HostGateway 能力时不宣称可调用。

原因只描述宿主状态，不泄露本地路径、权限内容或内部错误堆栈。

### 5.2 用户授权事务

新增固定宿主能力 `iyw.plugins.enable.request.v1`，只处理已经安装但当前作用域未启用的插件。
输入使用 stable plugin slug；插件版本、组件、permission ceiling 和摘要全部从本地 trusted
registry 读取，不接受 Agent 传入的权限 JSON。

调用流程：

1. SessionAuthority 提供 canonical workspace、Agent、connection cancellation；
2. 宿主展示插件、版本、本地代码、workspace 读写、网络域名、剪贴板和 open-link 权限；
3. 用户拒绝或取消时返回 denied，不写 activation/grant；
4. 用户批准时在一个 SQLite transaction 中为每个 HostGateway Connector upsert 精确的
   `(plugin, connector, workspace, agent)` activation，并 upsert 当前 workspace grant；
5. 提交后发布新 registry generation，再允许重新 search/invoke；
6. permission digest 变化后旧 grant 不匹配，必须重新确认，不沿用更大的历史授权。

现有空 workspace、disabled activation 和 pending grant 作为安装默认记录保留，不能通过
原地 update 把唯一一条记录从 workspace A 搬到 workspace B。多 workspace/Agent 授权必须
并存且可独立撤销。

## 6. 市场与 Skill 行为

### 6.1 卡片导航

插件预览卡使用一个覆盖完整卡片摘要区域的 button。点击只打开详情，不直接安装。
导航 target 到达后：

1. 当前列表已有 slug 时立即选中并打开详情；
2. 否则切到 market 标准过滤条件并触发精确 slug 查询；
3. 查询成功后打开详情并消费 requestId；
4. 加载失败保留重试，确实不存在时显示 not-found；
5. 不再要求 target 必须先经历一次 `list.loading=true` 才可处理。

### 6.2 Cowart Skills

三个 Cowart Skill 统一说明：

- 只通过 IYW capability gateway 搜索、读取和调用 stable capability；
- `render-canvas` available 时调用一次，不搜索原生 MCP namespace；
- `permission_pending` 或 `connector_disabled` 时调用
  `iyw.plugins.enable.request.v1`，用户批准后重新 search/read/invoke；
- `plugin_unavailable`、`runtime_quarantined` 或 `unsupported_agent` 直接报告准确状态；
- 不建议通过重开会话、等待自动加载或启动旧 Web 服务修复持久授权问题。

## 7. App Launch 与渲染

### 7.1 后端数据流

```text
Agent invoke stable capability
  -> Gateway validates session + schema
  -> PluginRouter rechecks trusted version + activation + permission
  -> Supervisor starts/reuses cowart-mcp and calls render tool
  -> Host resolves app binding from trusted manifest
  -> Host reads declared ui:// resource from the same plugin/version/connector
  -> PluginAppRegistry persists token-free instance and issues ephemeral lease
  -> Tool result carries host-owned plugin-app launch reference
  -> message adapter renders PluginAppHost
```

Agent/MCP 返回值不能指定任意 app key、resource URI、HTML URL、plugin/version 或 display
mode。这些字段必须来自当前 trusted manifest 的 capability-to-app binding。tool payload 只可
作为经过大小和结构限制的 `launch_payload`。

App launch 必须绑定 conversation、可见 gateway tool call、workspace、plugin version 和
permission revision。若现有 SessionAuthority 缺少 conversation/tool-call identity，由宿主
调用链增加不可伪造的内部绑定；该身份不下放给插件。

### 7.2 前端展示

新增显式 `plugin-app` 内容 part。inline 使用稳定的响应式高度，不因 loading、resize 或错误
改变周围消息布局；工具栏只提供 fullscreen、关闭和明确状态图标，使用现有 Lucide 图标与
tooltip。fullscreen 复用同一 instance、bridge 和 canvas state，退出后回到原回复位置。

前端先获取宿主代理文档和短期 resource lease，再把 HTML 交给 opaque sandbox。Widget
不能访问 Tauri globals、应用/ACP token、Cookie 或任意宿主 API。`ui/message`、resize、
clipboard、open-link 与 Widget tools/call 都继续校验 source、nonce、lease、方法、大小、
频率和 manifest/grant 交集。

### 7.3 恢复与错误

历史记录只保存 instance reference。恢复时后端重新检查：插件版本目录仍存在、app binding
未改变、workspace/Agent activation 仍启用、permission digest 仍获授权。通过后读取资源并
签发新 lease；失败则返回 `disabled`、`permission_changed`、`version_missing`、
`runtime_unavailable` 或 `widget_unsupported` 状态，不执行旧 HTML。

升级期间打开的 instance 固定旧版本，直到 lease 释放；新调用使用新版本。清理旧版本前
必须确认没有 runtime 或 app lease。关闭、禁用、卸载和 connection revoke 先 teardown，
再撤销 lease 和有界停止 runtime。

## 8. 安全与可观测性

- capability、app、schema、entrypoint 和 resource path canonicalize 后必须位于安装版本目录；
- app HTML、launch payload、postMessage、tool 参数/返回值和 resize 都有硬上限；
- CSP 取宿主上限、manifest ceiling 与用户 grant 的交集，默认拒绝未声明网络与 frame；
- `resources/read` 保持标准 MCP Apps JSON-RPC；资源 `_meta.ui.csp` / `permissions` 经过
  manifest ceiling、用户 grant 与资源声明的交集后下发，供后续插件复用；
- 日志记录 plugin/version、workspace hash、Agent、状态转换、runtime/instance ID 和稳定错误码；
- 不记录 token、nonce、完整权限 JSON、HTML、用户消息、绝对 workspace 路径或 canvas 内容；
- 权限拒绝、runtime 崩溃和 Widget bridge 失败分别记录，不能全部归为 `unavailable`。

## 9. 发布与验证

### 9.1 临时处置

spec 审批后先在 Fusion 将 Cowart 标记为 disabled，并验证普通用户目录不再返回它。保留
现有 artifact 与安装记录，便于迁移和回归；不删除本机已安装副本或 canvas 数据。

### 9.2 实施验证

仓库 `AGENTS.md` 默认不新增或运行测试文件。本次先执行允许的静态验证：Rust fmt、Cargo
metadata、定向 TypeScript/ESLint、i18n key 检查、`git diff --check` 和完整调用链审查。
除非用户另行批准，不新增单元/集成/E2E 测试文件。

此外必须用真实 Cowart 包和正式安装客户端完成运行验证：

当前已完成隔离 Cowart 包的 runtime JSON-RPC smoke；这只证明插件进程、工具目录和
`text/html;profile=mcp-app` 资源可读，不替代正式客户端 UI 证据。

- 已安装 pending 状态无需重装即可出现授权确认；拒绝与批准路径均核对 SQLite；
- 当前 workspace/Agent 可用，其它 workspace/Agent 仍不可用；
- runtime cold start、warm reuse、schema 校验、崩溃和取消；
- inline 首帧非空、fullscreen 同实例、resize、DPR 与中等宽度；
- 保存/选择、图片、HTML、复制、下载、刷新与历史恢复；
- 权限摘要变化、升级保留旧 lease、禁用、卸载和应用退出无残留进程；
- desktop Windows 必测，server/browser 和其它发布平台未实测时必须明确标注。

### 9.3 重新上架门禁

1. tldraw production license/key/domain 已有可审计结论；
2. Cowart 更新包的 manifest、Skills、runtime 与 Widget 制品通过 Fusion 校验；
3. 新版 `iyw-claw` 已发布，普通 updater 与 Tauri updater 验证通过；
4. 已安装客户端完成上述真实纵向验证；
5. Cowart 重新启用后再用普通账号验证目录、安装和首次使用。

## 10. 失败与回滚

- 市场导航修复失败：回滚前端提交，不影响安装状态；
- 授权事务失败：transaction 回滚，registry generation 不变，runtime 不启动；
- runtime/app 接入失败：保持 Cowart disabled，已发布客户端不宣称 Widget 可用；
- Widget 安全门禁失败：fail closed，显示不支持，不回退同源 iframe 或旧 Web 服务；
- 新 Cowart 包失败：保留旧 artifact 供审计但不重新启用；
- 回滚客户端不得删除 plugin-data、workspace `canvas/` 或用户已生成成果。
