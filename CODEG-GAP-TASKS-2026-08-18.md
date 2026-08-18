# Codeg 能力对齐任务清单

日期：2026-08-18
对比基线：Codeg `v0.26.1`（`ea5177ea48dd18e13f1992c070703c1fa58cf2dd`）
目标项目：iyw-claw `feat/memory-recall-m0-m2`

## 范围

本清单只记录 Codeg 已有、但 iyw-claw 当前尚未完整接入或尚未完成实证的能力。当前已完成的 13 个内置 Agent、28 个受信 Agent、DeepSeek Harness 和 AIR `sessionFailure` 不再列为待办。

明确排除：

- Skill 相关改动。
- MCP HTTP、transport、delegation 相关改动；这些属于另一条并行任务。
- 对并发脏工作树进行整文件回退、广泛暂存、提交或推送。

状态标记：`[ ]` 未完成，`[~]` 部分完成，`[x]` 已完成。

## P1：运行时 Agent 注册表

- [ ] **实现 custom Agent registry CRUD**
  - 现状：Codeg 有 `custom_registry`、数据库持久化、添加/编辑/删除 API、hydrate 和设置界面；iyw-claw 当前主要依赖编译期 `trusted_agents` 白名单。
  - 参考：Codeg `src-tauri/src/acp/custom_registry.rs`、`src-tauri/src/commands/custom_agents.rs`。
  - 需要：定义可持久化的 Agent distribution schema；提供添加、编辑、删除、校验、hydrate、启动失败回滚和前端设置入口。
  - 验收：新增 Agent 无需重新编译即可出现在列表、可安装/启动；删除后不再可启动；非法命令、参数、环境变量和路径均被拒绝。

- [ ] **让 Agent 列表由动态 catalog 驱动**
  - 现状：已存在 `project_catalog()`，但 `acp_list_agents_core()` 仍使用 `all_identity_agents()`；远端 catalog 新增 Agent 不会自动进入选择器。
  - 参考：`src-tauri/src/commands/acp.rs` 的 `acp_list_agents_core`、`src-tauri/src/acp/trusted_agents/projection.rs`。
  - 需要：将 catalog 的 active/hidden/disabled、排序、版本和本地安装状态合并为前端列表；保留本地受信定义作为启动安全闸门。
  - 验收：catalog 新增/隐藏/禁用 Agent 后，刷新列表即可反映；无受信启动定义的 Agent 只能显示为不可启动，不能被远端字段直接执行。

## P1：稳定身份与跨服务契约

- [ ] **把稳定 `platform_id` 投影到 Agent 信息和前端**
  - 现状：后端 `TrustedAgentProjection` 已有 `platform_id`，但 `AcpAgentInfo` 和主要前端选择流程仍以 `registry_id` 为主。
  - 需要：在 Rust `AcpAgentInfo`、TypeScript `AcpAgentInfo`、设置项、安装/切换请求和缓存中增加稳定 `platform_id`；`registry_id` 仅作为本地目录键。
  - 验收：同一 Agent 改名、换 package 或排序后，安装记录、模型关联和前端选择仍通过 `platform_id` 稳定关联；旧 payload 可兼容读取。

## P1：28 个受信 Agent 的运行时能力

- [ ] **完成统一 Provider/endpoint/model 环境注入**
  - 现状：28 个受信 Agent 多数 `allowed_env_names` 为空；当前 provider overlay 主要覆盖原有内置 Agent。
  - 需要：逐 Agent 建立经审查的 provider、endpoint、model、credential 环境映射；区分固定环境、用户可配置环境和主机托管环境。
  - 验收：每个 Agent 的启动环境只包含允许的键；endpoint/model 配置实际传到对应进程；未知键被过滤并记录可定位日志；密钥不进入日志、快照或错误消息。

- [ ] **补齐协议能力矩阵**
  - 现状：28 个 Agent 当前大多声明 `ACP_ONLY`（MCP/resume/load 为 false）；Cursor 支持 MCP，DeepSeek 支持完整 session。
  - 需要：按每个 Agent 的真实协议版本确认 `mcp`、`resume`、`load`、取消、权限和 elicitation 能力；将声明接入 session/new、resume/load、MCP 转发和 UI 能力闸门。
  - 验收：能力为 false 时不会发送对应字段或调用对应方法；能力为 true 时有协议级验证记录；声明与初始化响应不一致时 fail closed。

- [ ] **接入 DeepSeek 中断回合时长回填**
  - 现状：正常 `turn/end` 时长已解析；当前拆分后的 DeepSeek parser 尚未接入 Codeg 的 `backfill_turn_durations` 路径。
  - 参考：`src-tauri/src/parsers/deepseek/mod.rs`、`src-tauri/src/parsers/mod.rs`。
  - 需要：为缺失 `turn/end` 的中断回合保留真实已知时间，不伪造结束时间；仅在满足现有回填规则时补齐估算时长。
  - 验收：正常回合时长不变；进程中断、损坏日志、缺少时间戳时不产生负数或跨回合污染；列表和详情统计一致。

## P0：生产与安装实证

- [ ] **验证 Fusion catalog 与 28 个 Agent 的端到端可用性**
  - 需要分层验证：catalog 是否发布、resolve 是否返回、目标平台 artifact 是否存在、下载校验是否通过、本地安装是否入库、启动是否成功、登录/首轮 ACP 是否成功。
  - 覆盖：至少 Windows 当前目标；记录每个 Agent 的 catalog revision、版本、平台、安装状态、启动结果和失败原因。
  - 验收：不能用源码静态存在或 HTTP 200 代替成功；产出逐 Agent 结果表，明确“未发布、策略拒绝、包缺失、安装失败、启动失败、登录失败”和“已验证”。
  - 注意：生产结论必须基于已部署 Fusion、实际客户端安装和真实启动结果，不能只根据本地源码判断。

## 建议顺序

1. 先完成 custom registry 和动态 catalog，确定运行时 Agent 的身份、来源和安全边界。
2. 再落地 `platform_id`，避免后续安装、模型和设置数据继续绑定 `registry_id`。
3. 按 28 个 Agent 建立环境注入与协议能力矩阵，并补 DeepSeek 中断回合时长。
4. 最后执行 Fusion catalog、安装、启动、登录的逐 Agent 实证，回填本清单状态和证据链接。

## 当前验证边界

本清单基于 Codeg v0.26.1 与 iyw-claw 源码、调用链和定向静态检查生成。当前未运行 Cargo check、单元/集成测试、桌面构建或逐 Agent 实际安装启动；这些不能标记为已验证。
