//! Companion-side MCP protocol — the bits that live inside the `iyw-claw-mcp`
//! binary but are factored out into the library so they can be unit-tested
//! without spawning the binary.
//!
//! The companion speaks newline-delimited JSON-RPC 2.0 on stdio:
//! one request → one response per line, with concurrent dispatch so
//! `notifications/cancelled` can race an in-flight `tools/call`. It exposes up
//! multiple tools — `delegate_to_agent` (async; returns a `task_id` ack),
//! `get_delegation_status` (poll/long-poll for the result), `cancel_delegation`,
//! `check_user_feedback` (pull the user's mid-turn steering notes),
//! `ask_user_question` (block on a multiple-choice card), and `get_session_info`
//! (resolve a referenced session by id), `show_image`, `analyze_image`,
//! `transcribe_audio`, `query_audio_transcription`,
//! `append_user_memory`, `propose_user_memory`, and scheduled-task CRUD — whose schemas are embedded at compile
//! time from [`TOOL_SCHEMA_JSON`] and gated by the `--features` groups (delegation
//! / feedback / ask / sessions / images / memory / memory-proposal). Only `delegate_to_agent` registers a broker-side
//! cancel handle; canceling a status / cancel / feedback / session round-trip
//! merely suppresses its response — and for `check_user_feedback` also skips the
//! delivery commit, so a cancelled note stays pending.
//!
//! Notifications (id = None) produce no response, matching MCP's expectation
//! that `notifications/initialized` etc. are fire-and-forget.
//!
//! Cancellation flow per the MCP 2024-11-05 / 2025-11-25 cancellation utility:
//!
//! 1. Companion receives `tools/call` with JSON-RPC `id = X`, mints an opaque
//!    `external_handle`, registers `X → (handle, cancel_tx)` in
//!    [`InflightCalls`], and kicks off the broker round-trip.
//! 2. If `notifications/cancelled` for `requestId = X` arrives, the
//!    notification handler pops the entry, fires `cancel_tx`, and sends a
//!    `BrokerMessage::Cancel { external_handle }` to the broker.
//! 3. The `tools/call` task observes `cancel_tx`, abandons its UDS read,
//!    and returns `None` — the binary suppresses the response per spec.
//! 4. If the round-trip completes before the cancel arrives, the entry is
//!    removed normally and the response goes out on stdout; a late cancel
//!    notification finds nothing and is silently ignored.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{oneshot, Mutex};

use crate::acp::automation_tools::{ScheduledTaskOperation, ScheduledTaskRequest};
use crate::acp::delegation::transport::{
    client_artifacts_round_trip, client_ask_round_trip,
    client_audio_transcription_query_round_trip, client_audio_transcription_round_trip,
    client_automation_round_trip, client_cancel, client_cancel_task_round_trip,
    client_channel_round_trip, client_commit_feedback, client_companion_ready_round_trip,
    client_feedback_round_trip, client_image_analysis_round_trip, client_memory_append_round_trip,
    client_memory_proposal_round_trip, client_round_trip, client_session_round_trip,
    client_status_round_trip, BrokerArtifactsRequest, BrokerAskRequest,
    BrokerAudioTranscriptionQueryRequest, BrokerAudioTranscriptionRequest, BrokerCancelRequest,
    BrokerCancelTaskRequest, BrokerChannelRequest, BrokerCommitFeedbackRequest,
    BrokerCompanionReadyRequest, BrokerFeedbackRequest, BrokerImageAnalysisRequest,
    BrokerMemoryAppendRequest, BrokerMemoryProposalRequest, BrokerRequest, BrokerResponse,
    BrokerSessionRequest, BrokerStatusRequest, COMPANION_PROTOCOL_VERSION,
};
use crate::acp::question::parse_questions;
use crate::acp::session_info::MAX_SESSION_MESSAGES;

/// Upper bound on one broker-side cancel round-trip. Bounds both
/// `handle_cancel_notification` (so stdin dispatch can't stall behind a
/// stuck UDS connect/read) and the shutdown-drain loop (so an
/// unresponsive listener can't keep the EOF / watchdog path hung). 500 ms
/// is generous for a same-host UDS exchange and short enough that a user
/// won't notice the bound being hit. Misses are absorbed by the iyw-claw
/// main side's `cancel_by_parent` cascade when the parent ACP connection
/// eventually ends.
const BROKER_CANCEL_BUDGET: Duration = Duration::from_millis(500);
const COMPANION_READY_REPORT_TIMEOUT: Duration = Duration::from_secs(2);

/// Wrap `client_cancel` in [`BROKER_CANCEL_BUDGET`] so callers can fire
/// a synchronous cancel without worrying about a hung listener freezing
/// them. Both success, transport error, and timeout collapse to `()` —
/// callers couldn't usefully react anyway, and the broker has independent
/// cancel backstops (parent / child disconnect cascades) if this one
/// misses.
async fn send_broker_cancel(socket_path: &str, req: &BrokerCancelRequest) {
    let _ = tokio::time::timeout(BROKER_CANCEL_BUDGET, client_cancel(socket_path, req)).await;
}

/// Static MCP tool schema. Lives next to this module so iyw-claw-mcp ships
/// a single embedded copy — no runtime file IO, no version skew with the
/// broker's [`super::types::DelegationRequest`].
pub const TOOL_SCHEMA_JSON: &str = include_str!("tool_schema.json");

/// Machine-readable contract used by the parent process to reject stale
/// sidecars before they reach an agent's MCP startup path.
pub fn binary_capabilities() -> Value {
    let tools = serde_json::from_str::<Value>(TOOL_SCHEMA_JSON)
        .ok()
        .and_then(|schema| schema.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|tool| tool.get("name")?.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    json!({
        "name": "iyw-claw-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": COMPANION_PROTOCOL_VERSION,
        "tools": tools,
    })
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    /// MCP notifications carry no `id`. We dispatch a response only when this
    /// is `Some`.
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

pub fn ok(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(result),
        error: None,
    }
}

pub fn err(id: Value, code: i64, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
            data: None,
        }),
    }
}

/// Which tool groups this companion exposes. One `iyw-claw-mcp` process can carry
/// the delegation tools, the feedback tool, or both — gated independently so
/// each feature can be toggled in settings without the other. Passed in via the
/// `--features` arg at launch; a tool whose group is off is hidden from
/// `tools/list` and rejected on `tools/call`.
#[derive(Debug, Clone, Copy)]
pub struct CompanionFeatures {
    pub delegation: bool,
    pub feedback: bool,
    pub ask: bool,
    pub sessions: bool,
    pub images: bool,
    pub memory: bool,
    pub memory_proposal: bool,
    pub artifacts: bool,
    pub channels: bool,
}

impl CompanionFeatures {
    /// Parse the comma-joined `--features` value (e.g.
    /// `delegation,feedback,ask,sessions,memory,memory-proposal`). Unknown tokens are ignored. An absent
    /// value (`None`) defaults to delegation-only — backward compatible with a
    /// parent that predates feature gating (companion + listener ship together, so
    /// post-upgrade the parent always passes an explicit `--features`).
    pub fn parse(raw: Option<&str>) -> Self {
        let Some(s) = raw else {
            return Self {
                delegation: true,
                feedback: false,
                ask: false,
                sessions: false,
                images: false,
                memory: false,
                memory_proposal: false,
                artifacts: false,
                channels: false,
            };
        };
        let mut f = Self {
            delegation: false,
            feedback: false,
            ask: false,
            sessions: false,
            images: false,
            memory: false,
            memory_proposal: false,
            artifacts: false,
            channels: false,
        };
        for tok in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            match tok {
                "delegation" => f.delegation = true,
                "feedback" => f.feedback = true,
                "ask" => f.ask = true,
                "sessions" => f.sessions = true,
                "images" => f.images = true,
                "memory" => f.memory = true,
                "memory-proposal" => f.memory_proposal = true,
                "artifacts" => f.artifacts = true,
                "channels" => f.channels = true,
                _ => {}
            }
        }
        f
    }

    /// Whether the named MCP tool is exposed under the enabled feature groups.
    pub fn allows_tool(&self, name: &str) -> bool {
        match name {
            "check_user_feedback" => self.feedback,
            "ask_user_question" => self.ask,
            "get_session_info" => self.sessions,
            "show_image" | "analyze_image" => self.images,
            "transcribe_audio" | "query_audio_transcription" => true,
            "append_user_memory" => self.memory,
            "propose_user_memory" => self.memory_proposal,
            "present_task_files" => self.artifacts,
            name if crate::acp::channel_tools::CHANNEL_TOOL_NAMES.contains(&name) => self.channels,
            "list_scheduled_task_projects"
            | "list_scheduled_tasks"
            | "create_scheduled_task"
            | "update_scheduled_task"
            | "delete_scheduled_task" => true,
            "delegate_to_agent" | "get_delegation_status" | "cancel_delegation" => self.delegation,
            _ => false,
        }
    }
}

