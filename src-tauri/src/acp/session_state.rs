//! 会话级状态结构。后端权威：流式累积、in-flight tool calls、待处理 permission 等
//! 全部住在这里。Phase 2 的 snapshot 端点直接从此处读取 live 部分。

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::acp::event_stream::{ConnectionEventStream, RecentEventsBuffer};
use crate::acp::feedback::{FeedbackItem, FeedbackStatus};
use crate::acp::question::PendingQuestionState;
use crate::acp::session_failure::{SessionFailureRecord, SessionFailureTable};
use crate::acp::types::{
    AcpEvent, AvailableCommandInfo, ConfigStaleKind, ConnectionStatus, EventEnvelope,
    PromptCapabilitiesInfo, PromptInputBlock, SessionConfigKindInfo, SessionConfigOptionInfo,
    SessionModeStateInfo, ToolCallImageInfo, UserMessageBlock,
};
use crate::models::agent::AgentType;
use crate::models::message::MessageRole;

/// 当前 streaming 中的 turn 的累积内容。turn 完成后清空。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: Vec<LiveContentBlock>,
    pub started_at: DateTime<Utc>,
}

/// 流式 turn 的内容块。事件按到达顺序追加。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveContentBlock {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolCallRef {
        tool_call_id: String,
    },
    Plan {
        entries: serde_json::Value,
    },
    UserInput {
        message_id: String,
        blocks: Vec<UserMessageBlock>,
        created_at: DateTime<Utc>,
    },
}

/// 工具调用的运行态。turn 完成时统一 clear。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallState {
    pub id: String,
    pub kind: ToolKind,
    pub label: String,
    pub status: ToolCallStatus,
    pub input: Option<serde_json::Value>,
    pub output: Option<ToolCallOutput>,
    /// Latest rendered content blocks reported by the agent (markdown / text).
    /// Distinct from `output` (which is the parsed `raw_output`); kept as the
    /// most recent value (replace-on-update, not append) for snapshot fidelity.
    pub content: Option<String>,
    /// File locations affected by this tool call (e.g. paths of edits).
    /// Forwarded verbatim from the agent's ToolCall/ToolCallUpdate event.
    /// `None` if the agent didn't supply it. Partial-update preservation:
    /// an incoming `None` from a `ToolCallUpdate` (which typically carries
    /// only changed fields) must NOT clobber a previously-set value.
    pub locations: Option<serde_json::Value>,
    /// ACP extensibility metadata. Used by frontend Phase 1 parent
    /// extraction. `None` if the agent didn't supply it. Same partial-update
    /// preservation semantic as `locations`.
    ///
    /// Convention used by iyw-claw's multi-agent delegation (the `delegate_to_agent`
    /// MCP tool) — `DelegationBroker` writes the following object under
    /// `meta["iyw-claw.delegation"]` on the parent's active tool call:
    ///
    /// ```jsonc
    /// {
    ///   "child_connection_id": "<uuid>",
    ///   "child_conversation_id": <i32>,
    ///   "status": "pending" | "running" | "completed" | "failed"
    /// }
    /// ```
    ///
    /// The frontend reads this to render "Delegating to <agent>…" on the live
    /// tool-call, and to anchor the inline `<DelegatedSubThread>` to the
    /// correct child conversation.
    pub meta: Option<serde_json::Value>,
    /// Latest images attached to this tool call (e.g. codex-acp v0.14+
    /// image generation). Replace-on-update semantics matching `content`:
    /// a fresh `ToolCallUpdate` carrying `Some(images)` replaces the prior
    /// vec, `None` preserves it. Persisted on snapshot so a frontend
    /// reconnecting mid-turn or after refresh sees the same image that was
    /// streamed live. ⚠ base64 image data can be multi-MB per entry; the
    /// snapshot endpoint payload grows accordingly. This is the cost of
    /// surviving page refresh without re-fetching from JSONL.
    #[serde(default)]
    pub images: Vec<ToolCallImageInfo>,
    /// 流式拼接的 input chunks（serde 不输出，仅运行时用）
    #[serde(skip)]
    pub raw_input_chunks: Vec<String>,
    /// Monotonic runtime-only start point for the current tool call. It is not
    /// serialized because it is meaningful only inside this process.
    #[serde(skip)]
    pub(crate) started_at: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// 工具种类。沿用 ACP 协议层枚举。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    Other,
}

/// 工具调用输出。可能是文本、错误、结构化结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolCallOutput {
    Text { content: String },
    Error { message: String },
    Json { value: serde_json::Value },
}

/// 待处理的权限请求。重连后从 SessionState 恢复，跨 UI 关闭不丢。
/// 注意：与 chat_channel::PendingPermission 不同（后者有 sent_message_id）。
///
/// `tool_call` 是 agent 原样转发的 JSON——保留 rawInput / content / locations /
/// patch / plan 等所有结构，前端 `parsePermissionToolCall` 依赖它来渲染 diff、
/// shell 命令、plan 列表等审批必备信息。压成 `description: String` 那种摘要
/// 字符串会让"刷新后继续审批"变成"盲签"。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPermissionState {
    pub request_id: String,
    pub tool_call_id: String,
    pub tool_call: serde_json::Value,
    pub options: Vec<crate::acp::types::PermissionOptionInfo>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub queued: u32,
}

/// 上下文 / 模型用量。
/// Snapshot of the most recent `AcpEvent::Error`. Carried on
/// `SessionState` so post-mortem readers (e.g. the delegation-settings
/// probe) can surface the agent's own error after the connection task
/// has already cleaned up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionLastError {
    pub message: String,
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentTitleCandidate {
    pub event_seq: u64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageInfo {
    pub used: u64,
    pub size: u64,
}

/// Snapshot-recoverable record of an IN-FLIGHT (running) sub-agent delegation,
/// keyed (in `SessionState.active_delegations`) by the parent's
/// `parent_tool_use_id`.
///
/// This is the live "currently delegating" SET, not a history log:
/// `DelegationStarted` inserts an entry; `DelegationCompleted` REMOVES it. So
/// its size tracks live concurrency (bounded by what the machine actually runs)
/// — there is no cap and no cumulative growth over the parent connection's
/// lifetime.
///
/// Completed delegations are recovered without this field: a live page keeps the
/// binding in `DelegationProvider` for its lifetime, and a cold load / refresh
/// rebuilds `meta["iyw-claw.delegation"]` (status + child id) from the child's
/// persisted DB row via `commands::conversations::inject_delegation_meta`
/// (authoritative, uncapped). The snapshot only has to recover the *running*
/// binding, which the transient `DelegationStarted` event cannot supply on the
/// snapshot attach path (cold attach, lagged re-attach, refresh) — that gap is
/// exactly what this field closes.
///
/// UNLIKE `active_tool_calls`, entries are NOT cleared on `TurnComplete`: an
/// async delegation's child runs in the background long after the parent's
/// `delegate_to_agent` tool call returns and the parent turn completes. The
/// broker emits `DelegationStarted`/`DelegationCompleted` only for a REAL
/// (non-synthetic) `parent_tool_use_id`, so synthetic-fallback cards never
/// create a phantom entry here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveDelegationState {
    pub parent_tool_use_id: String,
    pub child_connection_id: String,
    pub child_conversation_id: i32,
    pub agent_type: AgentType,
}

/// The in-flight user prompt for the current turn. Captured from
/// `AcpEvent::UserMessage` into `SessionState.pending_user_message` and carried
/// on `to_snapshot()` so a client attaching mid-turn can render the user turn
/// even though the one-shot `UserMessage` event won't replay for it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingUserMessage {
    pub message_id: String,
    pub blocks: Vec<crate::acp::types::UserMessageBlock>,
}

/// Codex ACP accepted a native steer by ending the current prompt and starting
/// a wrapper-owned background turn. The host adopts that turn only after the
/// old generation's ordered lifecycle settlement has completed.
#[derive(Debug, Clone)]
pub(crate) struct NativeBackgroundTurn {
    pub message_id: String,
    pub blocks: Vec<UserMessageBlock>,
    pub source_generation: i64,
    pub adopted_generation: Option<i64>,
    pub terminal_status: Option<String>,
}

/// Captured by the `TurnComplete` arm of [`SessionState::apply_event`] for
/// the memory harvest hook (Task 13): the completed turn's nonce (read before
/// `MemoryTurnTracker::complete_turn` clears the active bit) and sanitized,
/// bounded text references. Consumed by the lifecycle worker, then cleared.
#[derive(Debug, Clone)]
pub struct TurnHarvestCapture {
    pub turn_nonce: u64,
    pub user_input_ref: Option<String>,
    pub assistant_input_ref: Option<String>,
    pub stop_reason: String,
}

/// CAS 基线随 ACP `SessionStarted` 转换滚动保存。
///
/// 一个连接可能因为 fork 多次收到 `SessionStarted`；订阅者通常晚于
/// ACP 状态更新，不能再用连接启动时的 external id 推断当前事件的期望值。
#[derive(Debug, Clone)]
pub struct SessionStartedTransition {
    pub event_seq: u64,
    pub expected_external_id: Option<String>,
    pub session_id: String,
}

