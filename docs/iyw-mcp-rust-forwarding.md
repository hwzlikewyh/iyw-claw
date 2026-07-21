# iyw-claw-mcp 平台 MCP 转发（中转）设计

> 状态：需求已确认（2026-07-21），原调研遗留问题已全部由需求方答复（见第 10 节），
> 可按第 9 节步骤实施。

## 1. 需求背景

iyw 平台侧（`ai-application` 服务）已经提供了一个 MCP 服务，希望 iyw-claw 把它转发给各个 agent CLI 使用。核心诉求：

- **用 `iyw-claw-mcp` 做中转**：不把上游 HTTP MCP 配置直接写进 agent 的 MCP 配置，而是复用现有的 stdio 伴生进程做代理。
- **agent 无感鉴权**：agent / LLM 不需要知道也拿不到平台 token；中转层自己从已登录的平台账号会话里取 token，带着请求上游。
- 上游是标准 MCP（streamable HTTP），中转层只做协议转发，不理解具体业务工具。

### 上游接入信息

开发环境样例（用户提供）：

```json
{
  "type": "http",
  "url": "http://127.0.0.1:5002/mcp",
  "headers": {
    "token": "Bearer 7bb53770f75a49eaa076396603f379e8"
  }
}
```

- `token` 头的值就是平台用户登录后的 access_token。已确认：按现有代码惯例发**裸 token**，
  不带 `Bearer ` 前缀（上面样例里的前缀无需保留）。
- 正式环境走网关：`https://gateway.iyw.cn`，服务名 `ai-application`，即
  `https://gateway.iyw.cn/ai-application/mcp`（网关域名与登录用的网关一致，仅路径不同）。
- 测试环境（与模型网关测试环境同源，`provider_overlay.rs:21`）：
  `http://192.168.1.86:3201/ai-application/mcp`（已确认）。

## 2. 现状盘点（调研结论）

### 2.1 iyw-claw-mcp 伴生进程

- 入口 `src-tauri/src/bin_targets/iyw_claw_mcp.rs`；重逻辑全部在
  `src-tauri/src/acp/delegation/companion.rs`（可单测，不依赖进程）。
- stdio JSON-RPC（MCP）进程，由 `inject_iyw_claw_mcp`（`acp/connection.rs:1546`）在每次
  ACP 会话建立时注入到 agent 的 `mcpServers`，携带参数：
  `--parent-connection-id / --socket-path / --token（一次性临时凭证） / --parent-pid / --features / --working-dir`。
- 当前暴露 **8 个工具**，按 `--features` 分组开关（`tool_schema.json` + `CompanionFeatures`）：
  - `delegation`：`delegate_to_agent`、`get_delegation_status`、`cancel_delegation`
  - `feedback`：`check_user_feedback`
  - `ask`：`ask_user_question`
  - `sessions`：`get_session_info`
  - `images`：`show_image`（恒开）
  - `memory`：`append_user_memory`
- 所有工具调用都通过 UDS（Windows 命名管道）回连主进程：长度前缀 JSON 帧
  （上限 16 MiB），消息为 `BrokerMessage` tagged enum（`delegation/transport.rs:200`，
  现有 9 个变体），主进程侧由 `DelegationListener`（`delegation/listener.rs`）鉴权
  （`TokenRegistry`：临时 token → 父连接/工作目录/agent 类型）并执行。
- **新增能力的既定扩展方式就是加 `BrokerMessage` 变体**（transport.rs 注释明确说明），
  最近的 `MemoryAppend` 是最好的参照样板。

### 2.2 平台账号与 token

- 登录态存 SQLite（key `iyw_account_session`，`commands/iyw_account.rs`），含
  `access_token / refresh_token / expiration`。
- `iyw_account_access_token_core(conn)` 返回 `AccountAccessToken`（`acp/account_credentials.rs`），
  是现成的"取当前登录 token"入口。
- 现有网关调用（模型列表、agent 凭据同步）用的请求头都是 `token: <裸 access_token>`
  （无 `Bearer ` 前缀，见 `iyw_account_list_models_core`、`account_credentials_formats.rs`）。
  平台 MCP 转发沿用同一写法（已确认，无需 `Bearer ` 前缀）。
