//! Wire format for `iyw-claw-mcp` companion ↔ main process round-trip over UDS
//! (Unix) or named pipe (Windows).
//!
//! The frame is dead simple: a little-endian `u32` byte length followed by
//! that many bytes of UTF-8 JSON. One request, one response — the companion
//! reopens the socket per `tools/call`. This trades a few extra connects for
//! a wire that's trivial to test and that doesn't need multiplexing
//! (a parent makes at most one delegation call at a time from the LLM's
//! perspective — the broker handles concurrency at a higher level).
//!
//! Why length-prefix instead of newline-delimited JSON? The LLM-issued
//! `task` arguments can contain newlines, and we'd rather avoid escaping
//! them into a single line. JSON-RPC over stdio uses newlines because
//! Content-Length headers add complexity; for an internal UDS we can do
//! better.
//!
//! ### Message shapes
//!
//! Inbound traffic is a tagged [`BrokerMessage`] enum, one variant per MCP
//! tool plus the MCP cancel notification:
//!   * `call` — [`BrokerRequest`] for `delegate_to_agent`; returns a
//!     [`BrokerResponse`] wrapping a `DelegationTaskReport` (a `Running` ack, or
//!     a terminal report).
//!   * `status` — [`BrokerStatusRequest`] for `get_delegation_status`. Carries a
//!     `task_ids` list (one or many) and an optional `wait_ms` long-poll —
//!     omitted is an immediate snapshot, an explicit `0` blocks until a task is
//!     terminal, a positive value is a bounded wait. Returns a `{ "tasks": [..] }`
//!     envelope with one task report per requested id (in request order); a
//!     batch wait wakes as soon as ANY requested task reaches a terminal state.
//!   * `cancel_task` — [`BrokerCancelTaskRequest`] for `cancel_delegation`;
//!     returns a task report.
//!   * `cancel` — fire-and-forget [`BrokerCancelRequest`] from MCP
//!     `notifications/cancelled`, targeting an in-flight `delegate_to_agent`
//!     call by `external_handle`; gets a `Value::Null` ack.
//!   * `browser` — authenticated shared-browser operations. The listener derives
//!     Agent identity from the launch token and cancels work on peer close.
//!   * `automation` — global scheduled-task CRUD shared by MCP and the host CLI.
//!
//! Session-scoped arms are authenticated by the same per-launch `token`.
//! `automation` is deliberately tokenless for terminal-only Agents.
//!
//! ### Version coupling
//!
//! The companion (`iyw-claw-mcp`) and the listener (inside the iyw-claw main
//! process) ship in the SAME release artifact — the Tauri bundle, the
//! server Docker image, and the standalone binary tree all install both
//! binaries at the same path. The MCP config pointing the agent CLI at
//! `iyw-claw-mcp` uses an absolute path that is replaced atomically by the
//! upgrade, so an old-version companion talking to a new-version listener
//! is not a supported configuration. Ordinary tool messages therefore omit
//! version fields. The authenticated readiness report is the sole exception:
//! it verifies the actual launched companion before memory tools are enabled.
//! Tagged-enum cutovers remain deliberately non-backward-compatible — a stale
//! companion should fail visibly rather than behave incorrectly.

use std::io;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::acp::automation_tools::ScheduledTaskRequest;
use crate::acp::question::QuestionSpec;

#[path = "backend.rs"]
pub mod backend;

pub const COMPANION_PROTOCOL_VERSION: u32 = 7;

const fn default_companion_protocol_version() -> u32 {
    COMPANION_PROTOCOL_VERSION
}

/// One delegation call's worth of input forwarded from the companion to the
/// main process. The main process re-validates `token` and maps
/// `parent_connection_id` to the live ACP connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerRequest {
    /// Shared secret minted by the main process when it spawned the agent CLI;
    /// the agent passes it through to the companion via `--token`. Rejects
    /// anything else.
    pub token: String,
    /// iyw-claw-internal ACP connection UUID for the parent session.
    pub parent_connection_id: String,
    /// The MCP `tool_use_id` for the LLM-issued `delegate_to_agent` call.
    /// Used to bind the eventual child outcome back to the parent's
    /// tool_use_id in the UI / DB.
    pub parent_tool_use_id: String,
    /// Opaque companion-minted token (one per `tools/call`). The broker
    /// keys its `cancel_by_external_handle` lookup off this value so an
    /// MCP-side `notifications/cancelled` can target this specific call.
    /// Older companions / tests can omit it; missing handles disable the
    /// cancel path for that call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_handle: Option<String>,
    /// Raw `arguments` JSON from the MCP `tools/call` request, schema-shaped
    /// per [`super::tool_schema_json`]. The main process re-parses into
    /// [`super::types::DelegationRequest`].
    pub input: Value,
}