/// Process arguments threaded through every `tools/call` so the dispatcher
/// can build a [`BrokerRequest`] without re-parsing argv per call.
#[derive(Debug, Clone)]
pub struct CompanionContext {
    pub parent_connection_id: String,
    pub socket_path: String,
    pub token: String,
    pub working_dir: PathBuf,
    /// Current Agent identity used as the default executor on task creation.
    pub agent_type: Option<String>,
    /// Tool groups this launch exposes (see [`CompanionFeatures`]).
    pub features: CompanionFeatures,
}

/// Per-in-flight-call state. The companion stashes one of these per
/// `tools/call` so a subsequent `notifications/cancelled` for the same
/// JSON-RPC `id` can wake the round-trip task and trigger a broker-side
/// cancel.
pub struct InflightEntry {
    /// Companion-minted opaque handle threaded through the broker, for the
    /// `delegate_to_agent` tool ONLY — a `notifications/cancelled` during its
    /// setup must tear down the just-started child via the broker's
    /// `cancel_by_external_handle`. `None` for `get_delegation_status` /
    /// `cancel_delegation`: canceling those round-trips only suppresses the
    /// response (no broker-side cancel — the query/cancel itself must not touch
    /// the task).
    external_handle: Option<String>,
    /// Tripped by the cancel handler to wake the round-trip task.
    cancel_tx: oneshot::Sender<()>,
}

/// `request_id_key(id) → InflightEntry`. Keyed by a string form of the
/// JSON-RPC `id` so we can compare against the `requestId` payload of
/// `notifications/cancelled` which is itself a JSON value (numbers serialize
/// as their canonical string form here).
#[derive(Default)]
pub struct InflightCalls {
    inner: Mutex<HashMap<String, InflightEntry>>,
}

impl InflightCalls {
    pub fn new() -> Self {
        Self::default()
    }

    async fn register(&self, id_key: String, entry: InflightEntry) {
        self.inner.lock().await.insert(id_key, entry);
    }

    async fn take(&self, id_key: &str) -> Option<InflightEntry> {
        self.inner.lock().await.remove(id_key)
    }

    /// Drain every in-flight entry, clearing the registry. Called at
    /// companion shutdown so we can fire one broker cancel per pending
    /// delegation — without this the broker would park on `rx.await` for
    /// each entry until the parent ACP connection's `cancel_by_parent`
    /// fires (or never, if the agent CLI keeps running after only the
    /// MCP child died).
    pub async fn drain_all(&self) -> Vec<InflightEntry> {
        let mut map = self.inner.lock().await;
        map.drain().map(|(_k, v)| v).collect()
    }
}

/// Canonicalize a JSON-RPC `id` to a string suitable as a `HashMap` key.
/// JSON-RPC permits string OR number ids; we collapse both via
/// `serde_json::to_string` so a numeric `42` and string `"42"` stay
/// distinct (which the spec also requires).
pub fn request_id_key(id: &Value) -> String {
    serde_json::to_string(id).unwrap_or_else(|_| String::from("null"))
}

/// Dispatch verdict for a single inbound stdin line.
pub enum LineAction {
    /// Synchronous response — write `resp` to stdout immediately.
    Respond(JsonRpcResponse),
    /// Asynchronous tools/call — the binary should spawn the round-trip
    /// task and only write a response if the future returns `Some`.
    Spawn(SpawnedCall),
    /// Notification or no-op (parse errors with `id = null`). Nothing to
    /// emit on stdout.
    Silent,
}

/// Resolution of a spawned `tools/call`: the response to relay to the agent
/// (`None` = cancellation won, so suppress per the MCP spec) plus an optional
/// action the binary runs ONLY after that response is successfully written to
/// the agent's stdout.
///
/// `after_relay` exists for `check_user_feedback`: marking the pulled notes
/// `Delivered` (the broker `CommitFeedback`) must happen strictly AFTER the
/// agent actually receives them. Committing any earlier — at listener read
/// time, or right after the round-trip but before the stdout relay — would mark
/// a note delivered that a failed/never-reached write (or a companion dying mid
/// teardown) never put in front of the agent, breaking at-least-once delivery.
/// Every other tool leaves this `None`.
pub struct SpawnResult {
    pub response: Option<JsonRpcResponse>,
    pub after_relay: Option<futures_util::future::BoxFuture<'static, ()>>,
}

/// Materialized async tools/call ready to drive in a tokio task. The binary
/// awaits `future` to obtain the [`SpawnResult`]: it writes `response` (when
/// `Some`) and, on a successful write, runs `after_relay` (when `Some`).
pub struct SpawnedCall {
    /// JSON-RPC `id` of the original `tools/call` so the binary can stamp
    /// the response.
    pub request_id: Value,
    /// String form of `request_id` for inflight bookkeeping.
    pub request_id_key: String,
    /// The future that performs the UDS round-trip racing the cancel channel
    /// and resolves to the [`SpawnResult`] to relay (and optionally commit).
    pub future: futures_util::future::BoxFuture<'static, SpawnResult>,
}

/// Parse a stdin line and produce a [`LineAction`]. The binary handles the
/// IO side; this function is pure aside from registering the inflight
/// entry on `tools/call` so unit tests can drive it without stdio.
pub async fn dispatch_line(
    ctx: &CompanionContext,
    inflight: Arc<InflightCalls>,
    line: &str,
) -> LineAction {
    let req: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return LineAction::Respond(err(Value::Null, -32700, format!("parse error: {e}")));
        }
    };

    // Notifications carry no id — no response goes out. Cancellation is
    // the only notification we act on.
    if req.id.is_none() {
        if req.method == "notifications/cancelled" {
            handle_cancel_notification(ctx, &inflight, &req.params).await;
        }
        return LineAction::Silent;
    }

    let id = req.id.expect("checked is_none");
    match req.method.as_str() {
        "initialize" => LineAction::Respond(ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "iyw-claw-mcp",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": { "tools": {} },
            }),
        )),
        "tools/list" => {
            // The embedded schema is a JSON array of every tool the companion
            // can carry; filter to the groups enabled for this launch so a
            // disabled feature's tools never surface to the LLM.
            let all: Value = match serde_json::from_str(TOOL_SCHEMA_JSON) {
                Ok(v) => v,
                Err(e) => {
                    return LineAction::Respond(err(
                        id,
                        -32603,
                        format!("embedded schema invalid: {e}"),
                    ));
                }
            };
            let tools = match all.as_array() {
                Some(arr) => Value::Array(
                    arr.iter()
                        .filter(|t| {
                            t.get("name")
                                .and_then(|v| v.as_str())
                                .map(|n| ctx.features.allows_tool(n))
                                .unwrap_or(false)
                        })
                        .cloned()
                        .collect(),
                ),
                None => all,
            };
            LineAction::Respond(ok(id, json!({ "tools": tools })))
        }
        "tools/call" => build_tools_call_spawn(ctx.clone(), inflight, id, req.params).await,
        _ => LineAction::Respond(err(id, -32601, format!("method not found: {}", req.method))),
    }
}

/// Build the authenticated readiness report for a successful static
/// `tools/list` response. The binary schedules delivery only after stdout has
/// been flushed to the Agent.
pub fn companion_ready_report_after_tools_list(
    ctx: &CompanionContext,
    line: &str,
    response: &JsonRpcResponse,
) -> Option<BrokerCompanionReadyRequest> {
    let Ok(request) = serde_json::from_str::<JsonRpcRequest>(line) else {
        return None;
    };
    if request.method != "tools/list" || response.error.is_some() {
        return None;
    }
    let tools = response
        .result
        .as_ref()
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)?;
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name")?.as_str().map(str::to_string))
        .collect();
    Some(BrokerCompanionReadyRequest {
        token: ctx.token.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        protocol_version: COMPANION_PROTOCOL_VERSION,
        tools: names,
    })
}