/// 后端权威的会话状态。每个 AgentConnection 持有一个 Arc<RwLock<SessionState>>。
///
/// 字段范围：仅当前 turn 的 in-flight 数据 + 元信息 + 协商出的能力。
/// 已完成的 turn 不存在这里——它们由 parser 从 agent JSONL 读。
#[derive(Debug)]
pub struct SessionState {
    // 身份
    pub connection_id: String,
    pub conversation_id: Option<i32>,
    /// External session id requested when this connection was spawned.
    /// Immutable for the connection lifetime and used as the expected value
    /// when persisting a later `SessionStarted` event.
    pub requested_external_id: Option<String>,
    pub external_id: Option<String>,
    pub external_id_changed_at: Option<std::time::SystemTime>,
    pub(crate) session_started_transitions: VecDeque<SessionStartedTransition>,
    /// Agent 最近一次上报的原生标题。事件序号用于绑定补写时排除更晚到达的候选。
    pub(crate) agent_title_candidate: Option<AgentTitleCandidate>,
    pub agent_type: AgentType,
    pub working_dir: Option<PathBuf>,
    pub owner_window_label: String,
    pub folder_id: Option<i32>,

    // 状态
    pub status: ConnectionStatus,
    pub live_message: Option<LiveMessage>,
    pub active_tool_calls: BTreeMap<String, ToolCallState>,
    pub pending_permission: Option<PendingPermissionState>,
    /// AIR failure records retained for the connection lifetime. Resolved
    /// entries remain as revision watermarks so stale events cannot reopen.
    pub session_failures: SessionFailureTable,

    /// The agent's in-flight `ask_user_question` (one set of multiple-choice
    /// questions awaiting the user's answer). Set by `QuestionRequest`, cleared
    /// by a matching `QuestionResolved` (and defensively on `TurnComplete` /
    /// `UserMessage`). Carried on `to_snapshot()` so a client attaching mid-turn
    /// re-renders the interactive card the one-shot event won't replay for it.
    /// At most one is pending at a time (the agent is blocked in the tool call);
    /// the backend's `pending_questions` registry keys the answer one-shot.
    pub pending_question: Option<PendingQuestionState>,
    pub pending_channel_confirmation:
        Option<crate::acp::channel_tools::confirmation::PendingChannelConfirmationState>,

    /// In-flight (running) sub-agent delegations keyed by `parent_tool_use_id`.
    /// `DelegationStarted` inserts; `DelegationCompleted` removes. UNLIKE
    /// `active_tool_calls`, NOT cleared on `TurnComplete` (an async delegation
    /// outlives the parent turn). Carried on `to_snapshot()` so a web/server
    /// attach on the snapshot path (cold attach, lagged re-attach, refresh) can
    /// recover the running parent↔child binding the transient `DelegationStarted`
    /// event can't supply there. Size tracks live concurrency — no cap, no
    /// cumulative growth; completed delegations are recovered from the child's
    /// persisted DB row, not from here. See `ActiveDelegationState`.
    pub active_delegations: BTreeMap<String, ActiveDelegationState>,

    /// Live user-feedback ("steering") notes for the current turn. Appended by
    /// `FeedbackSubmitted` (a user note while the agent works), flipped to
    /// `Delivered` by `FeedbackConsumed` (the agent read them via the
    /// `check_user_feedback` MCP tool), and cleared on the next turn's
    /// `UserMessage` (notes are turn-scoped steering, not durable history).
    /// Carried on `to_snapshot()` so a client attaching mid-turn renders the
    /// pending notes the one-shot `FeedbackSubmitted` event won't replay for it.
    /// Size is human-bounded (one entry per note the user types this turn).
    pub feedback: Vec<FeedbackItem>,

    /// Durable host-owned inputs associated with this conversation. The DB is
    /// authoritative; this live projection is updated by `AgentInputChanged`
    /// so snapshots and attached windows do not need to poll between events.
    pub agent_inputs: Vec<crate::acp::AgentInputItem>,

    /// Launched but unresolved Claude background tasks mirrored from the
    /// transcript watcher. A recent watcher heartbeat keeps the CLI alive.
    pub background_outstanding: u32,
    pub background_activity_at: Option<DateTime<Utc>>,

    // ACP 协商出的能力
    pub modes: Option<SessionModeStateInfo>,
    pub current_mode: Option<String>,
    pub config_options: Option<Vec<SessionConfigOptionInfo>>,
    /// Model name currently selected in the agent's `model` config option.
    /// Extracted from `SessionConfigOptions` whenever it fires (the agent
    /// broadcasts the full option set on every config change, so this always
    /// reflects the latest selection). `None` when the agent advertises no
    /// `model` option and no launch preference is available.
    pub current_model: Option<String>,
    /// Managed package version captured at launch. Internal telemetry only;
    /// never serialized into the live session contract.
    pub(crate) managed_agent_version: Option<String>,
    /// Correlates request validation, Host startup, session setup, and the
    /// first prompt without exposing any launch environment values.
    pub(crate) startup_trace: Option<crate::acp::startup_trace::StartupTrace>,
    /// Hermes-native memory state captured from the effective launch profile.
    /// Contains no provider name, config content, or credential material.
    pub(crate) hermes_memory: crate::context_governor::HermesNativeMemoryDiagnostics,
    pub(crate) grok_effort_specs: Option<crate::acp::grok::EffortSpecs>,
    pub prompt_capabilities: Option<PromptCapabilitiesInfo>,
    pub fork_supported: bool,
    pub available_commands: Vec<AvailableCommandInfo>,
    pub usage: Option<UsageInfo>,
    /// True once the agent's initial selectors handshake (modes +
    /// config_options) has finished and `SelectorsReady` has fired. Persisted
    /// on the snapshot so a frontend that reconnects after refresh can see
    /// "init complete" without waiting for an event that already fired.
    pub selectors_ready: bool,
    /// Wakes `wait_for_session_options` the instant `selectors_ready` flips
    /// true. Created with `notify_waiters()` inside the `apply_event` write
    /// lock so no notification is lost: callers must create their `notified()`
    /// future WHILE HOLDING the state read lock (mirrors `launch_ready`).
    pub(crate) selectors_ready_notify: Arc<tokio::sync::Notify>,

    /// Most recent `AcpEvent::Error` payload, or `None` if no error has
    /// landed since the connection started. The probe path reads this
    /// after `wait_for_session_options` errors so it can fold the
    /// agent's own error message into the returned `AcpError` instead
    /// of surfacing a generic "connection not found" once the
    /// connection task has cleaned up its map entry.
    ///
    /// Not exposed on `to_snapshot()` today — chat-side error UX already
    /// flows through the live `AcpEvent::Error` channel.
    pub last_error: Option<SessionLastError>,

    /// Single-fire signal that fires when `SessionStarted` applies (i.e.
    /// `external_id` transitioned from None → Some). `ConnectionManager::
    /// spawn_agent` holds the per-(agent, working_dir, session_id) dedup
    /// lock until this fires (or times out), so a concurrent acp_connect
    /// for the same logical session sees the populated `external_id` and
    /// reuses instead of spawning a duplicate. `Some` immediately after
    /// `install_session_started_signal()`; `take()`'d in `apply_event::
    /// SessionStarted`; `None` thereafter (the signal is one-shot per
    /// connection). Lives only on the in-memory `SessionState`; not
    /// transmitted on the wire (`LiveSessionSnapshot` doesn't include it).
    pub(crate) session_started_tx: Option<tokio::sync::oneshot::Sender<()>>,

    // 事件锚点
    pub event_seq: u64,
    pub last_activity_at: DateTime<Utc>,
    /// Last time an event was actually applied to this session. Unlike
    /// `last_activity_at`, this is NOT refreshed by frontend keepalive
    /// touches, so it is the signal the prompt-stall watchdog uses to detect
    /// a hung generation (upstream stream stalled → no events → the UI would
    /// spin "生成中" forever without intervention).
    pub last_agent_event_at: DateTime<Utc>,

    /// Launcher PID for this connection's ACP process tree. Runtime-only;
    /// process inspection uses it to calculate private memory without guessing
    /// from executable names.
    pub(crate) agent_pid: Option<u32>,
    /// Whether the agent advertised a recoverable session operation during
    /// initialization. Unknown stays false so idle eviction is conservative.
    pub recoverable_session: bool,
    /// Set after a resume/load failure proves the advertised capability unusable.
    pub recovery_failed: bool,
    /// Shared counter for ACP terminal processes that have not exited yet.
    /// A terminal may outlive the turn that created it, so tool-call state alone
    /// is not sufficient to decide whether this connection is reclaimable.
    pub(crate) active_terminal_count: Arc<AtomicUsize>,
    /// Short lease refreshed only by a currently visible conversation surface.
    /// It naturally expires after a renderer crash or navigation away.
    pub(crate) visible_lease_until: Option<DateTime<Utc>>,
    /// Short lease for a client-side draft or local outbound queue. The payload
    /// never leaves the renderer; only this boolean lease reaches the backend.
    pub(crate) pending_input_lease_until: Option<DateTime<Utc>>,