/// Cancel an in-flight delegation by its companion-minted
/// `external_handle`. Sent fire-and-forget — the listener acknowledges by
/// writing an empty [`BrokerResponse`] so the companion can detect a
/// broken socket, but the response body carries no information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerCancelRequest {
    pub token: String,
    pub external_handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Query the status (and, optionally, block briefly for the result) of one or
/// more previously-issued delegation tasks by their broker `task_id`s. Backs the
/// `get_delegation_status` MCP tool. Authenticated by the same per-launch
/// `token`; the listener scopes each lookup to the token's parent connection
/// so one parent can't read another's tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerStatusRequest {
    pub token: String,
    /// One or many task ids to resolve. The companion forwards the MCP
    /// `task_ids` array into this list (trimmed, de-duplicated, order-preserving).
    /// The listener returns one report per id, in this order.
    pub task_ids: Vec<String>,
    /// How long the listener may block waiting for a task to reach a terminal
    /// state before returning the current (possibly still-running) snapshot.
    /// `None` (omitted) returns an immediate snapshot; an explicit `0` blocks
    /// with no timeout until a task finishes (long-running children); any
    /// positive value is a long-poll the listener clamps to a hard ceiling so a
    /// single bounded call can't hang unbounded. For a batch the wait resolves as
    /// soon as ANY requested task reaches a terminal state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_ms: Option<u64>,
}

/// Cancel a previously-issued delegation task by its broker `task_id`. Backs
/// the `cancel_delegation` MCP tool. Distinct from [`BrokerCancelRequest`],
/// which targets an in-flight `tools/call` by its companion-minted
/// `external_handle` for MCP `notifications/cancelled`; this targets a running
/// task the LLM is explicitly stopping by id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerCancelTaskRequest {
    pub token: String,
    pub task_id: String,
}

/// Pull the pending live-feedback notes for the parent session. Backs the
/// `check_user_feedback` MCP tool. Authenticated by the same per-launch
/// `token`; the listener resolves the parent connection from it and scopes the
/// drain to that connection so one parent can't read another's feedback.
/// Always returns an immediate snapshot — no blocking wait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerFeedbackRequest {
    pub token: String,
}

/// Confirm delivery of feedback notes, marking them `Delivered`. Sent by the
/// companion AFTER its `check_user_feedback` round-trip wins (i.e. it is
/// returning the result to the agent), NOT by the listener at UDS-write time —
/// so a per-request cancel that suppresses the agent-facing response (the agent
/// staying alive) leaves the notes pending for the next check (at-least-once).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerCommitFeedbackRequest {
    pub token: String,
    pub ids: Vec<String>,
}

/// Ask the user one or more multiple-choice questions and BLOCK until they
/// answer. Backs the `ask_user_question` MCP tool. Authenticated by the same
/// per-launch `token`; the listener resolves the parent connection from it,
/// registers the questions (broadcasting the card to every attached client),
/// and parks the response until the user answers (or the tool call is canceled,
/// detected via peer-close on this connection). The companion has already
/// validated the schema, so `questions` is well-formed and carries stable ids.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerAskRequest {
    pub token: String,
    pub questions: Vec<QuestionSpec>,
}

/// Resolve a session the user referenced (`iyw-claw://session/<id>`) into its
/// metadata + stats, optionally with its recent messages. Backs the
/// `get_session_info` MCP tool. Authenticated by the same per-launch `token`; the
/// lookup is by iyw-claw's internal conversation id (the number in the reference),
/// so — unlike the delegation arms — it is NOT scoped to the parent connection
/// (any non-deleted session the user references can be read).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerSessionRequest {
    pub token: String,
    /// iyw-claw's internal conversation PK (the number in `iyw-claw://session/<id>`).
    pub session_id: i32,
    /// How many of the most recent turns to include as compacted text. `None` /
    /// `0` → metadata only (no transcript parse); a positive value is clamped to
    /// [`crate::acp::session_info::MAX_SESSION_MESSAGES`] by the resolver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_messages: Option<u32>,
}