/// Deliver readiness out-of-band so a stuck host bridge cannot block the MCP
/// stdin loop. This bound is shorter than the host-side readiness wait.
pub async fn send_companion_ready_report(socket_path: String, report: BrokerCompanionReadyRequest) {
    match tokio::time::timeout(
        COMPANION_READY_REPORT_TIMEOUT,
        client_companion_ready_round_trip(&socket_path, &report),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => tracing::warn!(error = %error, "companion ready report failed"),
        Err(_) => tracing::warn!("companion ready report timed out"),
    }
}

/// Build the spawned-call descriptor for a `tools/call` (or, when the
/// arguments are obviously bogus, a synchronous error response). Registers
/// the inflight entry and returns a future the binary should drive.
async fn build_tools_call_spawn(
    ctx: CompanionContext,
    inflight: Arc<InflightCalls>,
    id: Value,
    params: Value,
) -> LineAction {
    let raw_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // Strip any MCP namespace prefix (e.g. "mcp__iyw_claw_mcp__append_user_memory"
    // → "append_user_memory"). Some Responses API hosts (Codex CLI) forward the
    // full namespace-prefixed name to the MCP server rather than stripping it
    // before dispatch; we normalise here so routing works regardless.
    let name = strip_namespace_prefix(&raw_name);
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let socket = ctx.socket_path.clone();
    // Defense in depth: tools/list already hides tools whose feature group is
    // off, but a misbehaving client could still call one by name. A disabled
    // tool is rejected uniformly as "unknown tool" — indistinguishable from a
    // genuinely nonexistent one (no leak that the feature exists but is off),
    // and matching the legacy unknown-tool rejection shape.
    if !ctx.features.allows_tool(&name) {
        return LineAction::Respond(err(id, -32602, format!("unknown tool: {name}")));
    }
    match name.as_str() {
        "show_image" => register_and_spawn_local(inflight, id, arguments, ctx.working_dir).await,
        "analyze_image" => register_and_spawn_image_analysis(inflight, id, arguments, ctx).await,
        "transcribe_audio" => {
            register_and_spawn_audio(inflight, id, arguments, ctx, AudioToolCall::Transcribe).await
        }
        "query_audio_transcription" => {
            register_and_spawn_audio(inflight, id, arguments, ctx, AudioToolCall::Query).await
        }
        "list_scheduled_task_projects"
        | "list_scheduled_tasks"
        | "create_scheduled_task"
        | "update_scheduled_task"
        | "delete_scheduled_task" => {
            let operation = match name.as_str() {
                "list_scheduled_task_projects" => ScheduledTaskOperation::ListProjects,
                "list_scheduled_tasks" => ScheduledTaskOperation::List,
                "create_scheduled_task" => ScheduledTaskOperation::Create,
                "update_scheduled_task" => ScheduledTaskOperation::Update,
                _ => ScheduledTaskOperation::Delete,
            };
            let req = ScheduledTaskRequest {
                operation,
                input: arguments,
                caller_agent_type: ctx.agent_type.clone(),
            };
            let round_trip =
                Box::pin(async move { client_automation_round_trip(&socket, &req).await });
            register_and_spawn(inflight, id, None, round_trip, render_automation_result).await
        }
        channel_tool if crate::acp::channel_tools::CHANNEL_TOOL_NAMES.contains(&channel_tool) => {
            let req = BrokerChannelRequest {
                token: ctx.token.clone(),
                tool: channel_tool.to_string(),
                input: arguments,
            };
            let round_trip =
                Box::pin(async move { client_channel_round_trip(&socket, &req).await });
            register_and_spawn(inflight, id, None, round_trip, render_channel_result).await
        }
        "present_task_files" => {
            let files = match parse_artifact_files(&arguments) {
                Ok(files) => files,
                Err(message) => return LineAction::Respond(err(id, -32602, message)),
            };
            let req = BrokerArtifactsRequest {
                token: ctx.token.clone(),
                files,
            };
            let round_trip =
                Box::pin(async move { client_artifacts_round_trip(&socket, &req).await });
            register_and_spawn(inflight, id, None, round_trip, render_artifacts_result).await
        }
        "append_user_memory" => {
            let content = match arguments.get("content").and_then(Value::as_str) {
                Some(content) if !content.trim().is_empty() => content.to_string(),
                _ => {
                    return LineAction::Respond(err(
                        id,
                        -32602,
                        "append_user_memory requires non-empty string `content`",
                    ));
                }
            };
            if content.chars().count() > crate::user_memory::USER_MEMORY_MAX_APPEND_CHARS {
                return LineAction::Respond(err(
                    id,
                    -32602,
                    format!(
                        "append_user_memory content exceeds {} characters",
                        crate::user_memory::USER_MEMORY_MAX_APPEND_CHARS
                    ),
                ));
            }
            let req = BrokerMemoryAppendRequest {
                token: ctx.token.clone(),
                content,
            };
            let round_trip = Box::pin(async move {
                memory_round_trip_result(
                    client_memory_append_round_trip(&socket, &req).await,
                    "append",
                )
            });
            register_and_spawn(inflight, id, None, round_trip, render_memory_append_result).await
        }
        "propose_user_memory" => {
            let proposal = match serde_json::from_value::<crate::user_memory::AgentMemoryProposal>(
                arguments,
            ) {
                Ok(proposal) if !proposal.content.trim().is_empty() => proposal,
                _ => {
                    return LineAction::Respond(err(
                        id,
                        -32602,
                        "propose_user_memory requires `content` and a valid `signal`",
                    ));
                }
            };
            if proposal.content.chars().count()
                > crate::user_memory::USER_MEMORY_MAX_CANDIDATE_CHARS
            {
                return LineAction::Respond(err(
                    id,
                    -32602,
                    format!(
                        "propose_user_memory content exceeds {} characters",
                        crate::user_memory::USER_MEMORY_MAX_CANDIDATE_CHARS
                    ),
                ));
            }
            let req = BrokerMemoryProposalRequest {
                token: ctx.token.clone(),
                content: proposal.content,
                signal: proposal.signal,
            };
            let round_trip = Box::pin(async move {
                memory_round_trip_result(
                    client_memory_proposal_round_trip(&socket, &req).await,
                    "proposal",
                )
            });
            register_and_spawn(
                inflight,
                id,
                None,
                round_trip,
                render_memory_proposal_result,
            )
            .await
        }
        "delegate_to_agent" => {
            // MCP clients (Codex / Claude Code) generally do NOT populate
            // `_meta.tool_use_id` when calling an MCP server. We still surface it
            // when present (the most precise binding), but a missing one is
            // expected — the broker falls back to claiming the most recent
            // `delegate_to_agent` tool_call_id observed on the parent's ACP
            // event stream.
            let tool_use_id = params
                .get("_meta")
                .and_then(|m| m.get("tool_use_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Mint an external_handle so a `notifications/cancelled` during setup
            // tears down the just-started child via `cancel_by_external_handle`.
            let external_handle = uuid::Uuid::new_v4().to_string();
            let req = BrokerRequest {
                token: ctx.token.clone(),
                parent_connection_id: ctx.parent_connection_id.clone(),
                parent_tool_use_id: tool_use_id,
                external_handle: Some(external_handle.clone()),
                input: arguments,
            };
            let round_trip = Box::pin(async move { client_round_trip(&socket, &req).await });
            register_and_spawn(
                inflight,
                id,
                Some(external_handle),
                round_trip,
                render_task_report,
            )
            .await
        }
        "get_delegation_status" => {
            // Normalize the `task_ids` array: trim, drop empty/whitespace
            // entries, de-dup (order-preserving). A non-string entry violates the
            // schema's `items: string` contract and is rejected outright (rather
            // than silently polling a subset); an all-empty / missing array maps
            // to `Ok(empty)`, rejected below.
            let task_ids = match normalize_status_task_ids(&arguments) {
                Ok(ids) if !ids.is_empty() => ids,
                Ok(_) => {
                    return LineAction::Respond(err(
                        id,
                        -32602,
                        "get_delegation_status requires a non-empty task_ids array \
                         (one or more task ids)",
                    ));
                }
                Err(msg) => return LineAction::Respond(err(id, -32602, msg)),
            };
            let wait_ms = arguments.get("wait_ms").and_then(|v| v.as_u64());
            let req = BrokerStatusRequest {
                token: ctx.token.clone(),
                task_ids,
                wait_ms,
            };
            // No external_handle: canceling a status query only suppresses its
            // response — it must not touch the task itself. The status round-trip
            // returns a `{tasks:[..]}` envelope, so it renders via
            // `render_status_result` — uniformly one `{tasks:[..]}` entry per id,
            // whether the poll asked for a single id or a whole fan-out.
            let round_trip = Box::pin(async move { client_status_round_trip(&socket, &req).await });
            register_and_spawn(inflight, id, None, round_trip, render_status_result).await
        }
        "cancel_delegation" => {
            let task_id = match arguments.get("task_id").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => {
                    return LineAction::Respond(err(
                        id,
                        -32602,
                        "cancel_delegation requires a non-empty string task_id",
                    ));
                }
            };
            let req = BrokerCancelTaskRequest {
                token: ctx.token.clone(),
                task_id,
            };
            let round_trip =
                Box::pin(async move { client_cancel_task_round_trip(&socket, &req).await });
            register_and_spawn(inflight, id, None, round_trip, render_task_report).await
        }
        "check_user_feedback" => {
            let req = BrokerFeedbackRequest {
                token: ctx.token.clone(),
            };
            // Feedback uses a dedicated spawn so it can COMMIT delivery only when
            // the round-trip wins the cancel race (i.e. the result actually goes
            // to the agent). A cancel that suppresses the response sends no
            // commit, leaving the notes pending for the next check.
            register_and_spawn_feedback(inflight, id, socket, ctx.token.clone(), req).await
        }
        "ask_user_question" => {
            // Validate + parse the schema HERE so a malformed call gets a
            // synchronous -32602 the LLM can fix, rather than round-tripping bad
            // data. Stable per-question ids are minted now and flow through to
            // the answer correlation.
            let questions = match parse_questions(&arguments) {
                Ok(qs) => qs,
                Err(msg) => return LineAction::Respond(err(id, -32602, msg)),
            };
            let req = BrokerAskRequest {
                token: ctx.token.clone(),
                questions,
            };
            // No external_handle: canceling a blocking ask only suppresses its
            // response. The companion dropping the round-trip future closes the
            // socket, which the listener observes (peer-close) to tear the
            // pending question down — no broker-side cancel to dispatch.
            let round_trip = Box::pin(async move { client_ask_round_trip(&socket, &req).await });
            register_and_spawn(inflight, id, None, round_trip, render_ask_result).await
        }
        "get_session_info" => {
            // `session_id` is the iyw-claw conversation id the agent read out of a
            // `iyw-claw://session/<id>` reference. Accept a JSON number or a numeric
            // string (some hosts stringify integer args); reject anything else
            // synchronously so the LLM can fix it.
            let session_id = match parse_session_id(&arguments) {
                Some(id) => id,
                None => {
                    return LineAction::Respond(err(
                        id,
                        -32602,
                        "get_session_info requires an integer `session_id` \
                         (the number in the iyw-claw://session/<id> reference)",
                    ));
                }
            };
            // Default to a modest recent-message window; `0` means metadata-only.
            // Robust against stringified / oversized values (see helper).
            let max_messages = parse_max_messages(&arguments);
            let req = BrokerSessionRequest {
                token: ctx.token.clone(),
                session_id,
                max_messages: Some(max_messages),
            };
            // No external_handle: a read-only lookup has nothing to cancel
            // broker-side — canceling only suppresses the response.
            let round_trip =
                Box::pin(async move { client_session_round_trip(&socket, &req).await });
            register_and_spawn(inflight, id, None, round_trip, render_session_result).await
        }
        other => LineAction::Respond(err(id, -32602, format!("unknown tool: {other}"))),
    }
}

/// Strip any MCP namespace prefix from a tool name, returning the bare name.
///
/// Codex CLI (and possibly other Responses API hosts) forwards the full
/// namespace-prefixed tool name to the MCP server's `tools/call` instead of
/// stripping it before dispatch.  For example:
///
///   `mcp__iyw_claw_mcp__append_user_memory` → `append_user_memory`
///   `iyw-claw-mcp__append_user_memory`      → `append_user_memory`
///   `append_user_memory`                     → `append_user_memory` (unchanged)
///
/// The stripping rule: take everything after the last `__` occurrence.  This
/// is safe because none of the bare tool names contain `__`.
fn strip_namespace_prefix(name: &str) -> String {
    if let Some(idx) = name.rfind("__") {
        let bare = &name[idx + 2..];
        if !bare.is_empty() {
            return bare.to_string();
        }
    }
    name.to_string()
}

async fn register_and_spawn_local(
    inflight: Arc<InflightCalls>,
    id: Value,
    arguments: Value,
    working_dir: PathBuf,
) -> LineAction {
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let id_key = request_id_key(&id);
    inflight
        .register(
            id_key.clone(),
            InflightEntry {
                external_handle: None,
                cancel_tx,
            },
        )
        .await;
    let response_id = id.clone();
    let task_key = id_key.clone();
    let task_inflight = inflight.clone();
    let future = Box::pin(async move {
        let response = tokio::select! {
            biased;
            _ = cancel_rx => None,
            result = crate::acp::delegation::image_tool::execute(arguments, working_dir) => {
                Some(ok(response_id, result))
            }
        };
        let _ = task_inflight.take(&task_key).await;
        SpawnResult {
            response,
            after_relay: None,
        }
    });
    LineAction::Spawn(SpawnedCall {
        request_id: id,
        request_id_key: id_key,
        future,
    })
}

async fn register_and_spawn_image_analysis(
    inflight: Arc<InflightCalls>,
    id: Value,
    arguments: Value,
    ctx: CompanionContext,
) -> LineAction {
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let id_key = request_id_key(&id);
    inflight
        .register(
            id_key.clone(),
            InflightEntry {
                external_handle: None,
                cancel_tx,
            },
        )
        .await;
    let response_id = id.clone();
    let task_key = id_key.clone();
    let task_inflight = inflight.clone();
    let future = Box::pin(async move {
        let analyze = execute_image_analysis_call(response_id, arguments, ctx);
        tokio::pin!(analyze);
        let response = tokio::select! {
            biased;
            _ = cancel_rx => None,
            result = &mut analyze => Some(result),
        };
        let _ = task_inflight.take(&task_key).await;
        SpawnResult {
            response,
            after_relay: None,
        }
    });
    LineAction::Spawn(SpawnedCall {
        request_id: id,
        request_id_key: id_key,
        future,
    })
}

async fn execute_image_analysis_call(
    response_id: Value,
    arguments: Value,
    ctx: CompanionContext,
) -> JsonRpcResponse {
    let prepared =
        match crate::acp::delegation::image_tool::prepare_analysis(arguments, &ctx.working_dir)
            .await
        {
            Ok(prepared) => prepared,
            Err(result) => return ok(response_id, result),
        };
    let request = BrokerImageAnalysisRequest {
        token: ctx.token,
        data: prepared.data,
        mime_type: prepared.mime_type,
        question: prepared.question,
        detail: prepared.detail,
        image_bytes: prepared.image_bytes,
    };
    let result = match client_image_analysis_round_trip(&ctx.socket_path, &request).await {
        Ok(response) => render_image_analysis_result(&response.outcome),
        Err(_) => image_analysis_error_result(
            "image_analysis_transport_failed",
            "The image analysis host service is unavailable.",
        ),
    };
    ok(response_id, result)
}

#[derive(Clone, Copy)]
enum AudioToolCall {
    Transcribe,
    Query,
}

async fn register_and_spawn_audio(
    inflight: Arc<InflightCalls>,
    id: Value,
    arguments: Value,
    ctx: CompanionContext,
    call: AudioToolCall,
) -> LineAction {
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let id_key = request_id_key(&id);
    inflight
        .register(
            id_key.clone(),
            InflightEntry {
                external_handle: None,
                cancel_tx,
            },
        )
        .await;
    let response_id = id.clone();
    let task_key = id_key.clone();
    let task_inflight = inflight.clone();
    let future = Box::pin(async move {
        let execute = execute_audio_call(response_id, arguments, ctx, call);
        tokio::pin!(execute);
        let response = tokio::select! {
            biased;
            _ = cancel_rx => None,
            result = &mut execute => Some(result),
        };
        let _ = task_inflight.take(&task_key).await;
        SpawnResult {
            response,
            after_relay: None,
        }
    });
    LineAction::Spawn(SpawnedCall {
        request_id: id,
        request_id_key: id_key,
        future,
    })
}

async fn execute_audio_call(
    response_id: Value,
    arguments: Value,
    ctx: CompanionContext,
    call: AudioToolCall,
) -> JsonRpcResponse {
    let result = match call {
        AudioToolCall::Transcribe => execute_audio_transcription(arguments, ctx).await,
        AudioToolCall::Query => execute_audio_query(arguments, ctx).await,
    };
    ok(response_id, result)
}

async fn execute_audio_transcription(arguments: Value, ctx: CompanionContext) -> Value {
    let input = match crate::acp::delegation::audio_tool::prepare_transcribe(arguments) {
        Ok(input) => input,
        Err(result) => return result,
    };
    let request = BrokerAudioTranscriptionRequest {
        token: ctx.token,
        path: input.path,
        language: input.language,
        options: input.options,
    };
    match client_audio_transcription_round_trip(&ctx.socket_path, &request).await {
        Ok(response) => render_audio_result(response.outcome),
        Err(_) => crate::acp::delegation::audio_tool::error_result(
            "audio_transcription_transport_failed",
            "The audio transcription host service is unavailable.",
        ),
    }
}

async fn execute_audio_query(arguments: Value, ctx: CompanionContext) -> Value {
    let input = match crate::acp::delegation::audio_tool::prepare_query(arguments) {
        Ok(input) => input,
        Err(result) => return result,
    };
    let request = BrokerAudioTranscriptionQueryRequest {
        token: ctx.token,
        job_id: input.job_id.trim().to_string(),
    };
    match client_audio_transcription_query_round_trip(&ctx.socket_path, &request).await {
        Ok(response) => render_audio_result(response.outcome),
        Err(_) => crate::acp::delegation::audio_tool::error_result(
            "audio_transcription_transport_failed",
            "The audio transcription host service is unavailable.",
        ),
    }
}

fn render_audio_result(outcome: Value) -> Value {
    let Some(structured) = outcome.get("structuredContent") else {
        return crate::acp::delegation::audio_tool::error_result(
            "audio_transcription_invalid_response",
            "The audio transcription host returned an invalid result.",
        );
    };
    if outcome.get("isError").and_then(Value::as_bool) == Some(true) {
        let code = structured
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("audio_transcription_failed");
        let (code, message) = safe_audio_error(code);
        return crate::acp::delegation::audio_tool::error_result(code, message);
    }
    let Some(job_id) = structured.get("jobId").and_then(Value::as_str) else {
        return crate::acp::delegation::audio_tool::error_result(
            "audio_transcription_invalid_response",
            "The audio transcription host returned an invalid result.",
        );
    };
    let Some(status) = structured.get("status").and_then(Value::as_str) else {
        return crate::acp::delegation::audio_tool::error_result(
            "audio_transcription_invalid_response",
            "The audio transcription host returned an invalid result.",
        );
    };
    let transcript = structured.get("transcript").cloned().unwrap_or(Value::Null);
    let text = transcript
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Transcription job {job_id} is {status}."));
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": {
            "jobId": job_id,
            "status": status,
            "transcript": transcript,
        },
    })
}

