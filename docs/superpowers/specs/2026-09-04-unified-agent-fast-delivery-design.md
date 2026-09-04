# 统一 Agent 无感快速投递设计

## 目标

为 Claude Code、Codex CLI、OpenCode、Gemini、OpenClaw、Cline、Hermes、CodeBuddy、Kimi Code、Pi、Grok 及受信任的自定义 ACP Agent 统一提供以下体验：

1. 用户提交后立即看到自己的消息，不等待进程初始化。
2. Agent 冷启动、重连或连接切换期间，Prompt 不丢失、不重复。
3. 连接就绪后自动投递，用户无需再次点击发送。
4. 进程退出、窗口切换或应用重启后，未完成 Prompt 可以恢复。
5. Agent 特有能力（原生 steering、feedback、图片、MCP、Session 恢复）继续由能力矩阵控制，不互相污染。

## 现状与问题

所有 ACP Agent 共用前端连接生命周期和 Rust `ConnectionManager`。前端目前把匹配的 `connecting` 连接视为可发送，后端则在 `send_prompt_linked` 中先将会话置为 `InProgress`，再等待启动完成并向 `ConnectionCommand` 投递 Prompt。若连接在这段窗口内被替换、移除或接收循环尚未建立，数据库会留下 `in_progress` 的空会话，而用户消息已经从输入框消失。

现有 Agent Input durable outbox 主要服务于“已有会话运行中时的后续输入”。新连接尚未绑定 `conversation_id` / `folder_id` 时，outbox worker 不能可靠消费首条消息；前端的 `sendPrompt` 在 Store 中找不到连接时还会静默返回，增加了“看似发送成功、实际没有投递”的风险。

## 方案

### 1. 连接状态分层

将“可接收 Prompt”和“进程已创建”明确分离：

- `connecting`：仅表示连接正在建立，不能直接发送。
- `connected`：Runtime Host、Session（new/resume/load）和基础 selector 初始化完成，可以投递 Prompt。
- `prompting`：已有 Prompt 正在执行，后续输入进入 Agent Input outbox。

前端 `connectionReady` 只在以下条件全部满足时为真：

- connectionId、Agent 类型和 workingDir 与当前 tab 一致；
- status 为 `connected`；
- sessionId 已存在；
- selector 初始化已完成，或当前 Agent 的协议不提供 selector。

### 2. 无感发送路径

用户点击发送时，前端立即完成乐观渲染并将 Prompt 放入 durable outbox；同时后台触发 `ensureConnected()`，不阻塞输入框和消息列表。

- 已有持久化会话：直接以 conversation_id 创建 outbox item。
- 新会话：先创建 conversation 行并绑定 tab，再创建 outbox item。
- 连接已完全就绪且 outbox 为空时，可走低延迟直发路径；该路径仍必须经过统一的后端投递确认。
- 连接处于 `connecting`、连接 ID 不存在、连接正在替换或连接刚被移除时，始终走 outbox，不走直发。

连接完成后只允许一个 flush worker 消费队首，避免直发和队列同时发送导致乱序。队列项在后端确认进入 `ConnectionCommand::Prompt` 后保持 `dispatching`，在连接循环确认收到后变为 `consumed`；通道关闭或连接重启时按投递确定性恢复为 `fallback_queued` 或 `failed`。

### 3. 后端绑定与错误闭环

对于 `acp_connect` 已知的持久化 conversation，在创建 `SessionState` 时写入经过身份校验的 `conversation_id` / `folder_id`，使连接初始化期间启动的 outbox worker 具备消费条件。新会话仍在第一次发送时创建并绑定行。

`send_prompt_linked` 调整为：

1. 校验连接、会话身份和 Prompt；
2. 确保 Prompt 已进入可靠 outbox 或已成功 reserve channel；
3. 再写入 `ConversationStatus::InProgress`；
4. 失败时回滚会话状态，并返回可分类错误。

所有连接层入口禁止静默成功：

- 前端 Store 中找不到连接时抛出明确错误；
- 后端 connection map 找不到连接时返回 `ConnectionNotFound`；
- 发送失败必须触发 outbox 重试或明确的终态，不得留下 generation 0 的 `in_progress`。

### 4. 统一恢复策略

复用现有 `agent_input_worker` 和 `recover_connection`，补齐“连接建立前首条消息”场景：

- 连接进入 `connected` 后立即唤醒对应 conversation 的 worker；
- 连接被替换或重连时恢复 `dispatching` claim；
- 应用重启后 `list_recoverable` 继续按 FIFO 恢复；
- 只有原生 steering 结果不确定时才标记 `failed`，普通 Prompt 在结果不确定时回到 `fallback_queued`，避免丢消息。

### 5. 选择性预热

- 保留 Codex 共享 Runtime Host 预热；
- 当前用户选中的 Agent 在打开工作区后后台预热；
- 不一次性启动全部 Agent，避免内存、MCP 和 SQLite 竞争；
- Hermes、Pi 等特殊 Agent 的预热失败不阻塞 UI；
- 预热只创建可复用的 Runtime Host，不创建无主 Session、不写 conversation 行。

## Agent 差异边界

统一首条 Prompt 投递，不统一 Agent 特有行为：

- Claude / Codex：保留标准 ACP steering；Claude 不启用有风险的 post-tool cancel。
- Codex：保留共享 Host、rollout migration 和图片输入转换。
- OpenCode：保留 Binary 启动和其 Session/Prompt 能力。
- Gemini / Pi / Grok：保留 deferred interrupt。
- OpenClaw：保留 session key、reset-session 和无 MCP wire 转发限制。
- Hermes：继续校验 HTTP MCP 兼容身份；MCP 不可用时按现有安全策略处理。
- Cline：继续限制历史 Session 恢复能力。
- CodeBuddy / Kimi Code：继续使用各自 provider overlay 和运行时配置。

## 日志

在不记录 Prompt 内容、Token 或完整文件内容的前提下，补充结构化阶段日志：

- `prompt_accepted`
- `prompt_queued`
- `prompt_dispatch_claimed`
- `prompt_enqueued_to_cmd_channel`
- `prompt_loop_received`
- `prompt_consumed`
- `prompt_send_failed`
- `prompt_requeued`

每条日志至少带 `connection_id`、`agent_type`、`conversation_id`、`message_id`、`turn_generation` 和结果码。

## 验收标准

1. 所有 Agent 在 `connecting` 时提交 Prompt，用户消息立即显示且最终自动执行。
2. 同一 Prompt 在连接切换、重连和应用重启后最多执行一次。
3. 任何发送失败都不会留下 generation 0 的 `in_progress` 空会话。
4. 队列按 FIFO 投递；直发不会越过已有队列。
5. 连接建立后首条 Prompt 无需用户重复点击。
6. Codex 预热速度不回退；其他 Agent 不因预热引入全局资源压力。
7. Claude 图片兼容、Hermes MCP 能力、Pi Skill 冲突等特有问题保持独立错误分类。

## 范围外

- 不修改安装目录 `D:\software\原助理` 中的二进制、配置或数据库。
- 不重写 ACP 协议适配器。
- 不把所有 Agent 强制改为共享 Runtime Host。
- 不在本次工作中清理历史会话或终止用户进程。