    /// Per-connection event broadcaster used by the WS attach protocol.
    /// New subscribers register receivers here while holding the SessionState
    /// read lock; `emit_with_state` broadcasts after releasing the write
    /// lock. Wrapped in `Arc` so subscriber tasks can hold a reference
    /// independent of the SessionState lock.
    pub(crate) event_stream: Arc<ConnectionEventStream>,

    /// Bounded ring buffer of recent envelopes (most-recent-last). Pushed
    /// by `emit_with_state` inside the write-lock critical section, kept in
    /// strict lockstep with `event_seq`. Read by attach handlers under the
    /// read lock to decide between sending a snapshot or a batched replay.
    /// See `event_stream` module for size limits.
    pub(crate) recent_events: RecentEventsBuffer,

    /// Shared with the launch token entry so candidate proposals can only use
    /// host-owned provenance from the currently accepted turn.
    pub memory_turn_tracker: Arc<crate::acp::memory_turn::MemoryTurnTracker>,

    /// Launch-time user-memory snapshot. It is never serialized into the live
    /// session snapshot; only the connection loop can place its rendered
    /// envelope on the first accepted wire prompt.
    pub user_memory_context: crate::user_memory::UserMemoryContextSnapshot,
    /// Immutable launch-time capability vector exposed through live snapshots.
    /// It remains `not_evaluated` until built-in MCP readiness and injection
    /// decision have completed.
    pub user_memory_capabilities: crate::user_memory::UserMemoryCapabilities,
    /// Prompt senders wait for this flag so no turn can capture the provisional
    /// pre-probe context while the connection is still initializing.
    pub launch_finalized: bool,
    pub(crate) launch_ready: Arc<tokio::sync::Notify>,
    /// Set atomically with the first accepted prompt enqueue. A live connection
    /// keeps its launch snapshot and never reinjects it on later turns.
    pub user_context_injected: bool,

    /// Whether the `check_user_feedback` MCP tool was exposed to THIS agent at
    /// launch (the `feedback` feature was on when built-in MCP was injected).
    /// Fixed for the connection's lifetime — tool exposure can't change after
    /// launch. The authoritative gate for both the submit path and the UI: a
    /// session started before the feature was enabled has no tool, so notes
    /// would strand; one started after has it. Carried on `to_snapshot()` so the
    /// frontend gates the feedback bar on the agent's actual capability, not the
    /// (possibly later-toggled) global setting.
    pub feedback_tool_available: bool,
    /// Whether this connection advertised a native steering extension with a
    /// consumption acknowledgement understood by the locked Agent adapter.
    pub native_steering_available: bool,
    /// Pending or adopted wrapper-owned turn created by `_session/steering`.
    /// Runtime-only: reconnect cannot prove the result of an interrupted RPC.
    pub(crate) native_background_turn: Option<NativeBackgroundTurn>,
    pub(crate) native_background_notify: Arc<tokio::sync::Notify>,

    /// Concatenated text content of the just-completed turn's assistant
    /// message. Captured at TurnComplete (just before live_message is
    /// cleared) so the lifecycle subscriber can surface it as the
    /// `delegation_call_id`-bound child outcome. Cleared on the next prompt.
    pub last_assistant_text: Option<String>,
    /// Completed-turn harvest capture consumed by the lifecycle worker (Task 13).
    pub last_completed_turn_harvest: Option<TurnHarvestCapture>,

    /// The in-flight user prompt for the current turn, captured from
    /// `AcpEvent::UserMessage` and cleared on `TurnComplete` (alongside
    /// `live_message`). Carried on `to_snapshot()` so a client attaching
    /// mid-turn renders the user turn even though no `UserMessage` event will
    /// replay for it. `None` outside an active turn.
    pub pending_user_message: Option<PendingUserMessage>,

    /// Backend wall-clock instant the in-flight turn started, captured alongside
    /// `pending_user_message` from `AcpEvent::UserMessage` and cleared on
    /// `TurnComplete`. The detail endpoint uses it to tell the in-flight prompt
    /// — persisted at/after this instant by the agent CLI, a local subprocess
    /// sharing this machine's clock — apart from a prior identical prompt
    /// persisted during an earlier turn (see `apply_in_flight_message_id`). Not
    /// serialized: backend-internal, like `turn_in_flight`. `None` outside an
    /// active turn.
    pub pending_user_message_started_at: Option<DateTime<Utc>>,

    /// True between a prompt being accepted (enqueued to the connection loop)
    /// and that turn completing. Set by the manager BEFORE the enqueue (so it
    /// is guaranteed set before the loop can dequeue) and cleared on
    /// `TurnComplete`. The manager rejects a second prompt with
    /// `AcpError::TurnInProgress` while this is set — otherwise the second
    /// `Prompt` would queue behind the active turn and be silently dropped by
    /// the loop's in-turn command handler (`_ => {}`), with the caller still
    /// seeing success. Not serialized: it is a connection-loop liveness flag,
    /// not part of the client-visible snapshot.
    pub turn_in_flight: bool,
    /// Host-generated active-turn identity. ACP itself has no portable turn id,
    /// so accepted prompts increment this generation before enqueue.
    pub turn_generation: i64,
    /// Bounded, content-free shadow telemetry for the active turn.
    pub(crate) context_plan_receipt: Option<crate::context_governor::ContextPlanReceiptSeed>,
    /// True after `TurnComplete` applies until the lifecycle worker finishes
    /// ordered DB settlement for that generation. New prompts wait so an old
    /// `PendingReview` write cannot land after the next turn's `InProgress`.
    pub turn_completion_pending: bool,
    /// Wakes the durable input worker on turn/tool/input lifecycle changes.
    pub(crate) agent_input_notify: Arc<tokio::sync::Notify>,
    pub last_turn_ended_abnormally: bool,

    /// True when the agent's effective settings changed after this connection
    /// was spawned — the running process is still on its launch-time config and
    /// needs a restart to pick up the change. Set/cleared by
    /// `AcpEvent::SessionConfigStale` (emitted from
    /// `ConnectionManager::refresh_connection_staleness` after a settings save).
    /// Carried on `to_snapshot()` so a client attaching via the snapshot path
    /// (web reconnect, window refresh, a newly-tiled panel) sees the staleness
    /// the transient event won't replay for it.
    pub config_stale: bool,
    /// Which settings surface drifted, for the banner's wording. `Some` iff
    /// `config_stale`; reset to `None` when staleness clears.
    pub config_stale_kind: Option<ConfigStaleKind>,
}

impl SessionState {
    pub(crate) fn set_agent_pid(&mut self, pid: u32) {
        self.agent_pid = Some(pid);
    }

    pub(crate) fn set_recovery_capability(&mut self, advertised: bool) {
        self.recoverable_session = advertised;
        self.recovery_failed = false;
    }

    pub(crate) fn mark_recovery_failed(&mut self) {
        self.recovery_failed = true;
    }

    pub(crate) fn mark_recovery_succeeded(&mut self) {
        self.recovery_failed = false;
    }

    pub fn mark_launch_finalized(&mut self) {
        self.launch_finalized = true;
        self.launch_ready.notify_one();
    }

    pub(crate) fn mark_user_context_already_present(&mut self) {
        self.user_context_injected = true;
    }

    pub(crate) fn begin_context_plan_receipt(
        &mut self,
        turn_nonce: u64,
        hermes_shared_home_connections: Option<u16>,
    ) {
        let input = crate::context_governor::ContextPlanStart {
            connection_id: &self.connection_id,
            conversation_id: self.conversation_id,
            workspace: self.working_dir.as_deref(),
            turn_generation: self.turn_generation,
            turn_nonce,
            agent_type: self.agent_type,
            managed_agent_version: self.managed_agent_version.as_deref(),
            hermes_memory: self.hermes_memory,
            hermes_shared_home_connections,
            memory: &self.user_memory_context,
            context_loaded: self.user_context_injected
                || self.user_memory_context.rendered.is_some(),
        };
        self.context_plan_receipt = crate::context_governor::start_context_plan(input);
    }

    fn finish_context_plan_receipt(&mut self, stop_reason: &str) {
        let memory_calls = self.memory_turn_tracker.finish_turn();
        let Some(seed) = self.context_plan_receipt.take() else {
            return;
        };
        crate::context_governor::finish_context_plan(
            seed,
            crate::context_governor::ContextPlanFinish {
                stop_reason,
                memory_calls,
            },
        );
    }