fn safe_audio_error(code: &str) -> (&'static str, &'static str) {
    match code {
        "audio_transcription_invalid_path" => (
            "audio_transcription_invalid_path",
            "Audio path must be a readable workspace-relative audio file.",
        ),
        "audio_transcription_invalid_arguments" => (
            "audio_transcription_invalid_arguments",
            "Audio transcription arguments are invalid.",
        ),
        "audio_transcription_unsupported_format" => (
            "audio_transcription_unsupported_format",
            "The audio file format is not supported.",
        ),
        "audio_transcription_auth_required" => (
            "audio_transcription_auth_required",
            "Sign in to iyw-claw before transcribing audio.",
        ),
        "audio_transcription_transport_failed" => (
            "audio_transcription_transport_failed",
            "The transcription service could not be reached.",
        ),
        "audio_transcription_upload_failed" => (
            "audio_transcription_upload_failed",
            "The audio file could not be uploaded.",
        ),
        "audio_transcription_request_failed" => (
            "audio_transcription_request_failed",
            "The transcription request was rejected.",
        ),
        "audio_transcription_session_missing" => (
            "audio_transcription_session_missing",
            "The audio transcription session is unavailable.",
        ),
        _ => (
            "audio_transcription_failed",
            "The audio transcription request failed.",
        ),
    }
}