/// Read the current logged-in user's safe display profile. The listener owns
/// the account session and returns only an Agent-facing field allowlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerUserProfileRequest {
    pub token: String,
}

/// Append one Agent-proposed entry to the user's append-only memory document.
/// The listener derives the Agent type and write authorization from `token`; the
/// companion cannot choose a document, path, or identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerMemoryAppendRequest {
    pub token: String,
    pub content: String,
}

/// Submit one conservative memory candidate observation. The listener owns
/// Agent identity, opaque source, turn nonce, lifecycle state, and destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerMemoryProposalRequest {
    pub token: String,
    pub content: String,
    pub signal: crate::user_memory::UserMemoryCandidateSignal,
}

/// Read-only memory recall request. The listener derives scope and identity
/// from the launch token; the companion supplies only a bounded query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerMemoryRecallRequest {
    pub token: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Read the current contents of selected host-owned user memory documents.
/// The companion supplies document identifiers only; the listener owns the
/// root, policy, locking, and revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerMemoryDocumentsReadRequest {
    pub token: String,
    pub documents: Vec<crate::user_memory::UserMemoryDocumentId>,
}

/// Typed host-scoped memory administration request. Identity, workspace and
/// authorization are derived from the launch token by the listener.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerMemoryAdminRequest {
    pub token: String,
    pub tool: String,
    #[serde(default)]
    pub input: Value,
}

/// Register files produced by the current task. The listener resolves the
/// conversation from the authenticated companion token and owns validation
/// and persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerArtifactsRequest {
    pub token: String,
    pub files: Vec<String>,
}

/// Analyze one image through the authenticated parent session's current model.
/// The companion loads and validates bytes locally; the listener derives the
/// parent connection from `token`, so the model cannot choose a connection or
/// an internal visual model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerImageAnalysisRequest {
    pub token: String,
    pub data: String,
    pub mime_type: String,
    pub question: String,
    pub detail: String,
    pub image_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerChannelRequest {
    pub token: String,
    pub tool: String,
    #[serde(default)]
    pub input: Value,
}

/// Run one allow-listed shared-browser operation for the authenticated parent
/// Agent. The listener derives connection and conversation identity from the
/// launch token; the model cannot choose either scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerBrowserRequest {
    pub token: String,
    pub tool: String,
    #[serde(default)]
    pub input: Value,
}

/// Provider-neutral options accepted by the audio transcription MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioTranscriptionOptions {
    #[serde(default = "default_true")]
    pub punctuation: bool,
    #[serde(default = "default_true")]
    pub word_timestamps: bool,
    #[serde(default)]
    pub speaker_diarization: bool,
    #[serde(default)]
    pub channel_split: bool,
}

impl Default for AudioTranscriptionOptions {
    fn default() -> Self {
        Self {
            punctuation: true,
            word_timestamps: true,
            speaker_diarization: false,
            channel_split: false,
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioTranscriptionSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Submit one audio source through the authenticated host.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerAudioTranscriptionRequest {
    pub token: String,
    pub source: AudioTranscriptionSource,
    #[serde(default)]
    pub flash: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default)]
    pub options: AudioTranscriptionOptions,
}

/// Query one authenticated user's existing transcription job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerAudioTranscriptionQueryRequest {
    pub token: String,
    pub job_id: String,
}

/// Bounded Agent-facing report; internal candidate identifiers, provenance,
/// revision, timestamps, and paths stay host-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerMemoryProposalResult {
    pub observation_added: bool,
    pub status: crate::user_memory::UserMemoryCandidateStatus,
    pub observation_count: u32,
    pub confirmation_recommended: bool,
}

/// Confirm that this companion launch returned its actual `tools/list`
/// catalog to the parent Agent. The per-launch token authenticates the report;
/// wire protocol and required tools determine compatibility. Package version
/// is retained for diagnostics only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerCompanionReadyRequest {
    pub token: String,
    pub version: String,
    #[serde(default = "default_companion_protocol_version")]
    pub protocol_version: u32,
    pub tools: Vec<String>,
}

