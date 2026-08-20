# 消息渠道统一扫码接入设计

日期：2026-08-20

## 背景

当前 `iyw-claw` 已有微信 iLink 二维码授权、企业微信旧版 CLI 授权和多个可运行的
WebSocket/Stream backend，但各渠道的扫码状态、轮询生命周期和凭据落库路径不一致。
微信扫码还存在确定的长轮询超时与并发轮询问题；企业微信 AI Bot、钉钉和飞书仍要求
用户手填凭据。

本设计在保留已有数据库、keyring、消息 target 和 runtime reconcile 契约的前提下，
增加统一扫码 onboarding，并先修复已确认的微信与企业微信运行时问题。内置 MCP 的
Streamable HTTP 改造不属于本设计范围。

## 目标与非目标

### 目标

- 让微信、企业微信 AI Bot、钉钉和飞书/Lark 使用一致的扫码会话体验。
- 前端只接触二维码内容、opaque 会话 ID 和脱敏状态，不接触 provider poll token 或
  任何凭据。
- 扫码成功必须经过真实 runtime 连接验证后才报告成功。
- 同一扫码会话的轮询串行、可取消、可过期，迟到结果不能污染新会话。
- 保留现有渠道记录、token、sender context 和旧版 `wecom`/`wecom_agent` 兼容路径。

### 非目标

- 不把企微自建应用回调向导改造成一键扫码；该产品仍需要管理员配置回调。
- 不把旧版 `wecom` CLI 轮询协议宣称为 AI Bot 协议，也不绕过企业消息权限限制。
- 不在本阶段实现钉钉主动 OpenAPI 推送、飞书自动创建默认群聊或微信普通群聊支持。
- 不修改 MCP、数据库 schema 或无关消息渲染行为。

## 渠道策略

| 渠道 | 扫码协议 | 成功凭据 | 本阶段行为 |
| --- | --- | --- | --- |
| 微信 iLink | `get_bot_qrcode` / `get_qrcode_status` | `bot_token`、`baseurl` | 修复长轮询，补齐官方状态和重定向 |
| 企业微信 AI Bot | `/ai/qc/generate` / `/ai/qc/query_result` | `botid`、`secret` | 扫码后保存并连接唯一 WebSocket |
| 钉钉 Stream | `/app/registration/init` -> `begin` -> `poll` | `client_id`、`client_secret` | 扫码后进入现有 Stream backend |
| 飞书/Lark | `/oauth/v1/app/registration` | `client_id`、`client_secret` | 支持 Feishu/Lark 域名；首个入站建立 target |
| 企微旧版 `wecom` | `wecom-cli` 授权 | CLI 自有凭据 | 保留现状，单独显示兼容提示 |
| 企微自建应用 | 管理员回调配置 | corp/app/callback 凭据 | 保留现有分步向导 |

## 后端设计

### 扫码会话

新增一个进程内的 QR onboarding service。每个会话包含：

- `session_id`：随机 opaque ID，前端可见；
- `channel_id` 与 provider 类型；
- provider poll token、开始时间、过期时间和当前状态；
- 单会话互斥锁、`ACTIVE / COMMITTING / CANCELLED` 原子生命周期和一次性终态保护。

provider token 只保存在后端内存，不写数据库、日志或前端响应。进程重启会使未完成
会话失效，用户重新生成二维码即可；已落库凭据和已建立 runtime 不受影响。

### API

遵循仓库现有 POST 优先约定，新增三类共享 command/handler：

- `start_chat_channel_qr`：输入渠道类型及可选渠道 ID，返回 `session_id`、二维码数据、
  `expires_at` 和初始状态；
- `poll_chat_channel_qr`：输入 `session_id`，返回公共状态、可选脱敏错误和建议重试间隔；
- `cancel_chat_channel_qr`：输入 `session_id`，使后续迟到结果无效。

微信现有接口在迁移期间继续可用，内部复用相同的 provider adapter；不改变旧客户端
已有的 `weixin_get_qrcode`/`weixin_check_qrcode` 响应字段。

### 状态机

公共状态固定为：

`waiting -> scanned -> connecting -> connected`