- 网关 URL 三段式选择已有惯例（`provider_overlay.rs`）：
  `debug_assertions` → 本地；`test-gateway` feature → 测试；否则生产；
  另有环境变量覆盖（`IYW_CLAW_MODEL_GATEWAY_BASE_URL`）。平台 MCP URL 照搬此模式。

## 3. 方案对比

| 方案 | 做法 | 问题 |
| --- | --- | --- |
| A. 直接注入 HTTP MCP 条目 | 把 `{type:http, url, headers:{token}}` 写进 agent 的 mcpServers | token 走 ACP wire 暴露给 agent 进程、落 agent 配置盘；token 过期/换号后条目失效；HTTP MCP 依赖 agent 声明 `mcpCapabilities.http`，很多 agent 不支持（stdio 是唯一保底） |
| B. companion 直连上游 | iyw-claw-mcp 自己发 HTTP，token 经 spawn 参数或 UDS 下发 | token 离开主进程（进程参数可被 `ps` 看到 / 长会话过期后失效）；上游 MCP 会话、重试、刷新逻辑要在每个 companion 进程里各来一份 |
| **C. companion → 主进程中转（推荐）** | companion 收到 `tools/list` / `tools/call` 后经既有 UDS broker round-trip 交主进程；主进程持有唯一的上游 HTTP MCP 客户端，从 DB 取 token 附加后转发 | 上游流式/进度通知在 v1 丢失（业务工具基本是请求-响应，可接受） |

选 C 的理由：token 完全不出主进程（与现有 listener「companion 不能自选身份/路径」的
安全姿态一致）；上游会话、token 刷新、401 处理、URL 切换集中一处；桌面模式与
server 模式（`lib.rs:578` / `iyw_claw_server.rs:351` 两处 listener 构造）天然复用；
companion 二进制不需要新增任何 HTTP 依赖，保持轻量。

## 4. 总体架构

```
agent CLI ──stdio(MCP)── iyw-claw-mcp(--features platform)
                              │  UDS/命名管道, 长度前缀 JSON 帧
                              ▼
                     DelegationListener（鉴权：临时 token）
                              │
                              ▼
                    PlatformMcpService（主进程单例）
                      · 从 DB 读平台 access_token
                      · 维护上游 MCP 会话（initialize / Mcp-Session-Id）
                      · 缓存 tools/list（短 TTL）
                              │  HTTPS, header: token: <access_token>
                              ▼
              dev: http://127.0.0.1:5002/mcp
              prod: https://gateway.iyw.cn/ai-application/mcp
```

**注入形态**：注入**第二个** stdio MCP server 条目（建议名 `iyw-platform`），复用同一个
`iyw-claw-mcp` 二进制，`--features platform`。不与现有 8 个内置工具合并进同一实例，理由：

- 故障隔离：网关慢/挂只影响平台工具实例，内置工具的 `tools/list` 仍是静态秒回；
- 工具名零冲突：上游工具保持原名（平台文档怎么写，LLM 就怎么看）；
- 注入门控独立：只在「已登录 + 设置开关开」时注入，语义干净。

## 5. 详细设计

### 5.1 companion 侧（`companion.rs` + `bin_targets/iyw_claw_mcp.rs`）

- `CompanionFeatures` 增加 `platform: bool`（`--features platform`）。platform 模式下：
  - `initialize`：照常静态应答（serverInfo 名可复用，capabilities 只报 tools）。
  - `tools/list`：发 `BrokerMessage::PlatformToolsList { token }` round-trip，把主进程返回的
    工具数组原样透出。失败/超时 → 返回空 `tools: []` 并 stderr 记日志（**不报错**，
    避免个别 agent 因单个 MCP server 失败拖垮会话启动）。
  - `tools/call`：任意工具名 → `BrokerMessage::PlatformToolsCall { token, name, arguments }`，
    结果原样透出（上游返回的 MCP `content`/`structuredContent`/`isError` 不做加工）。
  - `notifications/cancelled`：复用现有 InflightCalls 机制，取消即丢弃 round-trip、
    按 MCP 规范抑制响应（v1 不向上游传播取消）。