/// Tagged top-level message dispatched by the listener. Adding new variants
/// is the wire-stable way to grow the broker protocol without touching the
/// frame layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrokerMessage {
    Call(BrokerRequest),
    Cancel(BrokerCancelRequest),
    Status(BrokerStatusRequest),
    CancelTask(BrokerCancelTaskRequest),
    Feedback(BrokerFeedbackRequest),
    CommitFeedback(BrokerCommitFeedbackRequest),
    Ask(BrokerAskRequest),
    SessionInfo(BrokerSessionRequest),
    UserProfile(BrokerUserProfileRequest),
    MemoryAppend(BrokerMemoryAppendRequest),
    MemoryProposal(BrokerMemoryProposalRequest),
    MemoryRecall(BrokerMemoryRecallRequest),
    MemoryDocumentsRead(BrokerMemoryDocumentsReadRequest),
    MemoryAdmin(BrokerMemoryAdminRequest),
    Artifacts(BrokerArtifactsRequest),
    ImageAnalysis(BrokerImageAnalysisRequest),
    Channel(BrokerChannelRequest),
    Browser(BrokerBrowserRequest),
    AudioTranscription(BrokerAudioTranscriptionRequest),
    AudioTranscriptionQuery(BrokerAudioTranscriptionQueryRequest),
    Automation(ScheduledTaskRequest),
    CompanionReady(BrokerCompanionReadyRequest),
}

/// The wrapped outcome the main process returns over the same socket.
/// `outcome` is a serialized [`super::types::DelegationTaskReport`] for `Call`
/// / `CancelTask` messages, a `{ "tasks": [report, ...] }` envelope (one report
/// per requested id, in request order) for `Status`, and `Value::Null` for
/// `Cancel` acknowledgements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerResponse {
    pub outcome: Value,
}

/// Maximum allowed frame size, 32 MiB. This accommodates the Base64 expansion
/// of the 20 MiB image-analysis limit while still bounding peer allocations.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

/// Write one length-prefixed JSON frame.
pub async fn write_frame<W, T>(stream: &mut W, value: &T) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let bytes = serde_json::to_vec(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("encode: {e}")))?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame {} bytes exceeds cap {MAX_FRAME_BYTES}", bytes.len()),
        ));
    }
    let len: u32 = bytes
        .len()
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame > u32::MAX"))?;
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

/// Read one length-prefixed JSON frame. Rejects frames larger than
/// [`MAX_FRAME_BYTES`].
pub async fn read_frame<R, T>(stream: &mut R) -> io::Result<T>
where
    R: AsyncReadExt + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame {len} bytes exceeds cap {MAX_FRAME_BYTES}"),
        ));
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("decode: {e}")))
}

/// One-shot client round-trip: connect, write one [`BrokerMessage`], read the
/// response, drop the connection. The three public helpers below differ only
/// in which message they build, so the connect/write/read is shared here.
#[cfg(unix)]
pub(super) async fn message_round_trip(
    socket_path: &str,
    msg: &BrokerMessage,
) -> io::Result<BrokerResponse> {
    use tokio::net::UnixStream;
    let mut stream = UnixStream::connect(socket_path).await?;
    write_frame(&mut stream, msg).await?;
    read_frame(&mut stream).await
}

/// Windows path uses named pipes; the address format is `\\.\pipe\<name>`.
#[cfg(windows)]
pub(super) async fn message_round_trip(
    socket_path: &str,
    msg: &BrokerMessage,
) -> io::Result<BrokerResponse> {
    let mut stream = open_named_pipe_with_retry(socket_path)
        .await
        .map_err(|e| io::Error::other(format!("open pipe: {e}")))?;
    write_frame(&mut stream, msg).await?;
    read_frame(&mut stream).await
}

/// Dispatch a `delegate_to_agent` call and read back the broker's
/// [`super::types::DelegationTaskReport`] (a `Running` ack, or a terminal
/// report when the child finished during setup / setup failed).
pub async fn client_round_trip(
    socket_path: &str,
    req: &BrokerRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::Call(req.clone())).await
}

/// Dispatch a `get_delegation_status` query and read back the
/// `{ "tasks": [report, ...] }` envelope (one report per requested id, in
/// request order).
pub async fn client_status_round_trip(
    socket_path: &str,
    req: &BrokerStatusRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::Status(req.clone())).await
}

pub async fn client_channel_round_trip(
    socket_path: &str,
    req: &BrokerChannelRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::Channel(req.clone())).await
}

pub async fn client_browser_round_trip(
    socket_path: &str,
    req: &BrokerBrowserRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::Browser(req.clone())).await
}

/// Dispatch a `cancel_delegation` request and read back the task report.
pub async fn client_cancel_task_round_trip(
    socket_path: &str,
    req: &BrokerCancelTaskRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::CancelTask(req.clone())).await
}

