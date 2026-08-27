# 插件运行时目标架构

本文件是 [主设计](../2026-08-26-plugin-runtime-architecture-design.md) 的目标架构分册。

## 6. 目标架构

```text
Fusion Plugin Market
  | install-plan + immutable artifact + dedicated signature
  v
Plugin Installer
  | verify -> stage -> activate artifact
  v
PluginRegistry ------------------------- PluginActivationPolicy
  | immutable descriptors                PluginPermissionGrant
  |
  +-> GatewayCatalogView <---- SessionAuthority / Agent runtime capability
  |        |
  |        +-> PluginRouter -> PluginRuntimeSupervisor -> local/remote MCP
  |
  +-> PluginAppRegistry -> PluginAppHost -> isolated sandbox iframe
                               |
                               +-> own tools/call only
                               +-> consented ui/message
```

### 6.1 `PluginRegistry`

职责：

- 从数据库和当前版本指针建立不可变 registry snapshot；
- 保存插件、版本、组件、capability、app、permission ceiling 和 routing mode；
- 提供单调递增 `registry_generation` 与 digest；
- 在安装、升级、禁用、权限变化和卸载后原子发布新 snapshot；
- 启动时核对 DB、`current.json`、版本目录和 catalog，标记或修复不一致。

Registry 不负责启动子进程，不持有数据库锁等待外部 I/O，也不删除用户数据。

### 6.2 `PluginRuntimeSupervisor`

首版只支持经过审核的本地 stdio MCP。运行实例键为：

```text
(plugin_slug, plugin_version, connector_key, workspace_key)
```

默认不跨 workspace 共享。以后只有 manifest 明确声明且安全审计确认无工作区状态时，才可
支持 installation-wide 共享。

Supervisor 必须实现：

- 每个实例键的 single-flight 启动；
- 使用受管 Node/Python 的绝对路径，不通过 shell；
- entrypoint 必须位于当前签名版本目录；
- 只传宿主白名单环境变量，不继承凭证和完整用户环境；
- MCP initialize、tools/resources contract 校验；
- 并发调用上限、启动/调用/关闭超时；
- request cancellation 与 session cancellation 传播；
- 引用计数、Widget lease、空闲 TTL 和有界 drain；
- stderr 有界收集和敏感字段脱敏；
- 重复崩溃后的 quarantine，用户显式重试才能解除；
- 应用退出时停止接收新调用并有界关闭全部实例。

不能在 managed MCP catalog lock、数据库 transaction 或 registry 写锁内等待进程启动、
MCP 调用或停止。固定锁顺序为：

```text
读取 registry snapshot -> 释放 registry lock
-> 获取 runtime-key async lock -> 启动/调用
-> 需要持久化时在调用完成后独立写 DB
```

### 6.3 `PluginRouter`

Router 接收宿主已经解析和认证的：

```text
PluginCallContext {
  connection_id,
  conversation_id,
  agent_type,
  workspace_key,
  cwd,
  plugin_slug,
  plugin_version,
  capability_id,
  permission_revision,
  cancellation,
}
```

每次调用重新检查：

1. artifact 仍是 trusted/current 或被现有 lease 固定；
2. component 对当前 workspace 和 Agent 请求启用；
3. 当前 Agent session 实际具有 HostGateway 能力；
4. permission grant digest 与当前 manifest 一致；
5. capability 的静态 schema 校验通过；
6. live MCP `tools/list` 名称和 schema digest 与签名 descriptor 一致；
7. session authority 未撤销。

任一检查失败返回稳定错误，不猜替代工具，也不自动切到 NativeAgent。

### 6.4 动态能力目录

Plugin capability 必须在签名 manifest 中显式声明 stable ID、Connector、真实 MCP tool、
输入 schema、短描述、意图 metadata 和只读/写入提示。建议 ID 采用：

```text
plugin.<plugin-slug>.<component-key>.<action>.v1
```

ID 和版本由发布者声明，Fusion 校验唯一性和格式；客户端不根据运行时工具名猜 stable ID。
安装时可读取 descriptor，无需启动插件。第一次启动 runtime 后，Supervisor 必须把 live
`tools/list` 与 descriptor digest 对比，不匹配则将组件标记为 `runtime_contract_mismatch`。

Gateway search 合并两类结果：

- 已安装且当前 session 可用的宿主/插件能力；
- 用户明确请求具体插件或能力时，Fusion 市场缓存中的精确候选，状态为
  `install_required`。

不把整个市场目录注入模型上下文。远端搜索只发送 2–5 个规范化关键词，不发送完整用户
消息、文件内容或会话历史。

当结果为 `install_required` 时，Agent 不能直接 invoke 该插件。它必须调用一个固定宿主
能力 `iyw.plugins.install.request.v1`。该调用复用现有用户交互机制停放，前端显示插件
身份、版本、大小、本地代码、权限和 Agent 范围。用户批准后执行安装，返回新的 catalog
digest；Agent 随后重新 search/read/invoke。拒绝或取消不会产生安装文件。

### 6.5 `PluginAppHost`

App Host 不依赖 Codex/Claude 等 adapter 原样转发 `openai/outputTemplate`。PluginRouter
识别自己签名的 app binding，在主进程创建宿主拥有的 `PluginAppLaunch`：

```text
PluginAppLaunch {
  instance_id,
  conversation_id,
  tool_call_id,
  plugin_slug,
  plugin_version,
  app_key,
  resource_uri,
  display_mode,
  launch_payload,
}
```

启动记录不含 token、绝对插件路径、凭证或完整 MCP raw payload。记录持久化到应用数据库，
用于刷新和历史恢复；真实 Widget lease 每次渲染重新签发，不能持久化。

安全宿主使用 MCP Apps 的双 iframe 模式：

1. 前端加载由应用内置、版本固定的 sandbox proxy；
2. sandbox proxy 与主应用不同源，首版用宿主生成的 opaque sandbox 文档并验证浏览器行为；
3. Host 只在 proxy ready 后通过一次性 `MessageChannel` 传入 app HTML；
4. proxy 按 manifest permission ceiling 构建 CSP 和内层 iframe；
5. 消息同时校验 `event.source`、随机 nonce、instance lease、方法白名单、大小和频率；
6. Widget 只能调用自身声明并获授权的 server tool；
7. `ui/message` 经宿主校验后进入当前对话，并向用户显示来源；
8. fullscreen、resize、open-link 和 clipboard 分别受宿主 capability 控制；
9. 关闭、禁用、升级、卸载、connection revoke 或超时都会先发送 teardown，再撤销 lease。

若目标 WebView/浏览器无法满足不同源 sandbox、MessageChannel 或 CSP，App Host fail closed，
显示“不支持交互 Widget”，不能降级为同源 `srcDoc` 执行。

### 6.6 用户数据

目录分为三类：

```text
<iyw-home>/plugins/<slug>/versions/<version>/    # 签名程序，只读语义
<iyw-home>/plugin-data/<slug>/...                # 应用级插件数据
<workspace>/canvas/...                           # Cowart 等项目成果
```

普通卸载只删除第一类和注册状态。第二类仅在独立“删除插件数据”操作中删除，第三类永远视为
用户项目文件，只能由用户在文件系统或明确的数据清理流程中删除。