/// Register the inflight entry and build the [`SpawnedCall`] that races the
/// broker round-trip against the cancel signal. `external_handle` is `Some` only
/// for `delegate_to_agent` (so a cancel during setup tears the child down);
/// `None` for status/cancel queries (a cancel only suppresses the response).
///
/// `render` maps the broker's `BrokerResponse.outcome` into the MCP `tools/call`
/// result body: `delegate_to_agent` / `cancel_delegation` pass
/// [`render_task_report`] (a single report); `get_delegation_status` passes
/// [`render_status_result`] (always a `{tasks:[..]}` envelope, one entry per id).
async fn register_and_spawn(
    inflight: Arc<InflightCalls>,
    id: Value,
    external_handle: Option<String>,
    round_trip: futures_util::future::BoxFuture<'static, std::io::Result<BrokerResponse>>,
    render: fn(&Value) -> Value,
) -> LineAction {
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let id_key = request_id_key(&id);
    inflight
        .register(
            id_key.clone(),
            InflightEntry {
                external_handle,
                cancel_tx,
            },
        )
        .await;

    let id_for_response = id.clone();
    let id_key_for_task = id_key.clone();
    let inflight_for_task = inflight.clone();
    let future = Box::pin(async move {
        // Race the UDS round-trip against the cancel signal. Cancel wins →
        // suppress the response per MCP spec; for `delegate_to_agent` the cancel
        // notification handler is responsible for dispatching the broker-side
        // `Cancel` (status/cancel queries carry no external_handle, so nothing
        // is dispatched).
        let response = tokio::select! {
            biased;
            _ = cancel_rx => {
                let _ = inflight_for_task.take(&id_key_for_task).await;
                None
            }
            rt = round_trip => {
                let _ = inflight_for_task.take(&id_key_for_task).await;
                match rt {
                    Ok(resp) => Some(ok(id_for_response, render(&resp.outcome))),
                    Err(e) => Some(err(
                        id_for_response,
                        -32603,
                        format!("broker round-trip failed: {e}"),
                    )),
                }
            }
        };
        // Delegation / status / cancel have no post-relay step.
        SpawnResult {
            response,
            after_relay: None,
        }
    });

    LineAction::Spawn(SpawnedCall {
        request_id: id,
        request_id_key: id_key,
        future,
    })
}

fn render_image_analysis_result(outcome: &Value) -> Value {
    if let Some(error) = outcome.get("error").and_then(Value::as_str) {
        let code = outcome
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("image_analysis_failed");
        return image_analysis_error_result(code, error);
    }
    let Some(analyses) = outcome.get("analyses").and_then(Value::as_array) else {
        return image_analysis_error_result(
            "image_analysis_invalid_response",
            "The image analysis host returned an invalid result.",
        );
    };
    let summaries = analyses
        .iter()
        .enumerate()
        .filter_map(|(index, analysis)| render_analysis_summary(index, analysis))
        .collect::<Vec<_>>();
    if summaries.is_empty() {
        return image_analysis_error_result(
            "image_analysis_invalid_response",
            "The image analysis host returned no analysis text.",
        );
    }
    json!({
        "content": [{ "type": "text", "text": summaries.join("\n\n") }],
        "isError": false,
        "structuredContent": outcome,
    })
}

fn render_analysis_summary(index: usize, analysis: &Value) -> Option<String> {
    let summary = analysis.get("summary")?.as_str()?.trim();
    if summary.is_empty() {
        return None;
    }
    let uncertainty = analysis
        .get("uncertainty")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|item| !item.trim().is_empty())
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    if uncertainty.is_empty() {
        Some(format!("Image {}: {summary}", index + 1))
    } else {
        Some(format!(
            "Image {}: {summary}\nUncertainty: {uncertainty}",
            index + 1
        ))
    }
}

