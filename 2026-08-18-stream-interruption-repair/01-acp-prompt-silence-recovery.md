# 修复方案 01：ACP prompt 静默恢复

## 目标

当 prompt 已进入 Agent 但持续没有事件时，系统必须在可控时间内结束前端“生成中”状态，并能区分卡在队列、Runtime Host 转发、Agent 首事件、工具调用还是进程退出阶段。

本方案不自动重发 prompt。已有 prompt 可能执行过文件写入、外部 API、消息发送或计费，自动重发会造成重复副作用。

## 已确认现象

- 三份日志共记录 11 次 prompt 静默超时，静默时间约 600 至 629 秒。
- `iyw-claw.2026-08-18(1).log` 中连接 `d3759867-3f63-4f90-b3e1-563c2ca3240b` 在 02:10:26 和 02:37:26 被同一 watchdog 再次取消。旧安装版没有证明第一次取消已真正结束该回合。
- 日志能看到 `prompt started` 和 watchdog，但不能完整证明请求已写入 Runtime Host、Agent 是否收到、首个事件是否到达、取消是否被 Agent 确认。

## 当前代码已有能力

- `src-tauri/src/acp/idle_sweep.rs`：默认 600 秒静默阈值，每 30 秒扫描一次。
- `src-tauri/src/acp/manager.rs::sweep_stalled_prompts`：按 `last_agent_event_at` 判断静默，排除 permission、question、后台任务和活动工具调用。
- `src-tauri/src/acp/connection.rs`：已有 prompt started/completed/failed/interrupted 日志，Cancel 分支会发 `CancelNotification`、释放终端并发出 `TurnComplete(cancelled)`。
- `src-tauri/src/acp/session_state.rs::apply_event`：Agent 事件会更新 `last_agent_event_at`。

这些能力能让 UI 退出部分挂起状态，但 watchdog 当前只把命令写入 channel，没有等待取消完成，也没有在取消失败后隔离对应 Runtime Host。它是兜底，不是上游断流根因修复。

## 具体改动

### 1. 建立一次 prompt 的阶段记录

在 `SessionState` 中增加不含正文的当前回合诊断字段，使用现有 `turn_generation` 作为稳定关联键：

```rust
struct PromptAttemptState {
    turn_generation: i64,
    enqueued_at: Instant,
    runtime_dispatched_at: Option<Instant>,
    first_agent_event_at: Option<Instant>,
    cancel_requested_at: Option<Instant>,
    terminal_at: Option<Instant>,
}
```

字段只保存在内存；不记录 prompt 正文、token、文件内容或完整 payload。

修改调用链：

1. `manager.rs::send_prompt_inner` 在命令成功写入 channel 后记录 `prompt_enqueued`。
2. `connection.rs` 在创建 `PromptRequest` 并首次 poll 请求前记录 `prompt_dispatched`。
3. 会话消息读取路径在该回合第一个 Agent update 到达时只记录一次 `prompt_first_event`。
4. prompt response、Cancel、Error、Disconnect 都写入唯一 terminal 结果。
5. Runtime Host driver 退出时带上 PID、host key 前缀和受限 stderr 分类。

统一结构化字段：

```text
connection_id, conversation_id, session_id, turn_generation,
agent_type, host_pid, stage, elapsed_ms, silent_ms,
had_output, active_tool_count, terminal_reason
```

### 2. 将 watchdog 改成两阶段恢复

新增 `ConnectionManager::cancel_stalled_prompt_with_deadline`，只供 watchdog 使用：

1. 第一次达到静默阈值时，将该回合原子标记为 `cancel_pending`。
2. 发送 `ConnectionCommand::Cancel`，记录 `cancel_requested`。
3. 等待同一 `turn_generation` 的 `TurnComplete` 或状态离开 `Prompting`，默认 8 秒。
4. 8 秒内完成：记录 `cancel_acknowledged`，保留连接供下一轮使用。
5. 超时：记录 `cancel_timeout`，断开该 session route，并将承载它的 Runtime Host 标记为 quarantined，禁止新 route 复用。
6. quarantined host 没有其他活动 route 时立即 shutdown/reap；仍有其他活动 route 时等其自然结束后 shutdown，不直接终止其他正在生成的回合。

同一 `turn_generation` 只能从 `active` 进入一次 `cancel_pending`，后续 sweep 必须跳过，避免旧日志中同一连接被重复取消。

### 3. 增加 Runtime Host 隔离接口

修改：

- `src-tauri/src/acp/runtime_host.rs`
- `src-tauri/src/acp/runtime_host/lifecycle.rs`
- `src-tauri/src/acp/runtime_host_registry.rs`

新增最小接口：

```rust
quarantine(reason, connection_id, turn_generation)
```

行为约束：

- 原子设置 `healthy=false`，让 `ready_host()` 不再返回该 host。
- 不立即杀死仍承载其他活动 route 的共享 host。
- 最后一个 route 释放后执行 shutdown，并在 5 秒内 reap；超时则 abort driver。
- 新建 host 必须走既有 singleflight，避免隔离后并发拉起多个替代进程。

### 4. 给用户返回明确终态

watchdog 首次触发时发出：

```text
code = prompt_stall_timeout
terminal = false
```

取消确认或强制断开后再发唯一 terminal event，确保 conversation 状态从 `InProgress` CAS 到 `Cancelled`。不要把 timeout 标成成功，也不要伪造 Agent 回复。

## 聚焦验证

1. Prompt 600 秒无事件：只触发一次 cancel，8 秒内确认后连接保持可复用。
2. Agent 忽略 cancel：8 秒后 route 被断开，host 被隔离，新 prompt 不复用该 host。
3. 共享 host 另有活动 route：隔离不终止另一个回合，最后一个 route 结束后才回收 host。
4. pending permission/question、有效后台任务和声明超时内的前台工具调用不会被 watchdog 取消。
5. 取消期间进程先退出：只产生一个 terminal 结果，不重复改写 conversation 状态。
6. 所有日志均不包含 prompt 正文、token、文件内容和完整 stderr。

## 验收标准

- 任意静默 prompt 在“静默阈值 + 8 秒 + 一个 sweep 周期”内离开生成中状态。
- 同一 `connection_id + turn_generation` 只有一条 terminal 记录。
- 日志可计算 enqueue、dispatch、first event、terminal 四段耗时。
- 取消失败的 Runtime Host 不再接收新 route。
- 不发生 prompt 自动重发。

## 发布后验证

在新安装版连续运行至少 24 小时，按 `connection_id + turn_generation` 聚合：

- `prompt_started` 必须最终对应 completed、failed、cancelled 或 disconnected。
- `cancel_requested` 必须在 10 秒内对应 acknowledged 或 forced_disconnect。
- 不再出现同一回合被 watchdog 重复取消。
- 若仍有大量 `prompt_first_event` 缺失，再按 host PID、Agent 版本和上游网络错误继续定位，不把 watchdog 成功当成断流根因已消失。

## 风险与回滚

- 误杀风险：只在既有排除条件全部通过且静默达到阈值后执行；共享 host 隔离不得直接杀其他活动 route。
- 状态竞争风险：所有 terminal 写入使用 `turn_generation` 和 conversation CAS。
- 回滚时可先关闭强制隔离，仅保留结构化阶段日志和现有 watchdog；不要关闭当前的 600 秒 UI 兜底。