/// Dispatch a `check_user_feedback` query and read back the
/// `{ "feedback": [..], "count": N }` envelope (the pending notes drained for
/// the parent session, possibly empty).
pub async fn client_feedback_round_trip(
    socket_path: &str,
    req: &BrokerFeedbackRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::Feedback(req.clone())).await
}

/// Confirm delivery of feedback notes (fire-and-forget). Reads the empty ack so
/// the listener can flush before the socket drops; the body carries nothing.
pub async fn client_commit_feedback(
    socket_path: &str,
    req: &BrokerCommitFeedbackRequest,
) -> io::Result<()> {
    let _ = message_round_trip(socket_path, &BrokerMessage::CommitFeedback(req.clone())).await?;
    Ok(())
}

/// Dispatch an `ask_user_question` request and BLOCK reading the response until
/// the user answers (or the question is canceled). The listener holds this
/// connection open for the whole wait — there is no `wait_ms`, the block is
/// inherent (waiting on a human). If the tool call is canceled, the companion
/// drops this future, closing the socket; the listener observes the peer-close
/// and tears the pending question down. Returns a `{ answers, declined }`
/// envelope.
pub async fn client_ask_round_trip(
    socket_path: &str,
    req: &BrokerAskRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::Ask(req.clone())).await
}

/// Dispatch a `get_session_info` request and read back the serialized
/// [`crate::acp::session_info::SessionInfo`] envelope (metadata + stats, and the
/// recent messages when `max_messages > 0`).
pub async fn client_session_round_trip(
    socket_path: &str,
    req: &BrokerSessionRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::SessionInfo(req.clone())).await
}

/// Append one durable user-memory entry through the authenticated main-process
/// service. The response is a serialized `UserMemoryAppendResult`, or an
/// `{ "error": ... }` envelope when the token is invalid or write-disabled.
pub async fn client_memory_append_round_trip(
    socket_path: &str,
    req: &BrokerMemoryAppendRequest,
) -> io::Result<BrokerResponse> {
    retry_memory_round_trip(
        socket_path,
        BrokerMessage::MemoryAppend(req.clone()),
        "append",
    )
    .await
}

/// Submit one candidate observation through the authenticated main-process
/// service. The response is a bounded [`BrokerMemoryProposalResult`] or an
/// `{ "error": ... }` envelope.
pub async fn client_memory_proposal_round_trip(
    socket_path: &str,
    req: &BrokerMemoryProposalRequest,
) -> io::Result<BrokerResponse> {
    retry_memory_round_trip(
        socket_path,
        BrokerMessage::MemoryProposal(req.clone()),
        "proposal",
    )
    .await
}

/// Query the host-owned current memory view through the authenticated
/// listener. This route is read-only and has its own capability gate.
pub async fn client_memory_recall_round_trip(
    socket_path: &str,
    req: &BrokerMemoryRecallRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::MemoryRecall(req.clone())).await
}

/// Read selected current user-memory documents through the authenticated
/// listener. This route is read-only and independent from recall ranking.
pub async fn client_memory_documents_read_round_trip(
    socket_path: &str,
    req: &BrokerMemoryDocumentsReadRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(
        socket_path,
        &BrokerMessage::MemoryDocumentsRead(req.clone()),
    )
    .await
}

/// Register task output files and read back per-file accepted/rejected results.
pub async fn client_artifacts_round_trip(
    socket_path: &str,
    req: &BrokerArtifactsRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::Artifacts(req.clone())).await
}

/// Analyze one validated image through the parent session's live model route.
pub async fn client_image_analysis_round_trip(
    socket_path: &str,
    req: &BrokerImageAnalysisRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::ImageAnalysis(req.clone())).await
}

/// Submit one audio file through the host's authenticated Fusion API route.
pub async fn client_audio_transcription_round_trip(
    socket_path: &str,
    req: &BrokerAudioTranscriptionRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::AudioTranscription(req.clone())).await
}

/// Query an existing audio transcription job through the host.
pub async fn client_audio_transcription_query_round_trip(
    socket_path: &str,
    req: &BrokerAudioTranscriptionQueryRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(
        socket_path,
        &BrokerMessage::AudioTranscriptionQuery(req.clone()),
    )
    .await
}

