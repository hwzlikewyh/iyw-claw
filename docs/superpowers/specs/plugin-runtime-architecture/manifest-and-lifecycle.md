# 插件清单、持久化与生命周期

本文件是 [主设计](../2026-08-26-plugin-runtime-architecture-design.md) 的清单与生命周期分册。

## 7. 插件清单 v2

`.iyw-plugin.json` 成为 v2 的权威清单。Codex/Claude native manifest 改为可选导出：只有
组件声明 `routing.mode = native_agent` 时才要求相应 native manifest 并校验身份。

示例：

```json
{
  "schemaVersion": 2,
  "name": "cowart",
  "version": "0.1.27-iyw.1",
  "targets": ["iyw-claw"],
  "components": {
    "skills": [
      {
        "key": "cowart-open-canvas",
        "path": "skills/cowart-open-canvas",
        "requiresConnectors": ["cowart-mcp"]
      }
    ],
    "runtimes": [
      {
        "key": "cowart-node",
        "kind": "node",
        "entrypoint": "mcp/generated/cowart-mcp.mjs",
        "dependencies": "bundled"
      }
    ],
    "connectors": [
      {
        "key": "cowart-mcp",
        "transport": "stdio",
        "runtimeKey": "cowart-node",
        "routing": { "mode": "host_gateway" },
        "activation": { "mode": "lazy", "scope": "workspace" }
      }
    ],
    "capabilities": [
      {
        "key": "open-canvas",
        "id": "plugin.cowart.canvas.open.v1",
        "connectorKey": "cowart-mcp",
        "toolName": "render_cowart_canvas_widget",
        "schemaPath": "contracts/open-canvas.schema.json",
        "description": "Open the Cowart canvas for the active workspace.",
        "intentTerms": ["cowart", "canvas", "画布"],
        "readOnlyHint": true
      }
    ],
    "apps": [
      {
        "key": "canvas",
        "connectorKey": "cowart-mcp",
        "resourceUri": "ui://widget/cowart/canvas.html",
        "capabilityKey": "open-canvas",
        "displayModes": ["inline", "fullscreen"]
      }
    ]
  },
  "permissions": {
    "workspace": {
      "read": ["canvas/**"],
      "write": ["canvas/**"]
    },
    "network": {
      "connectDomains": []
    },
    "host": ["send-message", "clipboard-write"]
  }
}
```

约束：

- 所有路径为 POSIX 相对路径，必须出现在签名包文件清单中；
- runtime kind 只允许客户端编译支持的枚举；
- `entrypoint` 不允许 shell、参数替换、环境展开和未声明下载；
- capability schema 独立文件必须计入内容摘要；
- CSP、network 和 host 权限只能收窄，不能由 runtime 返回值扩大；
- `native_agent` 与 `host_gateway` 对同一 connector/Agent 互斥；
- v1 继续走原 parser，不把 v1 静默推断成 v2。

## 8. 持久化模型

### 8.1 保留并扩展安装记录

`plugin_installation` 继续表示“某个不可变市场 artifact 已安装”，新增：

- `schema_version`；
- `publisher_id`；
- `trust_state`；
- `artifact_signature_key_id`；
- `permissions_digest`；
- `reconcile_state`。

原有 `status` 迁移为纯安装健康状态：`installed`、`repair_required`、`removing`。不能再用它
表示 Connector 未启用或 runtime 未启动。

`plugin_component_ownership` 增加 `component_config_json`，保留现有平铺字段以兼容 v1
查询和回滚。Fusion 对应组件表使用同样的低成本扩展，不删除旧列。

### 8.2 新增激活策略

`plugin_activation_policy`：

```text
plugin_slug
component_key
scope                 global | workspace
workspace_key
agent_type
requested_enabled
routing_mode
policy_source
updated_at
```

唯一键为 `(plugin_slug, component_key, scope, workspace_key, agent_type)`。这张表表达用户
希望，不代表 runtime 当前正在运行。

### 8.3 新增权限授权

`plugin_permission_grant`：

```text
plugin_slug
scope
workspace_key
permissions_digest
granted_permissions_json
grant_state            granted | revoked
granted_at
updated_at
```

升级后权限摘要完全相同可复用授权；权限减少可自动沿用；任何新增文件、网络、host 或本地
代码能力都必须重新确认。旧授权只按摘要匹配，不按版本字符串猜测兼容。

### 8.4 App 实例

`plugin_app_instance` 保存可恢复的宿主展示状态：

```text
instance_id
conversation_id
tool_call_id
plugin_slug
plugin_version
app_key
workspace_key
launch_payload_json
state
created_at
updated_at
```

不持久化 Widget lease、runtime PID、token 或 iframe 消息。插件卸载后保留轻量记录，使历史
会话显示“插件已卸载”，而不是丢失整块内容。

运行实例、引用计数和 PID 只存在于 `PluginRuntimeSupervisor` 内存，不建持久进程表。

## 9. 安装、升级、禁用与卸载

### 9.1 安装

```text
市场精确候选
-> 用户确认权限和本地代码
-> install-plan
-> 下载不可变 ZIP
-> SHA-256 + 专用插件签名验证
-> ZIP/manifest/file/schema 校验
-> staging
-> 写安装记录、组件和默认 disabled 策略
-> 发布 Skill
-> 切换 current.json
-> 发布 registry generation
```

HostGateway connector 安装时不写 Agent 原生 MCP 配置，也不启动 runtime。任何阶段失败都
回滚目录、数据库、Skill 发布和 registry；启动恢复扫描负责处理进程崩溃留下的 staging、
backup 或无效 current pointer。

### 9.2 升级

升级先验证新包和权限差异，再落新版本。权限扩大时新版本可以安装但保持
`pending_permission`，旧版本继续服务现有 lease。用户批准后 current pointer 和 registry
切换，新调用使用新版本；旧 runtime/App 在引用归零后回收。

升级不得原地覆盖正在运行的版本目录，也不得因为新版本启动失败删除最后一个可用版本。

### 9.3 禁用

禁用顺序：

1. activation policy 置 false；
2. 发布 registry generation，阻止新 search/read/invoke；
3. 撤销未开始的安装/调用和 Widget lease；
4. 对正在执行的写调用按 effect policy 返回明确状态；
5. 有界 drain 后停止 runtime。

### 9.4 卸载

卸载在禁用基础上：

1. 移除 Skill 投影和 NativeAgent 配置；
2. 从 registry、catalog source 和安装记录移除所有权；
3. 把插件程序目录移入 trash；
4. 完成后清理 trash；失败则恢复目录和记录；
5. 保留 `plugin-data`、项目文件和历史 `plugin_app_instance`。

用户若选择删除插件数据，必须是独立确认、显示精确目录和不可恢复后果的操作。