    pub fn new(
        connection_id: String,
        agent_type: AgentType,
        working_dir: Option<PathBuf>,
        owner_window_label: String,
        folder_id: Option<i32>,
    ) -> Self {
        Self {
            connection_id,
            conversation_id: None,
            requested_external_id: None,
            external_id: None,
            external_id_changed_at: None,
            session_started_transitions: VecDeque::new(),
            agent_title_candidate: None,
            agent_type,
            working_dir,
            owner_window_label,
            folder_id,
            status: ConnectionStatus::Connecting,
            live_message: None,
            active_tool_calls: BTreeMap::new(),
            pending_permission: None,
            session_failures: SessionFailureTable::default(),
            pending_question: None,
            pending_channel_confirmation: None,
            active_delegations: BTreeMap::new(),
            feedback: Vec::new(),
            agent_inputs: Vec::new(),
            background_outstanding: 0,
            background_activity_at: None,
            modes: None,
            current_mode: None,
            config_options: None,
            current_model: None,
            managed_agent_version: None,
            startup_trace: None,
            hermes_memory: Default::default(),
            grok_effort_specs: None,
            prompt_capabilities: None,
            fork_supported: false,
            available_commands: Vec::new(),
            usage: None,
            selectors_ready: false,
            selectors_ready_notify: Arc::new(tokio::sync::Notify::new()),
            last_error: None,
            session_started_tx: None,
            event_seq: 0,
            last_activity_at: Utc::now(),
            last_agent_event_at: Utc::now(),
            agent_pid: None,
            recoverable_session: false,
            recovery_failed: false,
            active_terminal_count: Arc::new(AtomicUsize::new(0)),
            visible_lease_until: None,
            pending_input_lease_until: None,
            event_stream: Arc::new(ConnectionEventStream::new()),
            recent_events: RecentEventsBuffer::new(),
            memory_turn_tracker: Arc::new(crate::acp::memory_turn::MemoryTurnTracker::default()),
            user_memory_context: crate::user_memory::UserMemoryContextSnapshot::pending(
                crate::user_memory::UserMemoryOrigin::Root,
            ),
            user_memory_capabilities: Default::default(),
            launch_finalized: false,
            launch_ready: Arc::new(tokio::sync::Notify::new()),
            user_context_injected: false,
            feedback_tool_available: false,
            native_steering_available: false,
            native_background_turn: None,
            native_background_notify: Arc::new(tokio::sync::Notify::new()),
            last_assistant_text: None,
            last_completed_turn_harvest: None,
            pending_user_message: None,
            pending_user_message_started_at: None,
            turn_in_flight: false,
            turn_generation: 0,
            context_plan_receipt: None,
            turn_completion_pending: false,
            agent_input_notify: Arc::new(tokio::sync::Notify::new()),
            last_turn_ended_abnormally: false,
            config_stale: false,
            config_stale_kind: None,
        }
    }

    /// Clone the broadcaster handle so attach handlers and subscriber tasks
    /// can hold an independent reference. Cheap (Arc clone).
    pub fn event_stream(&self) -> Arc<ConnectionEventStream> {
        Arc::clone(&self.event_stream)
    }

    /// Return events buffered after `since_seq`, or `None` if the cursor is
    /// older than what the ring buffer holds (caller must fall back to a
    /// snapshot). See `RecentEventsBuffer::range_after`.
    pub fn recent_events_after(&self, since_seq: u64) -> Option<Vec<Arc<EventEnvelope>>> {
        self.recent_events.range_after(since_seq)
    }

    /// Push an envelope into the ring buffer. Must be called under the
    /// write lock from `emit_with_state`, immediately after `event_seq`
    /// is incremented, so the buffer's tail seq matches `event_seq`.
    ///
    /// Returns the eviction count (events dropped from the buffer's head to
    /// stay within count/byte caps, plus any wholesale clear triggered by an
    /// oversized event). Caller propagates this into the
    /// `EventBusMetrics::ring_buffer_evict_count` counter.
    #[must_use = "evicted count feeds the ring_buffer_evict_count metric"]
    pub(crate) fn push_recent_event(&mut self, envelope: Arc<EventEnvelope>) -> usize {
        self.recent_events.push(envelope)
    }

