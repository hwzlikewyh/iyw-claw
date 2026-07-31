# Task 08：消息渠道端到端可靠性

## 目标

修复企微、飞书、微信“配置完成但无法对话”，把连接、入站、Agent readiness、会话执行和出站回复做成可诊断闭环。

## 已确认缺陷

### IYW-CHANNEL-001：新增后未连接

`add-chat-channel-dialog.tsx` 创建 `enabled=true` 后只刷新列表。`auto_connect_channels` 只在应用启动运行一次，新渠道保持数据库已启用但运行态未连接。

### IYW-CHANNEL-002：启用开关不连接

`channel-list-tab.tsx` 从开启改关闭会 disconnect；从关闭改开启只 update DB，不 connect。

### IYW-CHANNEL-003：企微被无关 token gate 阻断

`connect_chat_channel_core`/test/auto-connect 在 backend factory 前统一调用 `get_channel_token`。企微 backend 明确忽略 token 并依赖 wecom-cli QR credential，因此永远可能在 factory 前失败。

### IYW-CHANNEL-004：编辑覆盖内部配置

编辑页通过 `buildChatChannelConfig` 新建 JSON，丢失 `channel_workspace_root` 和未来未知字段。

### IYW-CHANNEL-005：微信授权覆盖配置

微信确认仅写 `{base_url}`，同样丢失 workspace root、默认 Agent 和其他字段。

### IYW-CHANNEL-006：测试不等于可对话

test 只验证凭据/API；没有验证 dispatcher -> workspace -> Agent -> prompt -> TurnComplete -> reply。

## scope_write

- `src-tauri/src/chat_channel/`
- `src-tauri/src/commands/chat_channel.rs`
- chat channel DB service/entity/migration（新增 SQLite migration 需在 handoff 标明，由 Task 13 注册）
- `src-tauri/src/web/handlers/chat_channel.rs`
- `src/components/settings/*chat-channel*`、`channel-list-tab.tsx`、add/edit/Weixin/WeCom 相关组件
- `src/lib/chat-channel-config.ts` 和 chat channel API 类型

## 禁止修改

- ACP manager/connection、运行时 bootstrap、根 router/lib.rs。
- 为了“能聊”绕过 Agent storage、gateway、权限或工作区安全校验。

## 统一 reconcile

增加 `reconcile_channel(id, desired_enabled, reason)`：

- 读取最新 DB model 和 typed config。
- 按 channel type 判定 credential：企微查 wecom-cli auth，不查 channel token；飞书/微信查 keyring。
- desired false：幂等 stop/remove backend。
- desired true：验证配置、创建 backend、start，成功后更新 runtime status。
- config/credential 变化时安全重连；失败保留 enabled 意图并记录 last error。
- create、enable、edit、save credential、扫码完成和应用启动都调用同一入口。

数据库 `enabled` 表示期望状态，不等于已连接。API 返回 desired/runtime/readiness 分开字段。

## typed config patch

- 后端拥有内部字段 `channel_workspace_root`，前端不能提交覆盖。
- UI 更新使用字段 patch，后端读取当前 JSON 后 merge 已知用户字段。
- 未知字段原样保留；删除字段使用显式 null/operation。
- 微信扫码只 patch base URL。
- config 解析失败不回退 `{}` 覆盖，返回错误和修复提示。

## readiness 状态

每渠道维护：

- `saved`
- `credential_ready`
- `transport_connected`
- `inbound_verified`
- `workspace_ready`
- `agent_ready`
- `roundtrip_ready`

Agent readiness 检查：default/sender/folder Agent 可解析、已启用、已安装、storage active、必要工具 ready、工作区存在可写、gateway/provider config 可用。不能为检查而真的执行用户任务。

## 入站与会话可靠性

- 每条入站生成 `message_trace_id`，用 channel message ID 做幂等键。
- dispatcher queue 有界；满时给渠道返回明确 busy/稍后重试，不静默丢。
- natural router 生成/读取日工作区失败时返回修复信息。
- `/task`、自然语言自动启动、follow-up、resume 都复用 readiness 和 runtime env。
- bridge 注册、conversation 状态和 prompt 发送失败必须补偿；不能留下假 active session。
- TurnInProgress 的 pending prompt 有持久或有界恢复策略；连接终止时明确失败。

## 出站可靠性

- 不再忽略 `send_to_target` 结果；记录 provider message ID 或失败。
- 长消息分片保持顺序和 trace ID。
- TurnComplete 为空、内容被 sanitizer 全部过滤、非终端 error、terminal error 都给用户可理解结果。
- 发送失败进入有界 retry/outbox；避免重复回复，按 channel/provider message ID 幂等。
- 权限/问题卡在不支持富交互的渠道降级为明确文本命令。

## 诊断 API 与 UI

- 快速检查：凭据 + transport，不启动 Agent。
- 完整回环：发送受控探针或用模拟入站，验证直到出站；产生 `diagnostic_id`。
- 展示每阶段状态、最近成功、最近失败、错误 code、时间和修复按钮。
- “已启用但未连接”使用醒目状态，不显示为正常。
- 配置保存成功但 reconcile 失败时不关闭对话框后静默；明确显示“已保存，连接失败”。

## 测试矩阵

每个渠道至少覆盖：

- 新建 enabled 立即连接。
- 禁用/启用、编辑重连、凭据更新、应用重启自动连接。
- 企微无 channel token 但已 QR 授权成功。
- 配置 patch 保留 workspace root/default Agent/未知字段。
- 入站重复、queue 满、Agent 未装、storage 未激活、workspace 不可写、gateway 失败。
- spawn 失败、TurnInProgress、terminal error、空回复、出站 429/超时/网络断开。
- 应用重启后 active binding 恢复或明确失效，不悬挂。

## 验证

- 后端 Rust 逻辑由远端 CI 执行定向测试；本机只静态审查。
- 目标环境使用真实企微/飞书/微信账号各完成：配置 -> 连接 -> 发消息 -> Agent 回复 -> follow-up -> 重启后再聊。
- 日志能用一个 trace ID 串起全链路，且不含 token、消息全文之外的不必要个人信息。

## 完成定义

- 三种渠道配置后可立即对话。
- 所有失败都能定位到 readiness 的一个阶段。
- enabled、connected 和 roundtrip-ready 不再混为一个布尔值。
