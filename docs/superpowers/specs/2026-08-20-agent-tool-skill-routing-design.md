# Agent 工具与 Skill 准确路由设计

日期：2026-08-20

## 目标

让 Agent 能在当前会话内准确发现、读取并调用 iyw-claw 的工具和 Skill：

- 工具只通过稳定 `capability_id` 调用，不允许根据内部工具名猜路由。
- `tools/list`、搜索结果、读取结果和实际 dispatch 使用同一份能力状态。
- 调用前校验业务参数，错误明确指出缺少字段、类型错误、未知字段或能力不可用。
- Skill 的触发条件、排除条件、别名、能力边界和调用方式以结构化 routing card 暴露。
- 禁用、缺依赖、运行时不可用与未知 ID 分开表达，失败后停止猜测替代路径。

本设计不改变当前 MCP Streamable HTTP 的认证、session、`/mcp` 路由和传输生命周期，也不把 38 个内部工具重新暴露为 Agent 可见工具。

## 当前事实

HTTP `tools/list` 只暴露三个网关工具：

```text
search_iyw_capabilities
read_iyw_capability
invoke_iyw_capability
```

网关再通过 `CapabilityCatalog` 将稳定 ID 映射到内部工具，并由 `FeatureSnapshot` 同时决定可见性和调用授权。工具 schema 当前嵌入 `delegation/tool_schema.json`，稳定 ID 注册表位于 `builtin_mcp/capability_registry.rs`，dispatch 还受 `CompanionFeatures` 和 family 路由表约束。

Skill inventory 已能扫描、合并、计算启用状态和描述预算；市场上传校验已经要求 routing card，但内置 Skill 和 Agent-facing `AgentSkillItem` 尚未提供同一套结构化字段。

## 统一路由契约

### Capability metadata

每个能力在 catalog 内形成一条不可变记录：

```text
capability_id       稳定、唯一、版本化 ID，例如 iyw.memory.recall.search.v1
tool_name           仅主进程内部使用，不返回给 Agent 作为调用入口
description         面向 Agent 的能力说明
input_schema        当前 JSON Schema
schema_digest       sha256:<hex>，用于识别 stale schema
aliases             由稳定 ID 的分类和动作路径生成的公开短检索词
category            automation/browser/memory/channel 等
status              available 或 unavailable
unavailable_reason  disabled/missing_dependency/runtime_not_ready/schema_invalid
required_inputs     从 object schema 提取的必填字段摘要
```

`search` 返回 `capability_id`、summary、category、aliases、status、required_inputs 和 schema_digest；`read` 返回完整 metadata 与 `input_schema`。为保持兼容，旧字段继续保留，新增字段均可选读取。

### Skill routing card

Skill 的 `SKILL.md` frontmatter 使用以下字段，camelCase 和 snake_case 均可读，写入和示例统一 camelCase：

```yaml
routing:
  capability: memory or capability-gateway
  coreTriggers:
    - user asks to recall durable project context
  exclusions:
    - current-turn facts that are already in the conversation
  aliases:
    - memory recall
  invocation: Read SKILL.md first; use only documented commands and paths.
```

投影到 `AgentSkillItem` 和 inventory 时使用结构化 `routing` 对象及 `routing_status`；对象内保留 `capability`、`coreTriggers`、`exclusions`、`aliases` 和 `invocation`。缺少 routing card 的已有本地 Skill 不阻断扫描，但状态标记为 `missing`，Agent 侧只能看到普通 description，不能据此自动触发。

## 运行流程

1. 服务启动时解析 embedded schema，执行 schema 与 stable binding 的双向覆盖、唯一性和 ID 格式校验；失败则服务不报告 ready。
2. Agent 看到完整三件套后，先用当前 schema 搜索；搜索只返回当前 session 可用的能力。
3. Agent 对候选 ID 执行 `read`，读取当前 schema、必填字段、状态和 digest。
4. `invoke` 先按稳定 ID resolve，再按同一个 schema 校验 arguments，最后执行既有 FeatureSnapshot 授权和 CompanionBridge dispatch。
5. 参数或路由失败返回机器可读错误；Agent 不重试另一个 namespace、内部工具名或猜测字段。
6. Skill discovery 先返回压缩 routing card；真正执行前必须读取对应 `SKILL.md`，并遵守 exclusions 与 invocation。

## 参数校验边界

项目暂不引入大型 JSON Schema runtime。第一阶段实现无副作用的通用校验子集：

- 根值必须是 object。
- `required` 字段必须存在。
- `additionalProperties: false` 时拒绝未知字段。
- 支持 object、array、string、integer、number、boolean、null 类型。
- 支持 string 的 minLength/maxLength 和 RFC3339 date-time format、array 的 minItems/maxItems、enum。
- 支持 properties、items、oneOf 中的单一可匹配分支。

未覆盖的 JSON Schema 关键字不会被伪装成已校验；dispatch 仍可在 family 内做更严格的领域校验。错误包含稳定 ID、字段路径和约束摘要，不包含 token、凭据或完整敏感参数。

## Skill 可用性

Skill 的可用状态按以下顺序计算：

1. disabled：用户或 Agent activation policy 关闭。
2. missing/invalid：缺少 routing card 或字段不合法。
3. dependency_blocked：依赖 Skill 或运行时不存在/未启用。
4. conflict/out_of_sync：inventory 已有对应状态。
5. available：可读取且已投影给目标 Agent。

只有 `available` Skill 才进入 Agent 的自动路由摘要；用户仍可在设置中查看和启用/禁用，市场 Skill 的安装与来源优先级不变。

## 一致性校验

静态检查必须覆盖：

- stable ID 非空、格式正确且唯一。
- schema -> binding 与 binding -> schema 双向无缺项。
- 每个 schema 工具都能通过 feature gate、tool family 和 dispatch 路由。
- `tools/list` 的三件网关工具与 handler dispatch 名称一致。
- routing card 的五个必需字段、类型和 240 字符预算。
- Agent projection 不丢失 core triggers、exclusions、aliases、invocation。

## 兼容与回滚

新增响应字段不改变旧 Agent 的读取方式；未知字段由现有 JSON 客户端忽略。旧 Skill 没有 routing card 时保持可手动读取和管理，但不会自动触发。若 schema 校验出现误拒绝，只能回退到同一稳定 ID 的领域错误，不允许绕过 catalog 改用内部工具名。

## 验证

遵守仓库当前规则，不运行桌面构建、Cargo 测试或 E2E。交付前执行：

- JSON schema 与 stable binding PowerShell 对账。
- routing card YAML 解析、必需字段和预算检查。
- 目标 Rust/TypeScript 格式与静态 lint（可执行时）。
- `git diff --check`。
- 逐段静态审查 `tools/list -> search -> read -> invoke -> dispatch` 及 Skill `scan -> inventory -> projection -> read` 调用链。
