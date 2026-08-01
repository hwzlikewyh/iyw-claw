# Task 07：新会话配置强制对账与默认能力

## 目标

Codex 和 Claude Code 每次新建会话前幂等写入并回读受控配置；多智能体协同和实时反馈对新安装默认开启，同时保留用户显式设置和后台紧急关闭。

## 已知现状

- `build_session_runtime_env` 是会话前协调入口。
- `spawn_agent_connection` 已调用 provider overlay。
- `provider_overlay_files.rs` 已有临时文件和原子替换。
- delegation 和 feedback 默认值当前均为 false。
- 现有 Agent/渠道/恢复会话可能从不同入口 spawn，需要统一收口。

## scope_write

- `src-tauri/src/acp/provider_overlay*.rs`
- 新增 `src-tauri/src/acp/session_config_reconciler/`
- delegation/feedback 配置模型和 command
- 对应设置 UI 与 i18n
- 会话配置 fingerprint/诊断的非共享状态模块

## 禁止修改

- `commands/acp.rs` 大型共享入口、`connection.rs`、`manager.rs`、`lib.rs`、router/root config；由 Task 13 接线。
- 用户记忆模块和 chat channel 模块。
- delegation companion/listener 中的记忆工具路由由 Task 09 独占；需要共享 listener 变更时提交 integration request。

## 受控配置模型

Codex：基于实际 TOML/JSON 配置适配 gateway、model、MCP、Skill 搜索路径、delegation/feedback 和受管 PATH。Claude Code：基于实际 settings/MCP 配置适配同等字段。

每个 provider adapter 声明：

- 支持的 schema/version。
- 受控字段路径。
- 用户字段保留规则。
- 凭据只通过受信环境/密钥存储注入，不写明文文件。
- 规范化、排序和 fingerprint 算法。
- 校验和错误修复说明。

服务端不得提供任意配置路径或原始配置片段，只提供已知策略值。

## reconciler 流程

1. 取得 per-session 独占锁，防两个窗口同时写同一 Agent 配置。
2. 加载现有文件；不存在则从最小模板创建。
3. 解析为结构化模型；解析失败保留原文件并返回可操作错误，不覆盖。
4. 合并后台有效策略、用户设置、当前 catalog/inventory。
5. 只改受控字段，保留用户自定义 MCP/其他配置。
6. 写同目录临时文件、sync、原子替换；Windows 重试有上限。
7. 回读、解析并逐字段比较，生成 fingerprint。
8. 把 fingerprint、来源和时间写入 session launch snapshot。
9. 任一必要字段失败，阻止新会话 spawn；UI 提供查看诊断/重试/打开设置。

## 新建与恢复

- 新建会话：每次必须 reconcile，不以“文件最近写过”跳过；无变化可避免实际重写，但必须完成回读校验并记录本次 fingerprint。
- 恢复会话：保持原记忆和策略代际；只刷新凭据、明确允许热更新的 gateway 安全字段和 security block。
- 从消息渠道创建的会话与桌面 UI 新会话走同一 gate。
- Agent probe 不写用户配置；使用隔离 probe origin。

## 默认值与迁移

有效值来源优先级：

```text
backend emergency kill switch
> backend mandatory org policy
> user explicit preference
> migrated legacy preference
> product default
```

- 新安装：delegation=true、feedback=true。
- 升级用户：存在持久键则原样保留；不存在则一次性写 true，并记录 migration version。
- 后台 kill switch 关闭时 UI 显示“由管理员关闭”，不能伪装成用户设置。
- 用户重新开启不能绕过安全 kill switch。
- 设置变更只影响新会话；现有会话显示 stale 状态。

## 诊断

每次 reconcile 记录：agent、new/resume、config schema、受控字段数量、changed、fingerprint、耗时和错误 code。禁止记录配置正文、token、key、完整用户路径。

UI 展示：

- 有效开关和来源：默认/用户/组织策略/安全关闭。
- 最近一次新会话对账时间和结果。
- 当前运行会话是否 stale。
- “重新对账”只执行 dry-run/修复，不启动 Agent。

## 测试矩阵

- 文件不存在、格式正确、格式损坏、只读、被占用、原子替换失败。
- 用户自定义字段/MCP 完整保留。
- 两会话并发 reconcile 无丢字段。
- 默认、显式 false、显式 true、无旧键迁移、kill switch。
- 新建每次校验；恢复不重复注入新代际。
- Codex/Claude Code 的 gateway/model/MCP/Skill/delegation/feedback 字段一致。

## 验证

- 使用纯解析/merge fixture 做定向测试可由远端 CI 执行；本机不运行桌面测试。
- 静态追踪所有 spawn 入口，列出尚未接入 gate 的入口给 Task 13。
- 远端 E2E 创建连续三个新会话，验证每次都有不同 launch event 和有效 fingerprint，配置内容幂等。

## 完成定义

- Codex/Claude Code 所有新会话都不能绕过 reconciler。
- 默认开启策略不会覆盖已有显式用户选择。
- 配置失败可诊断且不会以未知配置启动。

## 实施状态（2026-08-01，worktree `iyw-claw-t07` / branch `feat/managed-t07-session-config`）

- 已完成：`session_config_reconciler/`（model/lock/merge/write/diagnostics）、
  provider_overlay_files 对 Codex/Claude Code 的 reconciler gate、delegation/feedback
  有效来源与 kill switch/org policy 合并、设置 UI 来源展示与 i18n、纯解析/merge
  单元测试（model.rs / merge.rs / write.rs）。
- 待 Task 13 接线：恢复会话走 `reconcile_resumed_session` 区分热更新字段；
  对账诊断（`diagnostics_snapshot`）暴露给设置页展示最近一次对账结果；
  `session_config_stale` 会话内 banner 已由既有事件链路覆盖。
- 本机验证：静态调用链审查 + `git diff --check`；编译与单测由远端 CI 执行
  （桌面仓库禁本机构建）。