    /// Install a one-shot signal that fires when `SessionStarted` applies.
    /// Returns the receiver; caller (typically `spawn_agent_connection`)
    /// passes it back to the dedup waiter in `spawn_agent`. Calling this
    /// more than once on the same state replaces the previous sender,
    /// silently dropping it — the contract is "exactly one install per
    /// connection lifetime" and that's what `spawn_agent_connection` does.
    pub fn install_session_started_signal(&mut self) -> tokio::sync::oneshot::Receiver<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.session_started_tx = Some(tx);
        rx
    }

    pub fn session_started_transition(&self, event_seq: u64) -> Option<&SessionStartedTransition> {
        self.session_started_transitions
            .iter()
            .find(|transition| transition.event_seq == event_seq)
    }

    pub fn latest_session_started_transition(&self) -> Option<&SessionStartedTransition> {
        self.session_started_transitions.back()
    }

    /// 单一分发器：把一个 AcpEvent 应用到 self。注意此方法**不**自增 event_seq——
    /// seq 由 emit_with_state 在外层管理（这样 apply_event 可独立单元测试）。
    pub fn apply_event(&mut self, payload: &AcpEvent) {
        match payload {
            AcpEvent::SessionStarted { session_id } => {
                let expected_external_id = self
                    .external_id
                    .clone()
                    .or_else(|| self.requested_external_id.clone());
                self.session_started_transitions
                    .push_back(SessionStartedTransition {
                        event_seq: self.event_seq.saturating_add(1),
                        expected_external_id,
                        session_id: session_id.clone(),
                    });
                const MAX_SESSION_STARTED_TRANSITIONS: usize = 64;
                while self.session_started_transitions.len() > MAX_SESSION_STARTED_TRANSITIONS {
                    self.session_started_transitions.pop_front();
                }
                if self.external_id.as_deref() != Some(session_id.as_str()) {
                    self.external_id_changed_at = Some(std::time::SystemTime::now());
                }
                self.external_id = Some(session_id.clone());
                self.status = ConnectionStatus::Connected;
                // Fire the dedup waiter (if any). Take()-and-send is
                // single-shot: a duplicate SessionStarted (replay, agent
                // re-init) finds None here and is a no-op, which is
                // exactly the desired idempotent behavior. send returns
                // Err only when the receiver dropped (timeout already
                // fired in spawn_agent) — also a no-op.
                if let Some(tx) = self.session_started_tx.take() {
                    let _ = tx.send(());
                }
            }
            AcpEvent::SessionTitleUpdated { title } => {
                self.agent_title_candidate = Some(AgentTitleCandidate {
                    event_seq: self.event_seq.saturating_add(1),
                    title: title.clone(),
                });
            }
            AcpEvent::StatusChanged { status } => {
                if matches!(status, ConnectionStatus::Prompting) {
                    self.last_error = None;
                }
                if matches!(
                    status,
                    ConnectionStatus::Disconnected | ConnectionStatus::Error
                ) {
                    let reason = match status {
                        ConnectionStatus::Disconnected => "connection_disconnected",
                        _ => "connection_error",
                    };
                    self.finish_context_plan_receipt(reason);
                    self.agent_input_notify.notify_one();
                }
                self.status = status.clone();
            }
            AcpEvent::SessionModes { modes } => {
                self.current_mode = Some(modes.current_mode_id.clone());
                self.modes = Some(modes.clone());
            }
            AcpEvent::ModeChanged { mode_id } => {
                self.current_mode = Some(mode_id.clone());
                // Keep `modes.current_mode_id` consistent with the latched
                // `current_mode`. Snapshot consumers read `modes.current_mode_id`
                // directly (the frontend's `denormalizeSnapshot` does not look
                // at the separate `current_mode` field), so without this sync
                // a session that has switched modes would hydrate post-refresh
                // showing the original default — even though the live event
                // stream has long since corrected it.
                if let Some(modes) = self.modes.as_mut() {
                    modes.current_mode_id = mode_id.clone();
                }
            }
            AcpEvent::SessionConfigOptions { config_options } => {
                if let Some(model) = extract_model_from_config_options(config_options) {
                    self.current_model = Some(model);
                }
                self.config_options = Some(config_options.clone());
            }
            AcpEvent::SessionConfigStale { stale, kind } => {
                self.config_stale = *stale;
                self.config_stale_kind = if *stale { Some(*kind) } else { None };
            }
            AcpEvent::PromptCapabilities {
                prompt_capabilities,
            } => {
                self.prompt_capabilities = Some(prompt_capabilities.clone());
            }
            AcpEvent::ForkSupported { supported } => {
                self.fork_supported = *supported;
            }
            AcpEvent::AvailableCommands { commands } => {
                self.available_commands = commands.clone();
            }
            AcpEvent::UsageUpdate { used, size } => {
                self.usage = Some(UsageInfo {
                    used: *used,
                    size: *size,
                });
            }
            AcpEvent::ContentDelta { text } => {
                self.session_failures.settle_retry_incidents();
                self.append_text_delta(text);
            }
            AcpEvent::Thinking { text } => {
                self.session_failures.settle_retry_incidents();
                self.append_thinking_delta(text);
            }
            AcpEvent::ToolCall {
                tool_call_id,
                title,
                kind,
                status,
                content,
                raw_input,
                raw_output,
                locations,
                meta,
                images,
            } => {
                self.session_failures.settle_retry_incidents();
                self.upsert_tool_call(
                    tool_call_id,
                    Some(kind),
                    Some(title),
                    Some(status),
                    content.as_deref(),
                    raw_input.as_deref(),
                    raw_output.as_deref(),
                    locations.as_ref(),
                    meta.as_ref(),
                    images.as_deref(),
                );
                // Anchor the tool call in `live_message.content` so snapshot
                // reload preserves position relative to surrounding text /
                // thinking blocks. Idempotent by id: a second ToolCall (or a
                // ToolCallUpdate, see below) for the same id must not push a
                // duplicate ref. Mirrors text/thinking deltas in lazily
                // creating `live_message` if absent.
                self.push_tool_call_ref_if_absent(tool_call_id);
                self.agent_input_notify.notify_one();
            }
            AcpEvent::ToolCallUpdate {
                tool_call_id,
                title,
                status,
                content,
                raw_input,
                raw_output,
                locations,
                meta,
                images,
                ..
            } => {
                self.upsert_tool_call(
                    tool_call_id,
                    None,
                    title.as_deref(),
                    status.as_deref(),
                    content.as_deref(),
                    raw_input.as_deref(),
                    raw_output.as_deref(),
                    locations.as_ref(),
                    meta.as_ref(),
                    images.as_deref(),
                );
                // Defensive: if a ToolCallUpdate arrives before its initial
                // ToolCall (unusual ordering / replay), ensure the ref block
                // still gets anchored. Idempotent so the normal-flow case is
                // a no-op here.
                self.push_tool_call_ref_if_absent(tool_call_id);
                self.agent_input_notify.notify_one();
            }
            AcpEvent::PermissionRequest {
                request_id,
                tool_call,
                options,
                queued,
            } => {
                let tc_id = extract_tool_call_id(tool_call);
                self.pending_permission = Some(PendingPermissionState {
                    request_id: request_id.clone(),
                    tool_call_id: tc_id,
                    tool_call: tool_call.clone(),
                    options: options.clone(),
                    created_at: Utc::now(),
                    queued: *queued,
                });
            }
            AcpEvent::PermissionQueueDepth { depth } => {
                if let Some(pending) = self.pending_permission.as_mut() {
                    pending.queued = *depth;
                }
            }
            AcpEvent::PermissionResolved { request_id } => {
                // Drop the snapshot's pending_permission iff the resolved
                // request matches the current one. Without the id check, a
                // late-arriving resolved event for an already-replaced
                // request could wipe the live dialog out from under the
                // user.
                if matches!(
                    &self.pending_permission,
                    Some(p) if p.request_id == *request_id,
                ) {
                    self.pending_permission = None;
                }
            }
            AcpEvent::QuestionRequest {
                question_id,
                questions,
            } => {
                self.pending_question = Some(PendingQuestionState {
                    question_id: question_id.clone(),
                    questions: questions.clone(),
                    created_at: Utc::now(),
                });
            }
            AcpEvent::QuestionResolved { question_id } => {
                // Mirror `PermissionResolved`: only clear when the resolved id
                // matches the current one, so a late event for an already-
                // replaced question can't wipe a live card from under the user.
                if matches!(
                    &self.pending_question,
                    Some(p) if p.question_id == *question_id,
                ) {
                    self.pending_question = None;
                }
            }
            AcpEvent::ChannelConfirmationRequested { confirmation } => {
                self.pending_channel_confirmation = Some(confirmation.clone());
            }
            AcpEvent::ChannelConfirmationResolved { confirmation_id } => {
                if matches!(
                    &self.pending_channel_confirmation,
                    Some(value) if value.confirmation_id == *confirmation_id,
                ) {
                    self.pending_channel_confirmation = None;
                }
            }
            AcpEvent::TurnComplete { stop_reason, .. } => {
                if stop_reason == "end_turn" {
                    self.session_failures.settle_warnings();
                }
                self.turn_completion_pending = true;
                self.agent_inputs
                    .retain(|input| input.status != crate::acp::AgentInputStatus::Consumed);
                // Capture the completed turn for the memory harvest hook
                // (Task 13) BEFORE the tracker clears its active bit and the
                // in-flight user/assistant text is dropped below.
                self.last_completed_turn_harvest =
                    self.memory_turn_tracker
                        .active_nonce()
                        .map(|turn_nonce| TurnHarvestCapture {
                            turn_nonce,
                            user_input_ref: self.pending_user_message.as_ref().and_then(
                                |pending| {
                                    let mut text = String::new();
                                    for block in &pending.blocks {
                                        if let UserMessageBlock::Text { text: block_text } = block {
                                            text.push_str(block_text);
                                            text.push(' ');
                                        }
                                    }
                                    crate::user_memory::harvest_reference(&text)
                                },
                            ),
                            assistant_input_ref: None,
                            stop_reason: stop_reason.clone(),
                        });
                self.finish_context_plan_receipt(stop_reason);
                self.last_turn_ended_abnormally = stop_reason != "end_turn";
                // Snapshot the just-finished turn's FINAL assistant text — what
                // `get_delegation_status` returns as the child result. We take
                // the Text blocks that follow the LAST tool call (the agent's
                // concluding answer), skipping any trailing Thinking/Plan blocks:
                // a `PlanUpdate` is always re-appended at the end of content, so a
                // trailing-only scan would wrongly drop the answer sitting before
                // it. No tool calls → all the turn's text. A turn ending on a tool
                // call (no concluding text) → empty, which CLEARS the field so a
                // prior turn's text can't leak as this turn's result; the LLM
                // reads the full result by opening the child session instead.
                if let Some(live) = self.live_message.as_ref() {
                    let after_last_tool_call = live
                        .content
                        .iter()
                        .rposition(|b| matches!(b, LiveContentBlock::ToolCallRef { .. }))
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let assembled: String = live.content[after_last_tool_call..]
                        .iter()
                        .filter_map(|b| match b {
                            LiveContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<&str>>()
                        .join("");
                    self.last_assistant_text = if assembled.trim().is_empty() {
                        None
                    } else {
                        Some(assembled)
                    };
                }
                if let Some(capture) = self.last_completed_turn_harvest.as_mut() {
                    capture.assistant_input_ref = self
                        .last_assistant_text
                        .as_deref()
                        .and_then(crate::user_memory::harvest_reference);
                }
                self.live_message = None;
                self.active_tool_calls.clear();
                // The turn's user prompt is no longer "in flight" — the
                // assistant reply is done and the transcript is the source of
                // truth. Clear it so a post-turn snapshot doesn't carry a stale
                // pending user message into a fresh attach.
                self.pending_user_message = None;
                self.pending_user_message_started_at = None;
                // Turn finished: release the concurrency gate so the next prompt
                // is accepted. (All connection-alive turn endings — normal,
                // cancel, stop-reason — emit TurnComplete; disconnect/error
                // discard the state entirely, so no stale flag can outlive them.)
                self.turn_in_flight = false;
                if self
                    .native_background_turn
                    .as_ref()
                    .and_then(|turn| turn.adopted_generation)
                    == Some(self.turn_generation)
                {
                    self.native_background_turn = None;
                    self.native_background_notify.notify_waiters();
                }
                // NOTE: `active_delegations` is intentionally NOT cleared here.
                // A running delegation's child runs in the background long after
                // the parent's `delegate_to_agent` tool call returns and this
                // turn completes; clearing it would drop the running binding from
                // the snapshot the instant the parent turn ends (the original
                // web-only bug). It's removed per-entry by `DelegationCompleted`.
                self.pending_permission = None;
                // A blocked `ask_user_question` can't outlive its turn: if the
                // turn ends (cancel / stop) the card is moot. The backend's
                // answer one-shot is cleaned via the listener's peer-close race;
                // this just keeps the snapshot honest.
                self.pending_question = None;
                self.pending_channel_confirmation = None;
                self.status = ConnectionStatus::Connected;
                self.agent_input_notify.notify_one();
            }
            AcpEvent::UserMessage { message_id, blocks } => {
                // Starting a new prompt acknowledges prior failures. Keep the
                // records as revision watermarks; a continuing incident must
                // re-arm itself with a higher revision.
                self.session_failures.settle_all();
                // Capture the in-flight user prompt so a client attaching
                // mid-turn renders the user turn from the snapshot (the
                // one-shot event won't replay for it). Cleared on TurnComplete.
                self.pending_user_message = Some(PendingUserMessage {
                    message_id: message_id.clone(),
                    blocks: blocks.clone(),
                });
                if self
                    .native_background_turn
                    .as_ref()
                    .is_some_and(|turn| turn.message_id == *message_id)
                {
                    self.agent_inputs.retain(|item| item.id != *message_id);
                }
                // Reference instant for the in-flight prompt's recency check in
                // `apply_in_flight_message_id`. Set here (not at manager enqueue)
                // so it tracks `pending_user_message` exactly.
                self.pending_user_message_started_at = Some(Utc::now());
                // Live-feedback notes are turn-scoped steering: a new user turn
                // starts with a clean slate. The previous turn's notes (read or
                // not) are history at this point; the frontend's "agent didn't
                // read your note → resend" fallback already had its post-turn
                // window before this next prompt arrives.
                self.feedback.clear();
                // A new user turn supersedes any stale pending question.
                self.pending_question = None;
                self.pending_channel_confirmation = None;
                self.agent_input_notify.notify_one();
            }
            AcpEvent::AgentInputChanged { item } => {
                if item.status == crate::acp::AgentInputStatus::Consumed
                    && matches!(
                        item.strategy,
                        Some(crate::acp::AgentInputStrategy::CooperativeFeedback)
                            | Some(crate::acp::AgentInputStrategy::NativeSteer)
                    )
                {
                    let belongs_to_background_turn = self
                        .native_background_turn
                        .as_ref()
                        .is_some_and(|turn| turn.message_id == item.id);
                    if !belongs_to_background_turn {
                        self.push_consumed_agent_input(item);
                    }
                }
                let remove_from_projection = item.status == crate::acp::AgentInputStatus::Deleted
                    || (item.status == crate::acp::AgentInputStatus::Consumed
                        && item.strategy == Some(crate::acp::AgentInputStrategy::DeferredNext));
                if remove_from_projection {
                    self.agent_inputs.retain(|existing| existing.id != item.id);
                } else if let Some(existing) = self
                    .agent_inputs
                    .iter_mut()
                    .find(|existing| existing.id == item.id)
                {
                    *existing = item.clone();
                } else {
                    self.agent_inputs.push(item.clone());
                }
                self.agent_inputs.sort_by(|left, right| {
                    left.sort_index
                        .cmp(&right.sort_index)
                        .then_with(|| left.created_at.cmp(&right.created_at))
                        .then_with(|| left.id.cmp(&right.id))
                });
                self.agent_input_notify.notify_one();
            }
            AcpEvent::ConversationLinked {
                conversation_id,
                folder_id,
                ..
            } => {
                self.conversation_id = Some(*conversation_id);
                self.folder_id = Some(*folder_id);
            }
            AcpEvent::PlanUpdate { entries } => {
                // Replace any existing Plan block, then append at end.
                // Mirrors the frontend's PLAN_UPDATE reducer semantic: there
                // is at most one plan block, always at the current end of
                // content. `Vec<PlanEntryInfo>` is converted to
                // `serde_json::Value` because the wire-side `Plan` variant
                // stores it opaquely (frontend casts back to PlanEntryInfo[]).
                let live = self.ensure_live_message();
                live.content
                    .retain(|b| !matches!(b, LiveContentBlock::Plan { .. }));
                live.content.push(LiveContentBlock::Plan {
                    entries: serde_json::to_value(entries).unwrap_or(serde_json::Value::Null),
                });
            }
            AcpEvent::ConversationStatusChanged { .. } => {
                // No-op on purpose. Conversation row `status` is row-level
                // metadata persisted by the lifecycle subscriber / send_prompt
                // path, not in-flight session state — snapshot consumers read
                // status via the conversation list endpoints, not via
                // `LiveSessionSnapshot`. Listed explicitly (rather than swept
                // up by the catchall) so the no-op is intentional and grep-able.
            }
            AcpEvent::SelectorsReady => {
                // Latches once. Snapshot exposes this so a fresh frontend (e.g.
                // after browser refresh) can tell the initial handshake is
                // already done — the event fires only once per connection.
                self.selectors_ready = true;
                // Wake any `wait_for_session_options` callers. This is called
                // from `emit_with_state` while holding the SessionState write
                // lock, so callers who created their `notified()` future while
                // holding the read lock cannot miss the wakeup.
                self.selectors_ready_notify.notify_waiters();
            }
            AcpEvent::Error {
                message,
                code,
                details,
                terminal,
                ..
            } => {
                if *terminal {
                    self.finish_context_plan_receipt("terminal_error");
                }
                // Capture so post-mortem readers (probe path, debug
                // snapshots) can surface the agent's own error message
                // after the connection task has cleaned up its map
                // entry. The same payload is independently emitted
                // through the event channel for live chat-side UX.
                self.last_error = Some(SessionLastError {
                    message: message.clone(),
                    code: code.clone(),
                    details: details.clone(),
                });
            }
            AcpEvent::DelegationStarted {
                parent_tool_use_id,
                child_connection_id,
                child_conversation_id,
                agent_type,
                ..
            } => {
                // Record the running delegation so the binding is snapshot-
                // recoverable (survives this connection's TurnComplete and any
                // re-attach on the snapshot path). The broker only emits this for
                // a REAL (non-synthetic) parent_tool_use_id, so synthetic-fallback
                // cards never create a phantom entry here — they rely on the
                // parent tool output (see DelegatedSubThread's ack fallback).
                self.active_delegations.insert(
                    parent_tool_use_id.clone(),
                    ActiveDelegationState {
                        parent_tool_use_id: parent_tool_use_id.clone(),
                        child_connection_id: child_connection_id.clone(),
                        child_conversation_id: *child_conversation_id,
                        agent_type: *agent_type,
                    },
                );
            }
            AcpEvent::DelegationCompleted {
                parent_tool_use_id, ..
            } => {
                // A running delegation finished: drop it from the live set. Its
                // terminal status/result reaches the LLM via
                // `get_delegation_status` and the UI via the live
                // `DelegationCompleted` event (DelegationProvider) or, on a cold
                // load, the child's persisted DB row (`inject_delegation_meta`).
                // Retaining it would turn this map into an unbounded history log;
                // it is deliberately only the in-flight set.
                self.active_delegations.remove(parent_tool_use_id);
            }
            AcpEvent::FeedbackSubmitted { item } => {
                // Idempotent by id (replay / double-attach safe): append only if
                // this note isn't already tracked. The authoritative append is
                // here so snapshot replay reconstructs the same list the live
                // node holds.
                if !self.feedback.iter().any(|f| f.id == item.id) {
                    self.feedback.push(item.clone());
                }
            }
            AcpEvent::FeedbackConsumed { ids, delivered_at } => {
                // Flip the named pending notes to Delivered. Idempotent: an id
                // already Delivered (the emitting node marked it directly under
                // the write lock; this re-apply is for replay/attach nodes) is
                // skipped. Order-independent and safe to apply more than once.
                for f in self.feedback.iter_mut() {
                    if f.status == FeedbackStatus::Pending && ids.contains(&f.id) {
                        f.status = FeedbackStatus::Delivered;
                        f.delivered_at = Some(*delivered_at);
                    }
                }
            }
            AcpEvent::FeedbackWithdrawn { ids } => {
                self.feedback.retain(|item| !ids.contains(&item.id));
            }
            AcpEvent::BackgroundActivity { outstanding, .. } => {
                self.background_outstanding = *outstanding;
                self.background_activity_at = Some(Utc::now());
            }
            AcpEvent::SessionFailure { record } => {
                self.session_failures.upsert(record.clone());
            }
            AcpEvent::ClaudeSdkMessage { .. }
            | AcpEvent::SessionLoadFailed { .. }
            | AcpEvent::UserPromptSent { .. } => {
                // 这些事件不直接修改 SessionState 的可见字段。
                // UserPromptSent 是纯通知事件，仅供 chat-channel 推送消费。
            }
        }
        self.last_activity_at = Utc::now();
        self.last_agent_event_at = Utc::now();
    }

    pub fn has_active_background_work(&self, now: DateTime<Utc>) -> bool {
        if self.background_outstanding == 0 {
            return false;
        }
        self.background_activity_at
            .map(|at| now.signed_duration_since(at) < background_keepalive_max_age())
            .unwrap_or(false)
    }

    /// A single-line "what the sub-agent is doing right now" hint, used by the
    /// delegation broker so `get_delegation_status` can prove a running child is
    /// genuinely making progress instead of returning a bare "Running.".
    ///
    /// Reads the still-streaming `live_message` — unlike `last_assistant_text`,
    /// which is only snapshotted at `TurnComplete` and so is empty/stale while a
    /// turn is in flight. Preference order, each reduced to one trimmed line
    /// capped at `max_chars` chars (char-based → never splits a UTF-8 codepoint;
    /// an `…` marks truncation):
    ///
    /// 1. the answer-in-progress — `Text` after the last `ToolCallRef`, mirroring
    ///    the `TurnComplete` answer extraction;
    /// 2. else the latest `Thinking` block (`thinking: …`);
    /// 3. else the most recent tool call's label (`running tool: …`).
    ///
    /// `None` when the turn hasn't produced anything renderable yet.
    pub fn latest_live_reply(&self, max_chars: usize) -> Option<String> {
        let live = self.live_message.as_ref()?;

        // (1) Answer-in-progress: the `Text` after the last tool call.
        //
        // Consecutive text deltas merge into a single block (see
        // `append_text_delta`), so this is almost always ONE block — borrow it
        // and take its last non-empty line without copying a potentially large
        // streaming answer on every poll (this runs under the `SessionState`
        // read lock on the `get_delegation_status` path). Only when the answer
        // is split across multiple `Text` blocks (a `Thinking` block interleaved
        // mid-answer) do we stitch them, which is rare.
        let after_last_tool_call = live
            .content
            .iter()
            .rposition(|b| matches!(b, LiveContentBlock::ToolCallRef { .. }))
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut texts = live.content[after_last_tool_call..]
            .iter()
            .filter_map(|b| match b {
                LiveContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            });
        match (texts.next(), texts.next()) {
            (None, _) => {}
            (Some(only), None) => {
                if let Some(line) = last_nonempty_line(only) {
                    return Some(truncate_one_line(line, max_chars));
                }
            }
            (Some(first), Some(second)) => {
                let mut joined = String::with_capacity(first.len() + second.len());
                joined.push_str(first);
                joined.push_str(second);
                for rest in texts {
                    joined.push_str(rest);
                }
                if let Some(line) = last_nonempty_line(&joined) {
                    return Some(truncate_one_line(line, max_chars));
                }
            }
        }

        // (2) Latest thinking block — the agent is reasoning, not silent.
        if let Some(line) = live
            .content
            .iter()
            .rev()
            .find_map(|b| match b {
                LiveContentBlock::Thinking { text } => Some(text.as_str()),
                _ => None,
            })
            .and_then(last_nonempty_line)
        {
            return Some(format!("thinking: {}", truncate_one_line(line, max_chars)));
        }

        // (3) Most recent tool call's label — work is happening in a tool.
        if let Some(label) = live
            .content
            .iter()
            .rev()
            .find_map(|b| match b {
                LiveContentBlock::ToolCallRef { tool_call_id } => Some(tool_call_id.as_str()),
                _ => None,
            })
            .and_then(|id| self.active_tool_calls.get(id))
            .map(|tc| tc.label.trim())
            .filter(|l| !l.is_empty())
        {
            return Some(format!(
                "running tool: {}",
                truncate_one_line(label, max_chars)
            ));
        }

        None
    }

    /// Lazily initialize `self.live_message` and return a mutable reference
    /// to it. Centralizes the "create-if-absent" pattern shared by the
    /// text/thinking delta appenders, the tool-call ref pusher, and the
    /// plan-update applier.
    fn ensure_live_message(&mut self) -> &mut LiveMessage {
        if self.live_message.is_none() {
            self.live_message = Some(LiveMessage {
                id: format!("live-{}", uuid::Uuid::new_v4()),
                role: MessageRole::Assistant,
                content: Vec::new(),
                started_at: Utc::now(),
            });
        }
        self.live_message
            .as_mut()
            .expect("live_message just initialized")
    }

    fn append_text_delta(&mut self, text: &str) {
        let live = self.ensure_live_message();
        if let Some(LiveContentBlock::Text { text: existing }) = live.content.last_mut() {
            existing.push_str(text);
        } else {
            live.content.push(LiveContentBlock::Text {
                text: text.to_string(),
            });
        }
    }

    fn append_thinking_delta(&mut self, text: &str) {
        let live = self.ensure_live_message();
        if let Some(LiveContentBlock::Thinking { text: existing }) = live.content.last_mut() {
            existing.push_str(text);
        } else {
            live.content.push(LiveContentBlock::Thinking {
                text: text.to_string(),
            });
        }
    }

    fn push_consumed_agent_input(&mut self, item: &crate::acp::AgentInputItem) {
        let live = self.ensure_live_message();
        if live.content.iter().any(|block| {
            matches!(
                block,
                LiveContentBlock::UserInput { message_id, .. } if message_id == &item.id
            )
        }) {
            return;
        }
        let mut blocks: Vec<UserMessageBlock> = item
            .payload
            .blocks
            .iter()
            .filter_map(|block| match block {
                PromptInputBlock::Image {
                    data,
                    mime_type,
                    uri,
                    ..
                } => Some(UserMessageBlock::Image {
                    data: data.clone(),
                    mime_type: mime_type.clone(),
                    uri: uri.clone(),
                }),
                _ => None,
            })
            .collect();
        if !item.payload.display_text.trim().is_empty() {
            blocks.push(UserMessageBlock::Text {
                text: item.payload.display_text.clone(),
            });
        }
        live.content.push(LiveContentBlock::UserInput {
            message_id: item.id.clone(),
            blocks,
            created_at: item.consumed_at.unwrap_or(item.created_at),
        });
    }

    /// Push a `ToolCallRef` block onto `live_message.content` for the given
    /// tool-call id, but only if no existing block in `content` already
    /// references that id. Called by both `ToolCall` and `ToolCallUpdate`
    /// arms so a tool's position survives any event-ordering edge case
    /// without ever duplicating.
    fn push_tool_call_ref_if_absent(&mut self, tool_call_id: &str) {
        let live = self.ensure_live_message();
        let already_present = live.content.iter().any(|b| {
            matches!(
                b,
                LiveContentBlock::ToolCallRef { tool_call_id: id } if id == tool_call_id
            )
        });
        if !already_present {
            live.content.push(LiveContentBlock::ToolCallRef {
                tool_call_id: tool_call_id.to_string(),
            });
        }
    }

    /// Insert-or-update a tool call entry. Used by both `ToolCall` (initial) and
    /// `ToolCallUpdate` events. `kind` is `Some` only on the initial event;
    /// title/status/content/raw_input/raw_output/locations/meta are merged
    /// when present. Partial-update preservation: a `None` value passed in
    /// from a `ToolCallUpdate` (which typically carries only the fields that
    /// changed) must NOT clobber a previously-set value on the entry.
    #[allow(clippy::too_many_arguments)]
    fn upsert_tool_call(
        &mut self,
        id: &str,
        kind: Option<&str>,
        title: Option<&str>,
        status: Option<&str>,
        content: Option<&str>,
        raw_input: Option<&str>,
        raw_output: Option<&str>,
        locations: Option<&serde_json::Value>,
        meta: Option<&serde_json::Value>,
        images: Option<&[ToolCallImageInfo]>,
    ) {
        let entry = self
            .active_tool_calls
            .entry(id.to_string())
            .or_insert_with(|| ToolCallState {
                id: id.to_string(),
                kind: ToolKind::Other,
                label: String::new(),
                status: ToolCallStatus::Pending,
                input: None,
                output: None,
                content: None,
                locations: None,
                meta: None,
                images: Vec::new(),
                raw_input_chunks: Vec::new(),
                started_at: None,
            });
        if let Some(s) = status {
            entry.status = parse_tool_call_status(s);
        }
        if entry.started_at.is_none()
            && matches!(
                &entry.status,
                ToolCallStatus::Pending | ToolCallStatus::InProgress
            )
        {
            entry.started_at = Some(std::time::Instant::now());
        }
        if let Some(k) = kind {
            entry.kind = parse_tool_kind(k);
        }
        if let Some(t) = title {
            entry.label = t.to_string();
        }
        if let Some(c) = content {
            entry.content = Some(c.to_string());
        }
        if let Some(chunk) = raw_input {
            entry.raw_input_chunks.push(chunk.to_string());
            // 后端目前发送的是已序列化的 JSON 文本（完整或正在累积）。
            // 对最新片段做尽力解析；解析失败则尝试拼接历史片段。
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(chunk) {
                entry.input = Some(value);
            } else if let Ok(value) =
                serde_json::from_str::<serde_json::Value>(&entry.raw_input_chunks.join(""))
            {
                entry.input = Some(value);
            }
        }
        if let Some(text) = raw_output {
            entry.output = Some(parse_tool_call_output_text(text));
        }
        if let Some(loc) = locations {
            entry.locations = Some(loc.clone());
        }
        if let Some(m) = meta {
            entry.meta = Some(m.clone());
        }
        if let Some(imgs) = images {
            // Replace-on-update: the agent re-sends the full image list on
            // every ToolCallUpdate that carries content (see
            // extract_tool_call_images in connection.rs). Absent images
            // (None at the AcpEvent layer) preserve the prior vec.
            entry.images = imgs.to_vec();
        }
    }

    /// 拷贝出对外可见的 wire-friendly snapshot。Phase 2 snapshot 端点直接调用此方法。
    pub fn to_snapshot(&self) -> LiveSessionSnapshot {
        LiveSessionSnapshot {
            connection_id: self.connection_id.clone(),
            conversation_id: self.conversation_id,
            folder_id: self.folder_id,
            status: self.status.clone(),
            external_id: self.external_id.clone(),
            live_message: self.live_message.clone(),
            active_tool_calls: self.active_tool_calls.values().cloned().collect(),
            pending_permission: self.pending_permission.clone(),
            pending_question: self.pending_question.clone(),
            pending_channel_confirmation: self.pending_channel_confirmation.clone(),
            pending_user_message: self.pending_user_message.clone(),
            active_delegations: self.active_delegations.values().cloned().collect(),
            feedback: self.feedback.clone(),
            agent_inputs: self
                .agent_inputs
                .iter()
                .map(crate::acp::AgentInputItem::client_projection)
                .collect(),
            background_outstanding: self.background_outstanding,
            feedback_tool_available: self.feedback_tool_available,
            user_memory_capabilities: self.user_memory_capabilities.clone(),
            modes: self.modes.clone(),
            current_mode: self.current_mode.clone(),
            config_options: self.config_options.clone(),
            prompt_capabilities: self.prompt_capabilities.clone(),
            usage: self.usage.clone(),
            fork_supported: self.fork_supported,
            available_commands: self.available_commands.clone(),
            selectors_ready: self.selectors_ready,
            config_stale: self.config_stale,
            config_stale_kind: self.config_stale_kind,
            last_error: self.last_error.clone(),
            session_failures: self.session_failures.snapshot(),
            event_seq: self.event_seq,
            turn_generation: self.turn_generation,
        }
    }
}

pub(crate) fn background_keepalive_max_age() -> chrono::Duration {
    static SECS: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    let secs = *SECS.get_or_init(|| {
        [
            "IYW_CLAW_ACP_BACKGROUND_KEEPALIVE_MAX_SECS",
            "CODEG_ACP_BACKGROUND_KEEPALIVE_MAX_SECS",
        ]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .unwrap_or(3600)
    });
    chrono::Duration::seconds(secs)
}

/// `to_snapshot()` 的输出——前端可消费的 wire shape。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSessionSnapshot {
    pub connection_id: String,
    pub conversation_id: Option<i32>,
    pub folder_id: Option<i32>,
    pub status: ConnectionStatus,
    pub external_id: Option<String>,
    pub live_message: Option<LiveMessage>,
    pub active_tool_calls: Vec<ToolCallState>,
    pub pending_permission: Option<PendingPermissionState>,
    /// The agent's in-flight `ask_user_question` (see
    /// `SessionState.pending_question`). `#[serde(default)]` so older payloads
    /// deserialize; `skip_serializing_if` keeps the common no-question case off
    /// the wire so every snapshot stays byte-identical with the pre-feature shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_question: Option<PendingQuestionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_channel_confirmation:
        Option<crate::acp::channel_tools::confirmation::PendingChannelConfirmationState>,
    /// The in-flight user prompt for the current turn (see
    /// `SessionState.pending_user_message`). `#[serde(default)]` so older
    /// payloads still deserialize; `skip_serializing_if` so the no-pending case
    /// keeps the wire shape byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_user_message: Option<PendingUserMessage>,
    /// Running sub-agent delegations recoverable from the snapshot (see
    /// `SessionState.active_delegations`). `#[serde(default)]` so older server
    /// payloads without this field still deserialize; `skip_serializing_if` so
    /// the common no-delegation case keeps the wire shape byte-identical and
    /// doesn't bloat every snapshot with an empty array.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_delegations: Vec<ActiveDelegationState>,
    /// Live user-feedback notes for the current turn (see `SessionState.feedback`).
    /// `#[serde(default)]` so older server payloads without this field still
    /// deserialize; `skip_serializing_if` keeps the common empty case off the
    /// wire so every snapshot stays byte-identical with the pre-feature shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback: Vec<FeedbackItem>,
    /// Durable waiting/consumed inputs projected from `agent_input_outbox`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_inputs: Vec<crate::acp::AgentInputItem>,
    #[serde(default, skip_serializing_if = "u32_is_zero")]
    pub background_outstanding: u32,
    /// Whether this agent has the `check_user_feedback` tool (see
    /// `SessionState.feedback_tool_available`). `#[serde(default)]` so older
    /// payloads deserialize to `false`; the frontend gates the feedback bar on
    /// it. Always serialized (a plain bool) so the frontend can rely on it.
    #[serde(default)]
    pub feedback_tool_available: bool,
    /// Host turn identity used to reject stale consumption acknowledgements.
    #[serde(default)]
    pub turn_generation: i64,
    #[serde(default)]
    pub user_memory_capabilities: crate::user_memory::UserMemoryCapabilities,
    pub modes: Option<SessionModeStateInfo>,
    pub current_mode: Option<String>,
    pub config_options: Option<Vec<SessionConfigOptionInfo>>,
    pub prompt_capabilities: Option<PromptCapabilitiesInfo>,
    pub usage: Option<UsageInfo>,
    pub fork_supported: bool,
    pub available_commands: Vec<AvailableCommandInfo>,
    pub selectors_ready: bool,
    /// Whether the running session is on stale (launch-time) config after a
    /// later settings save (see `SessionState.config_stale`). `#[serde(default)]`
    /// so older server payloads without the field deserialize to `false`; always
    /// serialized so the frontend can rely on it from the snapshot path.
    #[serde(default)]
    pub config_stale: bool,
    /// Which settings surface drifted (see `SessionState.config_stale_kind`).
    /// `#[serde(default)]` + `skip_serializing_if` keep the common not-stale case
    /// byte-identical with the pre-feature wire shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_stale_kind: Option<ConfigStaleKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<SessionLastError>,
    /// Full AIR table, including resolved entries used as revision watermarks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub session_failures: Vec<SessionFailureRecord>,
    pub event_seq: u64,
}