终态为：`expired`、`denied`、`cancelled`、`error`。

provider 特有状态只在后端映射：

- 微信 `scaned_but_redirect` 更新轮询 host；`need_verifycode` 保持等待并返回需要
  验证码的可操作错误；`binded_redirect` 视为已有绑定，不泄露旧凭据；
- 钉钉未知中间状态继续等待并记录脱敏状态；只有完整 `client_id` 和
  `client_secret` 才算成功；
- 飞书 `authorization_pending`/`slow_down` 继续等待，`access_denied` 和过期错误
  进入终态；
- 企微 AI Bot 只有同时获得 `botid` 与 `secret` 才进入 `connecting`。

### 成功提交与回滚

扫码返回凭据后按以下顺序执行：

1. 持有 channel operation lock 后原子抢占提交权，失败时不写入任何凭据；
2. 写入 keyring；
3. 以字段 patch 更新现有 `config_json`，不重建和丢失未知字段；
4. 通过现有 reconcile 启动真实 backend；
5. 等待该 channel generation 报告 `Connected`。

任何一步失败都返回可诊断错误。keyring/config 已写入但 runtime 失败时保留凭据，
将渠道置为 `error`，允许用户重试连接，不自动删除可能仍有效的凭据。

## 渠道修复

### 微信

- QR status HTTP 客户端超时提高到至少 35 秒，以容纳官方 long-poll；
- QR 获取使用官方 POST，并传最近的 `local_token_list`；
- 前端轮询改为递归 `setTimeout`，当前请求结束后才安排下一次；
- UI/后端覆盖 `wait`、`scaned`、`confirmed`、`expired`、重定向和验证码状态；
- 二维码重定向 host 缓存设置 10 分钟 TTL 和 256 条容量上限；
- 保留现有 sender-scoped `context_token` 持久化，并继续使用 target payload 做本轮回复。

### 企业微信 AI Bot

- 增加官方扫码生成/查询 adapter；
- 运行态已有连接时，`test_connection` 不再创建第二条订阅连接抢占正式连接；
- 主动发送显式携带保存的 `chat_type`（单聊/群聊），缺失时只允许由入站 target 推断；
- 遵守单 Bot 单有效连接，旧连接被踢出时只由当前 generation 更新 runtime 状态。

### 钉钉与飞书/Lark

- 复用统一会话层保存 device code，不把其暴露给前端；
- 扫码成功后自动保存 secret 并走现有 Stream/WebSocket reconcile；
- 飞书扫码时选择 Feishu 或 Lark 域名；`chat_id` 可为空，首条真实入站消息注册 target；
- 钉钉继续只依赖入站 `session_webhook` 做即时回复，日报和主动推送保持禁用。

## 前端设计

新增通用扫码 hook/状态视图，保留现有微信弹窗的外部调用方式。hook 负责：

- 开始、串行轮询、取消和卸载清理；
- 防止重复成功回调和旧会话结果覆盖新会话；
- 将公共状态映射为二维码、处理中、成功、过期和错误视图；
- 只把成功后的非敏感配置字段回填到表单。

渠道表单在扫码模式下不强制要求可由首条入站消息产生的 `chat_id`；手工配置模式仍
保留现有必填校验和管理员提示。

## 日志与安全

每个关键阶段记录 `channel_id`、渠道类型、`session_id`（脱敏或截断）、generation、
阶段、状态和错误类别。禁止记录 QR URL、device code、secret、bot token、完整消息或
完整个人信息。外部错误先归类再写入 runtime status，避免把 provider 原始敏感内容
直接展示给用户。

## 验收与验证边界

静态验收覆盖：provider adapter、会话互斥/终态、keyring/config patch、reconcile、
前端清理、状态映射、路由注册和国际化键；额外执行 `git diff --check` 与针对性格式
检查。遵循仓库规则，本阶段默认不运行 Cargo/Tauri/pnpm 测试或构建。

真实验收按顺序记录：扫码凭据、transport 连接、入站消息、Agent 调度、即时回复、
延迟回复和重启恢复。任何未在真实渠道完成的层级都明确标为未验证。