fn image_analysis_error_result(code: &str, message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
        "structuredContent": { "code": code, "error": message },
    })
}

/// `check_user_feedback`-specific spawn. Like [`register_and_spawn`], but it
/// carries an `after_relay` commit — a `CommitFeedback` round-trip marking the
/// pulled notes `Delivered` — that the binary runs ONLY after it successfully
/// writes this response to the agent's stdout (the listener does not commit at
/// read time). Two guards compose to make delivery at-least-once. First, if the
/// cancel branch wins the biased select the result is `response: None` with no
/// `after_relay`, so the check is suppressed and never committed (the notes stay
/// pending for the next check). Second, when the round-trip wins, `after_relay`
/// is built but only fires once the stdout relay succeeds; a failed or
/// never-reached write (a dying companion, a broken agent stdin) skips the
/// commit entirely. So a note flips to `Delivered` only after it was actually
/// put in front of the agent. The sole irreducible boundary is the agent
/// crashing after the bytes are flushed to its stdin but before it reads them —
/// at which point the note is moot (the agent will not act on it), the correct
/// semantics for a delivered best-effort steering side-channel.
async fn register_and_spawn_feedback(
    inflight: Arc<InflightCalls>,
    id: Value,
    socket: String,
    token: String,
    req: BrokerFeedbackRequest,
) -> LineAction {
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let id_key = request_id_key(&id);
    inflight
        .register(
            id_key.clone(),
            InflightEntry {
                external_handle: None,
                cancel_tx,
            },
        )
        .await;

    let id_for_response = id.clone();
    let id_key_for_task = id_key.clone();
    let inflight_for_task = inflight.clone();
    let future = Box::pin(async move {
        tokio::select! {
            biased;
            _ = cancel_rx => {
                // Cancelled before delivery → suppress AND do not commit.
                let _ = inflight_for_task.take(&id_key_for_task).await;
                SpawnResult {
                    response: None,
                    after_relay: None,
                }
            }
            rt = client_feedback_round_trip(&socket, &req) => {
                let _ = inflight_for_task.take(&id_key_for_task).await;
                match rt {
                    Ok(resp) => {
                        // Relay-then-commit: render the agent-facing result now,
                        // but defer the `CommitFeedback` to `after_relay` so it
                        // fires ONLY after the binary writes this response to the
                        // agent's stdout. A dead/failed relay skips the commit,
                        // leaving the notes pending for the next check
                        // (at-least-once at the agent-facing boundary).
                        let outcome = resp.outcome;
                        let response = ok(id_for_response, render_feedback_result(&outcome));
                        let commit: futures_util::future::BoxFuture<'static, ()> =
                            Box::pin(async move {
                                commit_feedback_after_delivery(&socket, &token, &outcome).await;
                            });
                        SpawnResult {
                            response: Some(response),
                            after_relay: Some(commit),
                        }
                    }
                    Err(e) => SpawnResult {
                        response: Some(err(
                            id_for_response,
                            -32603,
                            format!("broker round-trip failed: {e}"),
                        )),
                        after_relay: None,
                    },
                }
            }
        }
    });

    LineAction::Spawn(SpawnedCall {
        request_id: id,
        request_id_key: id_key,
        future,
    })
}

/// Send a `CommitFeedback` for the note ids the listener embedded in the
/// response (`_commit_ids`). Fire-and-forget, bounded by [`BROKER_CANCEL_BUDGET`]:
/// a failed commit just leaves the notes pending for the next check.
async fn commit_feedback_after_delivery(socket: &str, token: &str, outcome: &Value) {
    let ids: Vec<String> = outcome
        .get("_commit_ids")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return;
    }
    let req = BrokerCommitFeedbackRequest {
        token: token.to_string(),
        ids,
    };
    let _ = tokio::time::timeout(BROKER_CANCEL_BUDGET, client_commit_feedback(socket, &req)).await;
}

/// Handle a `notifications/cancelled` notification. Looks up the in-flight
/// call by `requestId` and fires its cancel channel. Unknown ids are
/// silently ignored per MCP spec.
async fn handle_cancel_notification(
    ctx: &CompanionContext,
    inflight: &Arc<InflightCalls>,
    params: &Value,
) {
    let request_id = match params.get("requestId") {
        Some(v) => v.clone(),
        None => return,
    };
    let id_key = request_id_key(&request_id);
    let Some(entry) = inflight.take(&id_key).await else {
        return;
    };
    let _ = entry.cancel_tx.send(());
    // Only `delegate_to_agent` carries an external_handle. For
    // `get_delegation_status` / `cancel_delegation` there is nothing to cancel
    // broker-side — suppressing the (possibly long-poll) response is the whole
    // effect, and dispatching a broker `Cancel` would wrongly target a task.
    let Some(external_handle) = entry.external_handle else {
        return;
    };
    // Single broker-side cancel per notification: the round-trip task
    // observes `cancel_rx` and only suppresses its response. If we ALSO
    // dispatched a cancel from the task we'd hit the broker twice — the
    // first call drains the pending entry, the second buffers the handle
    // in `pre_canceled_handles` with no consumer (silent leak).
    //
    // Synchronous, bounded by `BROKER_CANCEL_BUDGET`. Detaching via
    // `tokio::spawn` would race the runtime shutdown: if stdin closes
    // before the spawned task scheduled its UDS connect, the runtime
    // drops it and the broker never gets the cancel. The bounded await
    // here guarantees the cancel either lands or hits a known cap
    // before the next stdin line is read.
    let cancel_req = BrokerCancelRequest {
        token: ctx.token.clone(),
        external_handle,
        reason: params
            .get("reason")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    };
    send_broker_cancel(&ctx.socket_path, &cancel_req).await;
}

/// Drain every in-flight `tools/call` entry and dispatch a broker cancel
/// for each. Called at companion shutdown (stdin EOF, parent-watchdog
/// fire) so the broker doesn't hold a `pending` row open forever waiting
/// for a `TurnComplete` whose response we couldn't deliver anyway. Each
/// cancel is bounded by [`BROKER_CANCEL_BUDGET`] so a hung listener
/// can't pin shutdown — the iyw-claw main side's `cancel_by_parent` cascade
/// is the eventual backstop for any cancel that times out here.
pub async fn drain_and_cancel_all(
    ctx: &CompanionContext,
    inflight: &Arc<InflightCalls>,
    reason: &str,
) {
    for entry in inflight.drain_all().await {
        // Wake the round-trip task if it's still scheduled, so it can
        // exit promptly when the runtime tears down.
        let _ = entry.cancel_tx.send(());
        // Only delegate_to_agent entries hold an external_handle worth a
        // broker-side cancel; status/cancel queries have nothing to tear down.
        let Some(external_handle) = entry.external_handle else {
            continue;
        };
        let cancel_req = BrokerCancelRequest {
            token: ctx.token.clone(),
            external_handle,
            reason: Some(reason.to_string()),
        };
        send_broker_cancel(&ctx.socket_path, &cancel_req).await;
    }
}