fn u32_is_zero(value: &u32) -> bool {
    *value == 0
}

/// Last non-empty line of `s`, trimmed. `None` if every line is blank.
fn last_nonempty_line(s: &str) -> Option<&str> {
    s.lines().map(str::trim).rev().find(|l| !l.is_empty())
}

/// Cap `line` at `max_chars` characters, appending `…` when truncated. Operates
/// on `char`s so multi-byte text never splits mid-codepoint. Expects an
/// already single, trimmed line (see [`last_nonempty_line`]). Single-pass: takes
/// at most `max_chars + 1` chars total, so a huge (e.g. MB) input line never
/// triggers a second full scan to decide whether to mark truncation.
fn truncate_one_line(line: &str, max_chars: usize) -> String {
    let mut chars = line.chars();
    let mut out: String = (&mut chars).take(max_chars).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

/// Extract the currently-selected model name from a `SessionConfigOptions`
/// payload. Finds the option whose `id` is `"model"` and returns its
/// `Select.current_value`. Returns `None` when no such option exists (e.g.
/// agents that don't advertise a model selector, or the slice is empty).
fn extract_model_from_config_options(options: &[SessionConfigOptionInfo]) -> Option<String> {
    options.iter().find(|o| o.id == "model").and_then(|o| {
        let SessionConfigKindInfo::Select(select) = &o.kind;
        let v = select.current_value.trim();
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    })
}

fn parse_tool_kind(s: &str) -> ToolKind {
    match s {
        "read" => ToolKind::Read,
        "edit" => ToolKind::Edit,
        "delete" => ToolKind::Delete,
        "move" => ToolKind::Move,
        "search" => ToolKind::Search,
        "execute" => ToolKind::Execute,
        "think" => ToolKind::Think,
        "fetch" => ToolKind::Fetch,
        _ => ToolKind::Other,
    }
}

fn parse_tool_call_status(s: &str) -> ToolCallStatus {
    match s {
        "in_progress" => ToolCallStatus::InProgress,
        "completed" => ToolCallStatus::Completed,
        "failed" => ToolCallStatus::Failed,
        _ => ToolCallStatus::Pending,
    }
}

/// `raw_output` 是已序列化的 JSON 文本。尽力解析为结构化 JSON；解析失败时回退为
/// 文本。如果解析后的 JSON 顶层有 `"error"` 字段，提升为 `Error` 变体。
fn parse_tool_call_output_text(text: &str) -> ToolCallOutput {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => {
            if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
                ToolCallOutput::Error {
                    message: err.to_string(),
                }
            } else if let Some(s) = value.as_str() {
                ToolCallOutput::Text {
                    content: s.to_string(),
                }
            } else {
                ToolCallOutput::Json { value }
            }
        }
        Err(_) => ToolCallOutput::Text {
            content: text.to_string(),
        },
    }
}

/// Permission 事件的 `tool_call` 字段是 ACP 的 ToolCall JSON。提取 id 用作
/// `PendingPermissionState.tool_call_id`——快查路径（match by id 时不必每次重
/// 解析整个 tool_call value）。完整 tool_call value 由调用方另行保留，前端
/// 依赖它做 diff / 命令 / plan 渲染。同时兼容 camelCase / snake_case。
fn extract_tool_call_id(tool_call: &serde_json::Value) -> String {
    tool_call
        .as_object()
        .and_then(|o| {
            o.get("toolCallId")
                .or_else(|| o.get("tool_call_id"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string()
}