- 单实例只开 `platform` 一个 feature；内置 8 工具在该实例全部隐藏（`allows_tool` 走
  platform 分支时仅放行转发路径）。

### 5.2 broker 协议扩展（`delegation/transport.rs`）

新增两个变体（wire 稳定，向后兼容——老 companion 不发新 kind）：

```rust
PlatformToolsList(BrokerPlatformToolsListRequest { token: String }),
PlatformToolsCall(BrokerPlatformToolsCallRequest {
    token: String,
    name: String,
    arguments: serde_json::Value,
}),
```

响应沿用 `BrokerResponse { outcome }`：list → `{ "tools": [...] }`；call → 上游
`tools/call` 的 result 对象（或 `{ "error": "..." }`，companion 转成 `isError` 工具结果）。

### 5.3 主进程：`PlatformMcpService`（新模块，建议 `acp/platform_mcp.rs`）

`DelegationListener` 新增字段 `platform: Arc<dyn PlatformMcpAccess>`（与 `session_info`
等同构，trait 便于单测），实现持有 `DatabaseConnection` + `reqwest::Client`：

- **上游会话管理**：懒初始化，进程内共享一个上游 MCP 会话
  （`initialize` → 记录响应头 `Mcp-Session-Id` → `notifications/initialized`）。
  404/会话失效 → 重新握手一次后重试；不做逐 agent 会话（上游业务工具视为无状态，
  见待确认问题 4）。
- **请求头**：`Content-Type: application/json`、`Accept: application/json, text/event-stream`、
  `token: <access_token>`（裸 token，与现有网关调用一致；每次请求现读 DB，
  保证换号/重登后立即生效）。
- **响应解析**：streamable HTTP 允许单 JSON 或 SSE 两种响应形态，都要支持
  （SSE 场景取匹配请求 id 的最后一个 `data:` 消息即可）。
- **tools/list 缓存**：短 TTL（建议 60s）+ 5s 超时。会话启动高频触发 `tools/list`，
  不能每次打网关。
- **鉴权失败**：未登录 → 返回明确错误（"平台账号未登录"）；上游 401 → 使缓存会话失效、
  返回"平台登录已过期，请在 iyw-claw 重新登录"（v1 不做 refresh_token 自动续期，
  现状 `iyw_account.rs` 也只存不刷）。
- **调用超时**：`tools/call` 默认 120s（业务工具可能内部再调 LLM）；`tools/list` 5s。

### 5.4 URL 配置（对齐 `provider_overlay.rs` 惯例）

```rust
pub const PLATFORM_MCP_LOCAL_URL: &str = "http://127.0.0.1:5002/mcp";
pub const PLATFORM_MCP_TEST_URL: &str = "http://192.168.1.86:3201/ai-application/mcp";
pub const PLATFORM_MCP_PRODUCTION_URL: &str = "https://gateway.iyw.cn/ai-application/mcp";
pub const PLATFORM_MCP_URL_ENV: &str = "IYW_CLAW_PLATFORM_MCP_URL"; // 环境变量覆盖，测试/私有化用
```

选择逻辑与 `MODEL_GATEWAY_BASE_URL` 完全一致：env 覆盖 > debug 本地 > `test-gateway`
feature 测试 > 生产。

### 5.5 注入门控（`acp/connection.rs`）

在 `inject_iyw_claw_mcp` 旁新增 `inject_iyw_platform_mcp`，条件为全部满足：

1. `agent_supports_mcp && agent_delivers_wire_mcp(agent_type)`（沿用现有两道门）；
2. 设置开关开（新增热切换 `PlatformToolsRuntimeConfig`，仿 `SessionInfoRuntimeConfig`）；
3. **平台账号已登录**（`iyw_account_access_token_core` 返回 Some）。未登录不注入——
   否则 agent 会拿到一个空/报错的 MCP server，徒增 LLM 困惑。登录后新开的会话自然生效。