/// Normalize the MCP `get_delegation_status` arguments into the wire `task_ids`
/// list. Reads the `task_ids` array, trims each entry, drops empty / whitespace
/// strings, and de-duplicates while preserving first-seen order. A non-string
/// entry violates the schema's `items: string` contract, so the whole call is
/// rejected (`Err`) instead of silently polling a subset — otherwise a malformed
/// `{"task_ids":[123,"abc"]}` would quietly resolve to just `abc`. `Ok(empty)`
/// means nothing usable was supplied (missing array, or all empty/whitespace);
/// the caller rejects both `Err` and `Ok(empty)` with `-32602`. Empty strings are
/// dropped (not rejected): `items` carries no `minLength`, so `""` satisfies the
/// schema and is treated as a formatting nicety. No upper bound on the count: a
/// fan-out can be arbitrarily wide.
fn normalize_status_task_ids(arguments: &Value) -> Result<Vec<String>, String> {
    let Some(arr) = arguments.get("task_ids").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for v in arr {
        let Some(s) = v.as_str() else {
            return Err(
                "get_delegation_status task_ids must contain only string task ids".to_string(),
            );
        };
        let trimmed = s.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

fn parse_artifact_files(arguments: &Value) -> Result<Vec<String>, String> {
    const MAX_FILES: usize = 100;
    const MAX_PATH_CHARS: usize = 4096;
    let files = arguments
        .get("files")
        .and_then(Value::as_array)
        .ok_or("present_task_files requires a non-empty files array of artifact references")?;
    if files.is_empty() {
        return Err(
            "present_task_files requires a non-empty files array of artifact references".into(),
        );
    }
    if files.len() > MAX_FILES {
        return Err(format!(
            "present_task_files accepts at most {MAX_FILES} artifacts"
        ));
    }
    let mut normalized = Vec::with_capacity(files.len());
    for value in files {
        let reference = value
            .as_str()
            .ok_or("present_task_files artifact references must be strings")?
            .trim();
        if reference.is_empty() {
            return Err("present_task_files artifact references must not be empty".into());
        }
        if reference.chars().count() > MAX_PATH_CHARS {
            return Err(format!(
                "present_task_files artifact references must be at most {MAX_PATH_CHARS} characters"
            ));
        }
        normalized.push(reference.to_string());
    }
    Ok(normalized)
}

pub fn render_artifacts_result(outcome: &Value) -> Value {
    let error = outcome.get("error").and_then(Value::as_str);
    let accepted = outcome
        .get("accepted")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let rejected = outcome
        .get("rejected")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let text = error.map_or_else(
        || format!("Presented {accepted} task artifact(s); rejected {rejected}."),
        |code| format!("Task artifact registration failed: {code}."),
    );
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": error.is_some(),
        "structuredContent": outcome.clone(),
    })
}

pub fn render_automation_result(outcome: &Value) -> Value {
    let is_error = outcome.get("error").is_some();
    let text = serde_json::to_string(outcome)
        .unwrap_or_else(|_| String::from("{\"error\":\"invalid response\"}"));
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
        "structuredContent": outcome.clone(),
    })
}

pub fn render_channel_result(outcome: &Value) -> Value {
    let is_error = outcome.get("error").is_some();
    let text = serde_json::to_string(outcome)
        .unwrap_or_else(|_| String::from("{\"error\":\"CHANNEL_RESULT_INVALID\"}"));
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
        "structuredContent": outcome.clone(),
    })
}

/// Render the `get_delegation_status` round-trip outcome (always a
/// `{ "tasks": [..] }` envelope from the broker) into an MCP `tools/call`
/// result. EVERY poll renders through [`render_batch_report`] — a single id and
/// a fan-out take the SAME path — so the shape the LLM and frontend see is
/// uniform: a `{ "tasks": [..] }` object with one entry per requested id (one
/// entry for a single id), each carrying its `task_id` + `status`. A bare report
/// with no `tasks` array (older / unexpected shape) is wrapped as a one-element
/// batch so the output stays uniform.
pub fn render_status_result(outcome: &Value) -> Value {
    match outcome.get("tasks").and_then(|v| v.as_array()) {
        Some(tasks) => render_batch_report(tasks),
        None => render_batch_report(std::slice::from_ref(outcome)),
    }
}

/// Render a `get_delegation_status` result as a `{ "tasks": [..] }` batch — the
/// single rendering path for every poll, whether it carries one report or many.
/// The `content` text is the compact `{ "tasks": [..] }` JSON so hosts that
/// persist only `CallToolResult.content` text (e.g. Claude Code) can still
/// recover every task; `structuredContent` carries the same shape for hosts that
/// keep it. `isError` is set only when EVERY task failed — a coarse signal (a
/// lone failed task therefore flags `isError`, matching the old single-report
/// behavior); the frontend derives per-task badges from the structured reports,
/// not from this flag.
fn render_batch_report(tasks: &[Value]) -> Value {
    let all_failed = !tasks.is_empty()
        && tasks
            .iter()
            .all(|t| t.get("status").and_then(|v| v.as_str()) == Some("failed"));
    let envelope = json!({ "tasks": tasks });
    let text = serde_json::to_string(&envelope).unwrap_or_else(|_| String::from("{\"tasks\":[]}"));
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": all_failed,
        "structuredContent": envelope,
    })
}

/// Map a serialized [`super::types::DelegationTaskReport`] into MCP `tools/call`
/// result content. Shared by `delegate_to_agent` and `cancel_delegation`, which
/// each resolve to a single report; `get_delegation_status` no longer uses this
/// path — it always renders via [`render_status_result`] / [`render_batch_report`].
/// Kept separate so unit tests can assert the mapping without a real socket.
///
/// The human-readable `content` text is the result for a `completed` task and
/// the `message` (status note / failure reason) otherwise. `isError` is set
/// ONLY for `failed` — `running` (ack), `canceled` (a successful cancel or a
/// canceled task), and `unknown` are all valid tool results the LLM should read
/// rather than treat as errors. The full report rides along in
/// `structuredContent` so the frontend can read `status` + the child ids.
/// Map the `check_user_feedback` round-trip outcome (a `{ count, feedback:[..] }`
/// envelope from the listener) into an MCP `tools/call` result.
///
/// The human-readable `content` text is the steering the LLM acts on: when
/// notes are present it frames them as high-priority user corrections and asks
/// the agent to adjust and acknowledge; when empty it says so plainly. The raw
/// envelope rides along in `structuredContent`. `isError` is always `false` — a
/// successful check with no feedback is a valid result, not an error.
pub fn render_feedback_result(outcome: &Value) -> Value {
    let count = outcome.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    let text = if count == 0 {
        "No new feedback from the user. Continue with your current plan.".to_string()
    } else {
        let mut s = format!(
            "The user sent {count} message(s) while you were working. Treat this as \
             high-priority steering: adjust your current approach to honor it now, and \
             briefly acknowledge what you changed.\n"
        );
        if let Some(notes) = outcome.get("feedback").and_then(|v| v.as_array()) {
            for (i, note) in notes.iter().enumerate() {
                let body = note.get("text").and_then(|v| v.as_str()).unwrap_or("");
                s.push_str(&format!("{}. {}\n", i + 1, body));
            }
        }
        s
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        // Rebuild the structured payload from count + feedback only — the
        // listener's internal `_commit_ids` must not leak to the agent's host.
        "structuredContent": {
            "count": count,
            "feedback": outcome.get("feedback").cloned().unwrap_or_else(|| json!([])),
        },
    })
}

/// Map the `ask_user_question` round-trip outcome (a `{ answers, declined }`
/// envelope from the listener) into an MCP `tools/call` result.
///
/// The human-readable `content` text reports the user's selections per question
/// so the agent can act on them; a declined / empty answer tells the agent to
/// proceed with its own judgment. The raw envelope rides along in
/// `structuredContent`. `isError` is always `false` — a declined question is a
/// valid result, not an error.
pub fn render_ask_result(outcome: &Value) -> Value {
    let declined = outcome
        .get("declined")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let answers = outcome
        .get("answers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let text = if declined || answers.is_empty() {
        "The user dismissed the question(s) without choosing an answer. Proceed \
         using your best judgment and reasonable defaults."
            .to_string()
    } else {
        let mut s = String::from("The user answered your question(s):\n");
        for (i, a) in answers.iter().enumerate() {
            let header = a.get("header").and_then(|v| v.as_str()).unwrap_or("");
            let question = a.get("question").and_then(|v| v.as_str()).unwrap_or("");
            let selected: Vec<&str> = a
                .get("selected")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            let joined = if selected.is_empty() {
                "(no selection)".to_string()
            } else {
                selected.join(", ")
            };
            s.push_str(&format!(
                "{}. [{header}] {question}\n   → {joined}\n",
                i + 1
            ));
        }
        s
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": { "answers": answers, "declined": declined },
    })
}

/// Extract the `session_id` integer from the `get_session_info` arguments,
/// tolerating a JSON number (int or whole float) or a numeric string — some MCP
/// hosts stringify integer args. `None` for missing / non-integer / out-of-range,
/// which the dispatcher maps to a synchronous `-32602` the LLM can fix.
fn parse_session_id(arguments: &Value) -> Option<i32> {
    let v = arguments.get("session_id")?;
    if let Some(n) = v.as_i64() {
        return i32::try_from(n).ok();
    }
    if let Some(f) = v.as_f64() {
        if f.fract() == 0.0 && f >= f64::from(i32::MIN) && f <= f64::from(i32::MAX) {
            return Some(f as i32);
        }
    }
    if let Some(s) = v.as_str() {
        return s.trim().parse::<i32>().ok();
    }
    None
}