/// Execute one global scheduled-task CRUD request. This route deliberately has
/// no launch token so terminal-only Agents can use the same host service.
pub async fn client_automation_round_trip(
    socket_path: &str,
    req: &ScheduledTaskRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::Automation(req.clone())).await
}

/// Memory writes are content/turn-idempotent in `UserMemoryService`, so an
/// identical broker frame may be retried once when the first transport result
/// is unknown. No token or memory content is logged.
async fn retry_memory_round_trip(
    socket_path: &str,
    message: BrokerMessage,
    operation: &'static str,
) -> io::Result<BrokerResponse> {
    match message_round_trip(socket_path, &message).await {
        Ok(response) => Ok(response),
        Err(first_error) => {
            tracing::warn!(
                target: "user_memory",
                route = "mcp_companion",
                operation,
                error = %first_error,
                "memory broker transport failed; retrying identical request once"
            );
            message_round_trip(socket_path, &message).await
        }
    }
}

/// Report that the authenticated companion successfully wrote its
/// `tools/list` response to the Agent-facing stdio channel.
pub async fn client_companion_ready_round_trip(
    socket_path: &str,
    req: &BrokerCompanionReadyRequest,
) -> io::Result<BrokerResponse> {
    message_round_trip(socket_path, &BrokerMessage::CompanionReady(req.clone())).await
}

/// Total budget for `open()` retries on Windows named pipes. Has to be
/// short enough that it nests comfortably inside the companion's
/// `BROKER_CANCEL_BUDGET` (500 ms) — leaving ≥ 300 ms for the actual
/// write/read after the open lands.
#[cfg(windows)]
const PIPE_OPEN_RETRY_BUDGET: std::time::Duration = std::time::Duration::from_millis(200);

#[cfg(windows)]
const PIPE_OPEN_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(10);

/// Windows-only: `ClientOptions::open()` can fail with
/// `ERROR_PIPE_BUSY` (231) or `NotFound` during the brief window between
/// the listener accepting one connection and binding the next instance
/// (see `DelegationListener::run` on Windows). The companion has already
/// removed the inflight entry by the time it dispatches a cancel, so
/// dropping the cancel on a transient open failure would silently lose
/// it. Retry with small backoff inside a tight budget. Non-busy errors
/// (e.g. listener not running at all) propagate immediately.
#[cfg(windows)]
async fn open_named_pipe_with_retry(
    socket_path: &str,
) -> io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let attempt = async {
        loop {
            match ClientOptions::new().open(socket_path) {
                Ok(client) => return Ok::<_, io::Error>(client),
                Err(e) => {
                    let busy = e.raw_os_error() == Some(231);
                    let not_found = e.kind() == io::ErrorKind::NotFound;
                    if !(busy || not_found) {
                        return Err(e);
                    }
                    tokio::time::sleep(PIPE_OPEN_RETRY_DELAY).await;
                }
            }
        }
    };
    match tokio::time::timeout(PIPE_OPEN_RETRY_BUDGET, attempt).await {
        Ok(inner) => inner,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "named pipe open: retry budget exhausted",
        )),
    }
}

/// Fire-and-forget cancel: open a fresh socket, write a
/// `BrokerMessage::Cancel`, read the (always-empty) ack so the listener gets
/// a chance to flush its side before we drop, then close. Errors are
/// returned but generally treated as "best effort" by callers — a cancel
/// race that loses to a completed response is fine, the companion will
/// suppress the response per MCP spec either way.
#[cfg(unix)]
pub async fn client_cancel(socket_path: &str, req: &BrokerCancelRequest) -> io::Result<()> {
    use tokio::net::UnixStream;
    let mut stream = UnixStream::connect(socket_path).await?;
    let msg = BrokerMessage::Cancel(req.clone());
    write_frame(&mut stream, &msg).await?;
    // The listener writes an empty BrokerResponse so we can detect a broken
    // pipe; we don't care what's inside.
    let _: io::Result<BrokerResponse> = read_frame(&mut stream).await;
    Ok(())
}

#[cfg(windows)]
pub async fn client_cancel(socket_path: &str, req: &BrokerCancelRequest) -> io::Result<()> {
    let mut stream = open_named_pipe_with_retry(socket_path)
        .await
        .map_err(|e| io::Error::other(format!("open pipe: {e}")))?;
    let msg = BrokerMessage::Cancel(req.clone());
    write_frame(&mut stream, &msg).await?;
    let _: io::Result<BrokerResponse> = read_frame(&mut stream).await;
    Ok(())
}