注入条目：`McpServerStdio::new("iyw-platform", 同一二进制路径)`，参数同现有条目
（`--parent-connection-id/--socket-path/--token/--parent-pid/--working-dir`），
`--features platform`。临时 token 单独 mint、单独注册进 `TokenRegistry`、随连接
teardown 一并 revoke（复用 `revoke_by_parent`）。

## 6. 安全考量

- 平台 access_token 只存在于主进程内存 + 本地 DB；不进 companion 进程、不进 agent
  进程、不落 agent 配置文件、不走 ACP wire。
- companion → 主进程的调用凭证仍是每次启动新 mint 的一次性 UUID token，连接断开即吊销；
  companion 无法指定身份或改道 URL。
- 上游工具的描述与返回内容对 LLM 而言是外部数据；转发层不做内容过滤（与用户自配
  MCP server 同等信任级别），但日志里不落 token 头。

## 7. 错误与降级矩阵

| 场景 | 行为 |
| --- | --- |
| 未登录 | 不注入 `iyw-platform` 条目 |
| 会话中途登出 | 下一次 `tools/call` 返回"未登录"工具错误（`isError: true`） |
| 上游 401 | 失效缓存会话；返回"登录已过期，请重新登录" |
| 上游超时/网络错误 | `tools/list` → 空列表 + 日志；`tools/call` → 工具错误，附错误详情 |
| 上游 `Mcp-Session-Id` 失效（404） | 自动重新 `initialize` 重试一次 |
| agent 不支持 MCP wire（Pi/OpenClaw） | 沿用现有门控，不注入 |

## 8. 测试要点

- `companion.rs`：platform 模式的 `tools/list`/`tools/call` 分发、空列表降级、取消抑制
  （仿现有 `dispatch_line` 单测）。
- `transport.rs`：新变体序列化 round-trip（仿 `Call`/`SessionInfo` 现有用例）。
- `listener.rs`：token 鉴权失败、未登录、`PlatformMcpAccess` mock 的成功/失败路径。
- `platform_mcp.rs`：用本地 mock HTTP 服务测 initialize 握手、Session-Id 透传、
  JSON 与 SSE 双响应形态、401/404 重试、TTL 缓存。
- 双模式编译：三个 clippy/check 命令（tauri-runtime / server-runtime / mcp-runtime）全绿。

## 9. 实施步骤

1. `transport.rs`：新增两个 `BrokerMessage` 变体 + 序列化测试。
2. `acp/platform_mcp.rs`：上游客户端（握手/会话/缓存/双形态解析）+ trait + 单测。
3. `listener.rs`：接新变体，构造点（`lib.rs:578`、`iyw_claw_server.rs:351`）传入服务实例。
4. `companion.rs` + `iyw_claw_mcp.rs`：`platform` feature 与转发分发。
5. `connection.rs`：`inject_iyw_platform_mcp` + 登录态/开关门控。
6. 设置页开关（`PlatformToolsRuntimeConfig` + 前端 toggle，可后置，默认开）。
7. 三套 check/clippy/test 全绿，真机对 `127.0.0.1:5002` 联调。

## 10. 已确认结论（2026-07-21，需求方答复）

1. **测试环境 MCP 地址**：`http://192.168.1.86:3201/ai-application/mcp`，确认无误。
2. **上游通知/进度**：上游不会推送通知。不接 SSE 长连接（GET 流）、不处理
   `notifications/*`。POST 响应的 JSON / SSE 双形态解析仍保留——这是传输层防御
   （streamable HTTP 服务端可任选一种形态编码响应），与业务通知无关。
3. **上游会话粒度**：全进程共享一个上游 MCP 会话，确认可行，按 5.3 设计实施。
4. **工具规模**：上游控制在 10 个以内，不做白名单/分组配置。
5. **暴露范围**：与现有 8 个内置工具的写法保持一致——仅对 iyw-claw 内启动的 ACP
   会话经 wire（`session/new` 的 `mcpServers`）注入，不写入外部 CLI 的原生 mcp.json。