/// Parse the optional `max_messages` tuning arg robustly: a JSON number (integer
/// or whole non-negative float) or a numeric string — consistent with how
/// `session_id` tolerates stringified ints. Clamps in `u64` space BEFORE narrowing
/// to `u32`, so a huge value (e.g. `4294967296`) saturates to the cap instead of
/// wrapping to a small number. An absent OR unparseable value falls back to the
/// default window — it is an optional knob, not a hard error — while an explicit
/// `0` (or `"0"`) is preserved to mean metadata-only.
fn parse_max_messages(arguments: &Value) -> u32 {
    const DEFAULT_MAX_MESSAGES: u32 = 20;
    let Some(v) = arguments.get("max_messages") else {
        return DEFAULT_MAX_MESSAGES;
    };
    let raw: Option<u64> = if let Some(n) = v.as_u64() {
        Some(n)
    } else if let Some(f) = v.as_f64() {
        // Reject negatives / fractions; `f as u64` saturates a huge float.
        (f.fract() == 0.0 && f >= 0.0).then_some(f as u64)
    } else if let Some(s) = v.as_str() {
        s.trim().parse::<u64>().ok()
    } else {
        None
    };
    match raw {
        Some(n) => n.min(u64::from(MAX_SESSION_MESSAGES)) as u32,
        None => DEFAULT_MAX_MESSAGES,
    }
}

/// Map the `get_session_info` round-trip outcome (a serialized
/// [`crate::acp::session_info::SessionInfo`]) into an MCP `tools/call` result. A
/// not-found result is surfaced as readable text with `isError: false` (the LLM
/// reads it and proceeds), never as a tool error. The full structured envelope
/// rides along in `structuredContent` for hosts that keep it.
pub fn render_session_result(outcome: &Value) -> Value {
    let found = outcome
        .get("found")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let text = if found {
        render_session_summary_text(outcome)
    } else {
        outcome
            .get("note")
            .and_then(|v| v.as_str())
            .unwrap_or("No matching session was found.")
            .to_string()
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}

/// Build the human-readable summary block for a found session: a metadata header
/// plus, when present, a "Recent messages" section.
fn render_session_summary_text(o: &Value) -> String {
    let s = |k: &str| o.get(k).and_then(|v| v.as_str());
    let id = o.get("session_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let agent = s("agent_type").unwrap_or("unknown");
    let mut out = format!("Session #{id} ({agent})\n");
    if let Some(t) = s("title") {
        out.push_str(&format!("Title: {t}\n"));
    }
    let mut meta: Vec<String> = Vec::new();
    if let Some(v) = s("status") {
        meta.push(format!("status: {v}"));
    }
    if let Some(v) = s("git_branch") {
        meta.push(format!("branch: {v}"));
    }
    if let Some(v) = s("model") {
        meta.push(format!("model: {v}"));
    }
    if !meta.is_empty() {
        out.push_str(&meta.join(" | "));
        out.push('\n');
    }
    if let Some(v) = s("workspace_path") {
        out.push_str(&format!("Workspace: {v}\n"));
    }
    if let Some(n) = o.get("message_count").and_then(|v| v.as_u64()) {
        out.push_str(&format!("Messages: {n}\n"));
    }
    if o.get("is_delegation_child")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        if let Some(p) = o.get("parent_id").and_then(|v| v.as_i64()) {
            out.push_str(&format!("Delegation child of session #{p}\n"));
        }
    }
    if let Some(tokens) = o
        .get("stats")
        .and_then(|st| st.get("total_tokens"))
        .and_then(|v| v.as_u64())
    {
        out.push_str(&format!("Total tokens: {tokens}\n"));
    }
    if let Some(note) = s("note") {
        out.push_str(&format!("Note: {note}\n"));
    }
    if let Some(messages) = o.get("messages") {
        let total = messages.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let included = messages
            .get("included")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let truncated = messages
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let suffix = if truncated {
            ", older turns omitted"
        } else {
            ""
        };
        out.push_str(&format!(
            "\nRecent messages ({included}/{total}{suffix}):\n"
        ));
        if let Some(items) = messages.get("items").and_then(|v| v.as_array()) {
            for item in items {
                let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("?");
                let body = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let tools: Vec<&str> = item
                    .get("tools")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                out.push_str(&format!("- [{role}] {body}"));
                if !tools.is_empty() {
                    out.push_str(&format!(" (tools: {})", tools.join(", ")));
                }
                out.push('\n');
            }
        }
    }
    out
}

pub fn render_task_report(report: &Value) -> Value {
    let status = report.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let is_error = status == "failed";
    let report_str = |key: &str| {
        report
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    };
    let text = if status == "completed" {
        // Prefer the result text; fall back to `message` so the DB-fallback note
        // ("Result no longer cached; open child session N…") for an evicted
        // result isn't rendered as empty content.
        report_str("text")
            .or_else(|| report_str("message"))
            .unwrap_or("")
            .to_string()
    } else {
        report_str("message")
            .or_else(|| report_str("text"))
            .unwrap_or("")
            .to_string()
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
        "structuredContent": report.clone(),
    })
}

/// Render the main process's append result into an MCP `CallToolResult`.
/// Authorization and persistence errors are regular tool errors so the Agent
/// can continue the turn without treating the MCP transport itself as broken.
pub fn render_memory_append_result(outcome: &Value) -> Value {
    if let Some(message) = outcome.get("error").and_then(Value::as_str) {
        let structured = normalized_memory_error(outcome, "memory_append_failed");
        return json!({
            "content": [{ "type": "text", "text": memory_error_text(message) }],
            "isError": true,
            "structuredContent": structured,
        });
    }
    let appended = outcome
        .get("appended")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = if appended {
        "User memory updated."
    } else {
        "This user memory was already recorded."
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}

fn memory_round_trip_result(
    result: std::io::Result<BrokerResponse>,
    operation: &'static str,
) -> std::io::Result<BrokerResponse> {
    Ok(match result {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(
                target: "user_memory",
                route = "mcp_companion",
                operation,
                error_code = "memory_transport_failed",
                retryable = true,
                error = %error,
                "memory broker transport failed after retry"
            );
            BrokerResponse {
                outcome: memory_error_outcome(
                    "memory_transport_failed",
                    format!("Memory transport failed after one retry: {error}"),
                    true,
                    None,
                ),
            }
        }
    })
}

fn memory_error_outcome(
    code: &str,
    message: impl Into<String>,
    retryable: bool,
    durable_changed: Option<bool>,
) -> Value {
    json!({
        "error": message.into(),
        "code": code,
        "retryable": retryable,
        "durableChanged": durable_changed,
        "fallback": "host_memory_action",
    })
}

fn normalized_memory_error(outcome: &Value, default_code: &str) -> Value {
    let mut structured = outcome.clone();
    let Some(fields) = structured.as_object_mut() else {
        return memory_error_outcome(default_code, "Memory operation failed.", false, Some(false));
    };
    fields.entry("code").or_insert_with(|| json!(default_code));
    fields.entry("retryable").or_insert_with(|| json!(false));
    fields
        .entry("durableChanged")
        .or_insert_with(|| json!(false));
    fields
        .entry("fallback")
        .or_insert_with(|| json!("host_memory_action"));
    structured
}

fn memory_error_text(message: &str) -> String {
    format!(
        "{message} No durable memory change was confirmed. Use the host Memory action on the source message if persistence is still needed."
    )
}

/// Render a bounded candidate-observation report without implying that the
/// candidate is already durable confirmed memory.
pub fn render_memory_proposal_result(outcome: &Value) -> Value {
    if let Some(message) = outcome.get("error").and_then(Value::as_str) {
        let structured = normalized_memory_error(outcome, "memory_proposal_failed");
        return json!({
            "content": [{ "type": "text", "text": memory_error_text(message) }],
            "isError": true,
            "structuredContent": structured,
        });
    }
    let added = outcome
        .get("observationAdded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let recommended = outcome
        .get("confirmationRecommended")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = match (added, recommended) {
        (true, true) => "Memory candidate observation recorded; user confirmation is recommended.",
        (true, false) => "Memory candidate observation recorded.",
        (false, _) => "No new memory candidate observation was recorded.",
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": outcome.clone(),
    })
}
