//! Main-process side of the `iyw-claw-mcp` round-trip: accept UDS / named-pipe
//! connections from companion processes, validate the per-launch token,
//! resolve the parent's current conversation, and hand off to the broker.
//!
//! The listener is intentionally tiny — most of the work (depth checking,
//! spawn lifecycle, timeout, cancellation) happens inside
//! [`DelegationBroker`]. The listener is the boundary between the wire and
//! the broker, plus the place where the per-launch token policy is enforced.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{watch, RwLock};

use crate::acp::automation_tools::AutomationAgentService;
use crate::acp::delegation::broker::{DelegationBroker, StatusWait};
use crate::acp::delegation::transport::{
    read_frame, write_frame, BrokerArtifactsRequest, BrokerAskRequest, BrokerCancelRequest,
    BrokerCancelTaskRequest, BrokerCommitFeedbackRequest, BrokerCompanionReadyRequest,
    BrokerFeedbackRequest, BrokerMemoryAppendRequest, BrokerMemoryProposalRequest,
    BrokerMemoryProposalResult, BrokerMessage, BrokerRequest, BrokerResponse, BrokerSessionRequest,
    BrokerStatusRequest, COMPANION_PROTOCOL_VERSION,
};
use crate::acp::delegation::types::{DelegationRequest, DelegationTaskReport, TaskStatus};
use crate::acp::feedback::{PendingFeedback, SessionFeedbackAccess};
use crate::acp::question::{QuestionOutcome, SessionQuestionAccess};
use crate::acp::session_info::{SessionInfo, SessionInfoAccess};
use crate::models::AgentType;
use crate::user_memory::{
    AgentMemoryAppend, AgentMemoryProposal, CandidateObservationSource, UserMemoryAppendResult,
    UserMemoryProposalResult, UserMemoryService, APPEND_USER_MEMORY_TOOL, PROPOSE_USER_MEMORY_TOOL,
};
use serde_json::Value;

/// Hard ceiling on a *positive* `get_delegation_status` long-poll, so a single
/// MCP tool call can't block the companion's round-trip unbounded. The child
/// keeps running past this; the LLM simply re-issues the wait. An explicit
/// `wait_ms = 0` opts out of the ceiling and blocks until the task is terminal.
const STATUS_WAIT_MAX_MS: u64 = 60_000;
const MAX_COMPANION_VERSION_CHARS: usize = 128;
const MAX_COMPANION_TOOL_NAME_CHARS: usize = 128;
const MAX_COMPANION_TOOLS: usize = 64;

/// Pluggable "what conversation is this parent currently in?" lookup. The
/// production impl wraps `ConnectionManager.get_state`; tests use an
/// in-memory map.
///
/// Kept as a trait so the listener can be unit-tested without spinning up a
/// real `ConnectionManager` or RwLock<SessionState>.
#[async_trait]
pub trait ParentSessionLookup: Send + Sync {
    async fn current_conversation_id(&self, parent_connection_id: &str) -> Option<i32>;
}

#[async_trait]
pub trait TaskArtifactAccess: Send + Sync {
    async fn register_task_artifacts(
        &self,
        conversation_id: i32,
        working_dir: &Path,
        files: Vec<String>,
    ) -> Value;
}

/// Per-launch token entry. Bound at MCP injection time and revoked on parent
/// connection teardown.
#[derive(Debug, Clone)]
pub struct TokenEntry {
    pub parent_connection_id: String,
    pub working_dir: PathBuf,
    /// Agent identity captured when the companion token was minted. Memory
    /// append requests never accept an Agent type from the LLM.
    pub agent_type: AgentType,
    /// Launch-snapshot authorization. Existing sessions retain this value until
    /// reconnect even if the user changes the policy in Settings.
    pub memory_write_enabled: bool,
    /// Independent launch-snapshot authorization for conservative proposals.
    pub memory_proposal_enabled: bool,
    /// Stable hash-derived provenance id; raw launch tokens are never persisted.
    pub opaque_source_id: String,
    /// Read-only authority for the current accepted turn nonce.
    pub memory_turn_tracker: Arc<crate::acp::memory_turn::MemoryTurnTracker>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionReadyReport {
    pub version: String,
    pub protocol_version: u32,
    pub tools: Vec<String>,
}

struct RegisteredToken {
    entry: TokenEntry,
    ready: watch::Sender<CompanionReadyState>,
}

struct CompanionReadyCandidate {
    ready: watch::Sender<CompanionReadyState>,
    parent_connection_id: String,
    version: String,
    protocol_version: u32,
    tools: Vec<String>,
    append_required: bool,
    proposal_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompanionReadyState {
    NotRequired,
    Pending,
    Ready(CompanionReadyReport),
    Disabled,
}

#[derive(Default)]
pub struct TokenRegistry {
    inner: RwLock<HashMap<String, RegisteredToken>>,
    listener_ready: AtomicBool,
}

impl TokenRegistry {
    pub async fn register(&self, token: String, entry: TokenEntry) {
        self.register_with_state(token, entry, CompanionReadyState::NotRequired)
            .await;
    }

    pub async fn register_companion(&self, token: String, entry: TokenEntry) {
        self.register_with_state(token, entry, CompanionReadyState::Pending)
            .await;
    }

    async fn register_with_state(
        &self,
        token: String,
        entry: TokenEntry,
        state: CompanionReadyState,
    ) {
        let (ready, _) = watch::channel(state);
        self.inner
            .write()
            .await
            .insert(token, RegisteredToken { entry, ready });
    }

    pub async fn revoke(&self, token: &str) {
        if let Some(registered) = self.inner.write().await.remove(token) {
            registered.entry.memory_turn_tracker.complete_turn();
        }
    }

    pub async fn lookup(&self, token: &str) -> Option<TokenEntry> {
        self.inner
            .read()
            .await
            .get(token)
            .map(effective_token_entry)
    }

    /// Drop every token whose `parent_connection_id` matches. Used on parent
    /// connection teardown so a leaked token can't be reused.
    pub async fn revoke_by_parent(&self, parent_connection_id: &str) {
        let mut map = self.inner.write().await;
        map.retain(|_, registered| {
            if registered.entry.parent_connection_id == parent_connection_id {
                registered.entry.memory_turn_tracker.complete_turn();
                false
            } else {
                true
            }
        });
    }

    pub async fn record_companion_ready(&self, request: BrokerCompanionReadyRequest) -> bool {
        let registered = self.inner.read().await.get(&request.token).map(|entry| {
            (
                entry.ready.clone(),
                entry.entry.parent_connection_id.clone(),
                entry.entry.memory_write_enabled,
                entry.entry.memory_proposal_enabled,
            )
        });
        let Some((ready, parent_connection_id, append_required, proposal_required)) = registered
        else {
            tracing::warn!("rejected companion readiness report with unknown token");
            return false;
        };
        CompanionReadyCandidate {
            ready,
            parent_connection_id,
            version: bounded_companion_version(&request.version),
            protocol_version: request.protocol_version,
            tools: bounded_companion_tools(request.tools),
            append_required,
            proposal_required,
        }
        .publish()
    }

    pub async fn wait_for_companion_ready(
        &self,
        token: &str,
        timeout: std::time::Duration,
    ) -> Option<CompanionReadyReport> {
        let mut receiver = self
            .inner
            .read()
            .await
            .get(token)
            .map(|registered| registered.ready.subscribe())?;
        if let Some(outcome) = companion_ready_outcome(&receiver.borrow()) {
            return outcome;
        }
        let waited = tokio::time::timeout(timeout, async move {
            loop {
                receiver.changed().await.ok()?;
                if let Some(outcome) = companion_ready_outcome(&receiver.borrow_and_update()) {
                    return outcome;
                }
            }
        })
        .await;
        match waited {
            Ok(outcome) => outcome,
            Err(_) => self.disable_pending_companion(token).await,
        }
    }

    async fn disable_pending_companion(&self, token: &str) -> Option<CompanionReadyReport> {
        let ready = self
            .inner
            .read()
            .await
            .get(token)
            .map(|registered| registered.ready.clone())?;
        ready.send_if_modified(|current| {
            if !matches!(current, CompanionReadyState::Pending) {
                return false;
            }
            *current = CompanionReadyState::Disabled;
            true
        });
        let outcome = match &*ready.borrow() {
            CompanionReadyState::Ready(report) => Some(report.clone()),
            _ => None,
        };
        outcome
    }

    pub fn listener_ready(&self) -> bool {
        self.listener_ready.load(Ordering::Acquire)
    }
}

impl CompanionReadyCandidate {
    fn publish(self) -> bool {
        if !self.protocol_compatible() || !self.required_tools_present() {
            disable_pending_ready(&self.ready);
            return false;
        }
        self.log_package_skew();
        let report = CompanionReadyReport {
            version: self.version,
            protocol_version: self.protocol_version,
            tools: self.tools,
        };
        self.ready.send_if_modified(|current| {
            if !matches!(current, CompanionReadyState::Pending) {
                return false;
            }
            *current = CompanionReadyState::Ready(report);
            true
        })
    }

    fn protocol_compatible(&self) -> bool {
        if self.protocol_version != COMPANION_PROTOCOL_VERSION {
            tracing::warn!(
                connection_id = %self.parent_connection_id,
                expected_protocol = COMPANION_PROTOCOL_VERSION,
                detected_protocol = self.protocol_version,
                detected_version = %self.version,
                advertised_tools = ?self.tools,
                "rejected protocol-incompatible companion readiness report"
            );
            return false;
        }
        true
    }

    fn required_tools_present(&self) -> bool {
        let missing_append = self.append_required
            && !self
                .tools
                .iter()
                .any(|tool| tool == APPEND_USER_MEMORY_TOOL);
        let missing_proposal = self.proposal_required
            && !self
                .tools
                .iter()
                .any(|tool| tool == PROPOSE_USER_MEMORY_TOOL);
        if missing_append || missing_proposal {
            tracing::warn!(
                connection_id = %self.parent_connection_id,
                protocol_version = self.protocol_version,
                detected_version = %self.version,
                append_required = self.append_required,
                proposal_required = self.proposal_required,
                advertised_tools = ?self.tools,
                "rejected companion readiness report missing authorized memory tools"
            );
            return false;
        }
        true
    }

    fn log_package_skew(&self) {
        if self.version != env!("CARGO_PKG_VERSION") {
            tracing::info!(
                connection_id = %self.parent_connection_id,
                expected_version = env!("CARGO_PKG_VERSION"),
                detected_version = %self.version,
                protocol_version = self.protocol_version,
                "accepted package-version-skewed companion via compatible protocol"
            );
        }
    }
}

fn bounded_companion_version(version: &str) -> String {
    version.chars().take(MAX_COMPANION_VERSION_CHARS).collect()
}

fn disable_pending_ready(ready: &watch::Sender<CompanionReadyState>) {
    ready.send_if_modified(|current| {
        if !matches!(current, CompanionReadyState::Pending) {
            return false;
        }
        *current = CompanionReadyState::Disabled;
        true
    });
}

fn bounded_companion_tools(tools: Vec<String>) -> Vec<String> {
    let mut tools = tools
        .into_iter()
        .filter(|tool| !tool.trim().is_empty())
        .map(|tool| tool.chars().take(MAX_COMPANION_TOOL_NAME_CHARS).collect())
        .collect::<Vec<String>>();
    tools.sort();
    tools.dedup();
    tools.truncate(MAX_COMPANION_TOOLS);
    tools
}

fn effective_token_entry(registered: &RegisteredToken) -> TokenEntry {
    let mut entry = registered.entry.clone();
    match &*registered.ready.borrow() {
        CompanionReadyState::NotRequired => {}
        CompanionReadyState::Ready(report) => {
            entry.memory_write_enabled &= report
                .tools
                .iter()
                .any(|tool| tool == APPEND_USER_MEMORY_TOOL);
            entry.memory_proposal_enabled &= report
                .tools
                .iter()
                .any(|tool| tool == PROPOSE_USER_MEMORY_TOOL);
        }
        CompanionReadyState::Pending | CompanionReadyState::Disabled => {
            entry.memory_write_enabled = false;
            entry.memory_proposal_enabled = false;
        }
    }
    entry
}

fn companion_ready_outcome(state: &CompanionReadyState) -> Option<Option<CompanionReadyReport>> {
    match state {
        CompanionReadyState::Pending => None,
        CompanionReadyState::Ready(report) => Some(Some(report.clone())),
        CompanionReadyState::NotRequired | CompanionReadyState::Disabled => Some(None),
    }
}

fn log_memory_unavailable(operation: &str, reason: &str, content_chars: usize) {
    tracing::warn!(
        target: "user_memory",
        route = "mcp_companion",
        operation,
        error_code = "memory_session_unavailable",
        reason,
        content_chars,
        "agent memory route unavailable"
    );
}

fn log_memory_append_result(
    entry: &TokenEntry,
    content_chars: usize,
    result: &Result<UserMemoryAppendResult, crate::app_error::AppCommandError>,
) {
    match result {
        Ok(value) => tracing::info!(
            target: "user_memory",
            route = "mcp_companion",
            operation = "append",
            connection_id = %entry.parent_connection_id,
            agent_type = ?entry.agent_type,
            content_chars,
            appended = value.appended,
            "agent memory append completed"
        ),
        Err(error) => tracing::warn!(
            target: "user_memory",
            route = "mcp_companion",
            operation = "append",
            connection_id = %entry.parent_connection_id,
            agent_type = ?entry.agent_type,
            content_chars,
            error_code = ?error.code,
            error = %error,
            "agent memory append failed"
        ),
    }
}

fn log_memory_proposal_result(
    entry: &TokenEntry,
    content_chars: usize,
    result: &Result<UserMemoryProposalResult, crate::app_error::AppCommandError>,
) {
    match result {
        Ok(value) => tracing::info!(
            target: "user_memory",
            route = "mcp_companion",
            operation = "proposal",
            connection_id = %entry.parent_connection_id,
            agent_type = ?entry.agent_type,
            content_chars,
            observation_added = value.observation_added,
            confirmation_recommended = value.confirmation_recommended,
            "agent memory proposal completed"
        ),
        Err(error) => tracing::warn!(
            target: "user_memory",
            route = "mcp_companion",
            operation = "proposal",
            connection_id = %entry.parent_connection_id,
            agent_type = ?entry.agent_type,
            content_chars,
            error_code = ?error.code,
            error = %error,
            "agent memory proposal failed"
        ),
    }
}

struct ListenerReadinessGuard {
    tokens: Arc<TokenRegistry>,
}

impl ListenerReadinessGuard {
    fn new(tokens: Arc<TokenRegistry>) -> Self {
        tokens.listener_ready.store(true, Ordering::Release);
        Self { tokens }
    }
}

impl Drop for ListenerReadinessGuard {
    fn drop(&mut self) {
        self.tokens.listener_ready.store(false, Ordering::Release);
    }
}

pub struct DelegationListener {
    pub broker: Arc<DelegationBroker>,
    pub tokens: Arc<TokenRegistry>,
    pub parent_lookup: Arc<dyn ParentSessionLookup>,
    /// Pulls pending live-feedback notes for the `check_user_feedback` tool.
    /// Shares the same `tokens` registry and parent-connection scoping as the
    /// delegation arms — one companion, one socket, two features.
    pub feedback: Arc<dyn SessionFeedbackAccess>,
    /// Registers / cancels the blocking `ask_user_question` tool's pending
    /// questions. Same `tokens` registry and parent-connection scoping.
    pub questions: Arc<dyn SessionQuestionAccess>,
    /// Resolves a referenced session for the `get_session_info` tool. Unlike the
    /// other arms this is NOT parent-scoped — it looks any non-deleted session up
    /// by its iyw-claw conversation id (still token-gated against an invalid caller).
    pub session_info: Arc<dyn SessionInfoAccess>,
    /// Backend-owned memory store shared with Settings and prompt snapshots.
    pub user_memory: Arc<UserMemoryService>,
    pub artifacts: Arc<dyn TaskArtifactAccess>,
    /// Global scheduled-task CRUD service. Its CLI route is intentionally
    /// tokenless because every local Agent already has terminal authority.
    pub automation: Arc<AutomationAgentService>,
}

impl DelegationListener {
    fn log_connection_failure(error: &std::io::Error) {
        let peer_closed = matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::UnexpectedEof
        ) || (cfg!(windows) && error.raw_os_error() == Some(232));
        if peer_closed {
            tracing::debug!(
                error = %error,
                kind = ?error.kind(),
                "[delegation] peer closed connection"
            );
        } else {
            tracing::error!(
                error = %error,
                kind = ?error.kind(),
                "[delegation] connection failed"
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        broker: Arc<DelegationBroker>,
        tokens: Arc<TokenRegistry>,
        parent_lookup: Arc<dyn ParentSessionLookup>,
        feedback: Arc<dyn SessionFeedbackAccess>,
        questions: Arc<dyn SessionQuestionAccess>,
        session_info: Arc<dyn SessionInfoAccess>,
        user_memory: Arc<UserMemoryService>,
        artifacts: Arc<dyn TaskArtifactAccess>,
        automation: Arc<AutomationAgentService>,
    ) -> Arc<Self> {
        Arc::new(Self {
            broker,
            tokens,
            parent_lookup,
            feedback,
            questions,
            session_info,
            user_memory,
            artifacts,
            automation,
        })
    }

    /// Run the accept loop until the socket is unbound. Errors on accept are
    /// logged and the loop continues — a single bad connection can't bring
    /// down the listener.
    #[cfg(unix)]
    pub async fn run(self: Arc<Self>, socket_path: PathBuf) -> std::io::Result<()> {
        let _ = tokio::fs::remove_file(&socket_path).await;
        if let Some(parent) = socket_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let listener = tokio::net::UnixListener::bind(&socket_path)?;
        let _readiness = ListenerReadinessGuard::new(self.tokens.clone());
        tracing::info!("[delegation] listening on UDS {}", socket_path.display());
        loop {
            match listener.accept().await {
                Ok((mut conn, _)) => {
                    let me = Arc::clone(&self);
                    tokio::spawn(async move {
                        if let Err(e) = me.serve_one(&mut conn).await {
                            Self::log_connection_failure(&e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("[delegation] accept failed: {e}");
                    // Brief backoff so a persistent accept error doesn't pin a core.
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Windows variant: bind a named pipe and follow Tokio's recommended
    /// accept pattern — wait for a connect, immediately create the *next*
    /// server instance, then hand the connected instance off to a worker.
    /// This keeps a pipe instance available at all times, so clients calling
    /// `ClientOptions::open()` between connections don't see `NotFound`.
    #[cfg(windows)]
    pub async fn run(self: Arc<Self>, socket_path: PathBuf) -> std::io::Result<()> {
        use tokio::net::windows::named_pipe::ServerOptions;
        let path_str = socket_path.to_string_lossy().to_string();
        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&path_str)?;
        let _readiness = ListenerReadinessGuard::new(self.tokens.clone());
        tracing::info!("[delegation] listening on named pipe {path_str}");
        loop {
            if let Err(e) = server.connect().await {
                tracing::error!("[delegation] connect failed: {e}");
                // Re-create the instance so the next iteration has a fresh
                // listener; a failed connect leaves the current one unusable.
                server = ServerOptions::new().create(&path_str)?;
                continue;
            }
            let connected = server;
            // Re-bind BEFORE serving the current client, so a client that
            // opens during this turn finds a server instance to connect to.
            server = ServerOptions::new().create(&path_str)?;
            let me = Arc::clone(&self);
            tokio::spawn(async move {
                let mut conn = connected;
                if let Err(e) = me.serve_one(&mut conn).await {
                    Self::log_connection_failure(&e);
                }
            });
        }
    }

    /// Stream-generic per-connection handler. Exposed so unit tests can drive
    /// it over `tokio::io::duplex` instead of a real socket.
    pub async fn serve_one<C>(&self, conn: &mut C) -> std::io::Result<()>
    where
        C: AsyncReadExt + AsyncWriteExt + Unpin,
    {
        let msg: BrokerMessage = read_frame(conn).await?;
        let resp = match msg {
            BrokerMessage::Call(req) => report_response(self.process(req).await)?,
            BrokerMessage::Status(req) => {
                // A status long-poll — especially `wait_ms = 0` (block until
                // terminal) — can park for the whole lifetime of the child.
                // Race it against peer-close on this one-shot connection so a
                // companion that cancels and drops the request socket doesn't
                // leave this task parked until the task happens to finish. A
                // status query has no side effects (unlike a delegation), so
                // abandoning the wait is safe and there's nothing to cancel
                // broker-side. The companion never writes a second frame on
                // this socket, so the probe read only resolves on EOF/error.
                let status_fut = self.process_status(req);
                tokio::pin!(status_fut);
                let mut probe = [0u8; 1];
                let reports = tokio::select! {
                    biased;
                    reports = &mut status_fut => reports,
                    _ = conn.read(&mut probe) => return Ok(()),
                };
                reports_response(reports)?
            }
            BrokerMessage::CancelTask(req) => report_response(self.process_cancel_task(req).await)?,
            BrokerMessage::Feedback(req) => {
                // at-least-once delivery: READ pending notes (no mutation),
                // WRITE the response, and COMMIT them delivered ONLY on a
                // successful write. A dropped/failed write skips the commit, so
                // the notes stay pending for the agent's next check.
                match self.feedback_target(&req).await {
                    None => {
                        // Invalid token: return an empty envelope (no leak of
                        // whether any feedback exists), nothing to commit.
                        write_frame(conn, &feedback_response(&[])?).await?;
                    }
                    Some(parent_conn_id) => {
                        let pending = self.feedback.read_pending_feedback(&parent_conn_id).await;
                        // Read-only: the response carries the note ids
                        // (`_commit_ids`); delivery is committed LATER, by the
                        // companion's `CommitFeedback` once it actually returns
                        // the result to the agent. So a cancel that suppresses
                        // the agent-facing response leaves the notes pending.
                        write_frame(conn, &feedback_response(&pending)?).await?;
                    }
                }
                return Ok(());
            }
            BrokerMessage::CommitFeedback(req) => {
                self.process_commit_feedback(req).await;
                // Empty ack so the companion can confirm the listener saw it.
                BrokerResponse {
                    outcome: Value::Null,
                }
            }
            BrokerMessage::Ask(req) => {
                // Register the question (broadcasting the card) and park until
                // the user answers — racing peer-close exactly like `Status`.
                // The companion holds this connection open for the whole wait
                // and never writes a second frame, so the probe read only
                // resolves on EOF/error; a canceled tool call drops the
                // companion's future, closing this socket, which we observe and
                // tear the pending question down. An invalid token, a gone
                // connection, or a connection that already has a pending ask
                // (one-at-a-time) yields a `declined` outcome (the LLM proceeds
                // with its own judgment) rather than hanging.
                let Some(parent_conn_id) = self.ask_target(&req).await else {
                    write_frame(conn, &ask_declined_response()?).await?;
                    return Ok(());
                };
                let Some(reg) = self
                    .questions
                    .register_question(&parent_conn_id, req.questions)
                    .await
                else {
                    write_frame(conn, &ask_declined_response()?).await?;
                    return Ok(());
                };
                let question_id = reg.question_id;
                let mut answer_rx = reg.answer_rx;
                // Close the teardown race: `ask_target` validated the token, but the
                // parent connection may have been revoked + swept
                // (`cancel_questions_by_parent`) in the window before the insert
                // above — the sweep would have missed this just-registered entry,
                // leaving it parked until peer-close. The token is revoked before
                // the sweep, so a re-check that now finds it gone means teardown is
                // underway: cancel immediately so the ask can't linger.
                if self.tokens.lookup(&req.token).await.is_none() {
                    self.questions
                        .cancel_question(&parent_conn_id, &question_id)
                        .await;
                    write_frame(conn, &ask_declined_response()?).await?;
                    return Ok(());
                }
                let mut probe = [0u8; 1];
                let outcome = tokio::select! {
                    biased;
                    ans = &mut answer_rx => ans.ok(),
                    _ = conn.read(&mut probe) => {
                        self.questions
                            .cancel_question(&parent_conn_id, &question_id)
                            .await;
                        return Ok(());
                    }
                };
                let resp = match outcome {
                    Some(o) => ask_response(&o)?,
                    // Sender dropped without sending (connection teardown drain):
                    // surface a declined outcome so the tool returns cleanly.
                    None => ask_declined_response()?,
                };
                write_frame(conn, &resp).await?;
                return Ok(());
            }
            BrokerMessage::SessionInfo(req) => {
                // Read-only resolution (DB + a bounded transcript parse). No
                // peer-close race needed: unlike Status/Ask this never blocks on
                // a long-poll or a human — the bounded parse always completes —
                // and there is nothing to tear down on cancel.
                session_response(self.process_session_info(req).await)?
            }
            BrokerMessage::MemoryAppend(req) => {
                memory_append_response(self.process_memory_append(req).await)?
            }
            BrokerMessage::MemoryProposal(req) => {
                memory_proposal_response(self.process_memory_proposal(req).await)?
            }
            BrokerMessage::Artifacts(req) => BrokerResponse {
                outcome: self.process_artifacts(req).await,
            },
            BrokerMessage::Automation(req) => BrokerResponse {
                outcome: self
                    .automation
                    .execute(req)
                    .await
                    .unwrap_or_else(|error| serde_json::json!({ "error": error })),
            },
            BrokerMessage::CompanionReady(req) => {
                self.tokens.record_companion_ready(req).await;
                BrokerResponse {
                    outcome: Value::Null,
                }
            }
            BrokerMessage::Cancel(cancel) => {
                self.process_cancel(cancel).await;
                // Empty ack — the companion only uses this to detect the
                // listener has at least seen the cancel before dropping.
                BrokerResponse {
                    outcome: Value::Null,
                }
            }
        };
        write_frame(conn, &resp).await?;
        Ok(())
    }

    /// Validate the token, resolve the caller's parent connection/conversation,
    /// and query the status of every requested task id (optionally blocking per
    /// the wire `wait_ms`: omitted → immediate snapshot, explicit `0` → block
    /// until a task is terminal, a positive value → bounded long-poll clamped to
    /// [`STATUS_WAIT_MAX_MS`]). Backs the `get_delegation_status` tool. Returns
    /// one report per requested id, in request order. An invalid token reports
    /// `Unknown` for each id — the caller can't usefully distinguish it from a
    /// genuinely unknown task, and we don't leak which.
    async fn process_status(&self, req: BrokerStatusRequest) -> Vec<DelegationTaskReport> {
        let Some(entry) = self.tokens.lookup(&req.token).await else {
            return req.task_ids.iter().map(|id| unknown_report(id)).collect();
        };
        let parent_conversation_id = self
            .parent_lookup
            .current_conversation_id(&entry.parent_connection_id)
            .await;
        // Map the wire `wait_ms` to a wait mode: omitted → immediate poll, an
        // explicit `0` → block with no timeout (long-running children), any
        // positive value → bounded long-poll clamped to the hard ceiling.
        let wait = match req.wait_ms {
            None => StatusWait::Immediate,
            Some(0) => StatusWait::Infinite,
            Some(ms) => StatusWait::Bounded(ms.min(STATUS_WAIT_MAX_MS)),
        };
        self.broker
            .get_tasks_status(
                &entry.parent_connection_id,
                parent_conversation_id,
                &req.task_ids,
                wait,
            )
            .await
    }

    /// Validate the token, resolve the caller's parent, and cancel the task.
    /// Backs the `cancel_delegation` tool.
    async fn process_cancel_task(&self, req: BrokerCancelTaskRequest) -> DelegationTaskReport {
        let Some(entry) = self.tokens.lookup(&req.token).await else {
            return unknown_report(&req.task_id);
        };
        let parent_conversation_id = self
            .parent_lookup
            .current_conversation_id(&entry.parent_connection_id)
            .await;
        self.broker
            .cancel_task_by_id(
                &entry.parent_connection_id,
                parent_conversation_id,
                &req.task_id,
            )
            .await
    }

    /// Validate the token and resolve the `check_user_feedback` target: the
    /// caller's parent connection id. `None` on an invalid token — the LLM can't
    /// usefully distinguish "no notes" from "bad token", and we don't leak which.
    async fn feedback_target(&self, req: &BrokerFeedbackRequest) -> Option<String> {
        let entry = self.tokens.lookup(&req.token).await?;
        Some(entry.parent_connection_id)
    }

    /// Validate the token and resolve the `ask_user_question` target: the
    /// caller's parent connection id. `None` on an invalid token — the LLM gets
    /// a `declined` outcome (proceed with judgment), and we don't leak which.
    async fn ask_target(&self, req: &BrokerAskRequest) -> Option<String> {
        let entry = self.tokens.lookup(&req.token).await?;
        Some(entry.parent_connection_id)
    }

    /// Mark the named feedback notes delivered, after the companion confirms it
    /// returned them to the agent. Token-scoped to the parent connection. Unknown
    /// tokens are dropped (no LLM on the receiving end to react).
    async fn process_commit_feedback(&self, req: BrokerCommitFeedbackRequest) {
        let Some(entry) = self.tokens.lookup(&req.token).await else {
            return;
        };
        self.feedback
            .commit_feedback_delivered(&entry.parent_connection_id, req.ids)
            .await;
    }

    /// Validate token + dispatch cancel to the broker. Unknown tokens and
    /// parent-mismatched cancels are silently dropped — there's no LLM on
    /// the receiving end of this method to react to errors.
    async fn process_cancel(&self, cancel: BrokerCancelRequest) {
        let Some(_entry) = self.tokens.lookup(&cancel.token).await else {
            return;
        };
        let reason = cancel
            .reason
            .unwrap_or_else(|| "mcp client canceled".into());
        self.broker
            .cancel_by_external_handle(&cancel.external_handle, reason)
            .await;
    }

    /// Validate the token and resolve the `get_session_info` target. An invalid
    /// token yields a `found:false` outcome (the LLM can't usefully distinguish it
    /// from a deleted session, and we don't leak which).
    ///
    /// SCOPE (deliberate, user-confirmed): the lookup is by iyw-claw conversation id
    /// and is intentionally NOT scoped to the caller's parent connection or to the
    /// session ids actually referenced in the prompt — any non-deleted session
    /// resolves. This is sound in iyw-claw's single-tenant trust model: there is no
    /// per-user isolation anywhere (desktop is one local user; server mode shares
    /// one `IYW_CLAW_TOKEN` + one data dir across an operator's devices), the user can
    /// already open every session in the UI, and the agent already has full
    /// filesystem access to every agent's raw session files via its own tools — so
    /// reading session metadata by id is strictly less capability than the agent
    /// already holds, not an escalation. The token gate above still prevents an
    /// unrelated process from reaching the broker at all.
    async fn process_session_info(&self, req: BrokerSessionRequest) -> SessionInfo {
        if self.tokens.lookup(&req.token).await.is_none() {
            return SessionInfo::not_found(req.session_id);
        }
        self.session_info
            .resolve(req.session_id, req.max_messages.unwrap_or(0))
            .await
    }

    /// Authenticate the launch token and enforce the connection's captured
    /// memory-write permission before calling the append-only backend service.
    async fn process_memory_append(
        &self,
        req: BrokerMemoryAppendRequest,
    ) -> Result<UserMemoryAppendResult, String> {
        let content_chars = req.content.chars().count();
        let Some(entry) = self.tokens.lookup(&req.token).await else {
            log_memory_unavailable("append", "unknown_token", content_chars);
            return Err("User memory update is unavailable for this session.".into());
        };
        if !entry.memory_write_enabled {
            log_memory_unavailable("append", "capability_disabled", content_chars);
            return Err("User memory update is unavailable for this session.".into());
        }
        let result = self
            .user_memory
            .append_agent_memory_authorized(AgentMemoryAppend {
                content: req.content,
                agent_type: entry.agent_type,
            })
            .await;
        log_memory_append_result(&entry, content_chars, &result);
        result.map_err(|error| error.message)
    }

    /// Authenticate proposal capability and derive all provenance from the
    /// launch token entry. The companion supplies only content and signal.
    async fn process_memory_proposal(
        &self,
        req: BrokerMemoryProposalRequest,
    ) -> Result<BrokerMemoryProposalResult, String> {
        let unavailable = || "User memory proposal is unavailable for this session.".to_string();
        let content_chars = req.content.chars().count();
        let entry = self.tokens.lookup(&req.token).await.ok_or_else(|| {
            log_memory_unavailable("proposal", "unknown_token", content_chars);
            unavailable()
        })?;
        if !entry.memory_proposal_enabled {
            log_memory_unavailable("proposal", "capability_disabled", content_chars);
            return Err(unavailable());
        }
        let turn_nonce = entry.memory_turn_tracker.active_nonce().ok_or_else(|| {
            log_memory_unavailable("proposal", "turn_inactive", content_chars);
            unavailable()
        })?;
        let turn_tracker = entry.memory_turn_tracker.clone();
        let result = self
            .user_memory
            .propose_agent_memory_authorized_with_lease(
                AgentMemoryProposal {
                    content: req.content,
                    signal: req.signal,
                },
                CandidateObservationSource {
                    agent_type: entry.agent_type,
                    opaque_source_id: entry.opaque_source_id.clone(),
                    turn_nonce,
                },
                move || turn_tracker.acquire_commit_lease(turn_nonce),
            )
            .await;
        log_memory_proposal_result(&entry, content_chars, &result);
        let result = result.map_err(|error| error.message)?;
        Ok(BrokerMemoryProposalResult {
            observation_added: result.observation_added,
            status: result.candidate.status,
            observation_count: result.candidate.observation_count,
            confirmation_recommended: result.confirmation_recommended,
        })
    }

    async fn process_artifacts(&self, req: BrokerArtifactsRequest) -> Value {
        let Some(entry) = self.tokens.lookup(&req.token).await else {
            return serde_json::json!({
                "accepted": [],
                "rejected": req.files.into_iter().map(|path| serde_json::json!({
                    "path": path,
                    "reason": "invalid_session"
                })).collect::<Vec<_>>()
            });
        };
        let Some(conversation_id) = self
            .parent_lookup
            .current_conversation_id(&entry.parent_connection_id)
            .await
        else {
            return serde_json::json!({
                "accepted": [],
                "rejected": req.files.into_iter().map(|path| serde_json::json!({
                    "path": path,
                    "reason": "session_not_ready"
                })).collect::<Vec<_>>()
            });
        };
        self.artifacts
            .register_task_artifacts(conversation_id, &entry.working_dir, req.files)
            .await
    }
    async fn process(&self, req: BrokerRequest) -> DelegationTaskReport {
        // 1. Token + parent_connection_id consistency check. Treat both as
        //    "canceled" since the LLM can't usefully react to either —
        //    the parent has either been torn down or is impersonating.
        let entry = match self.tokens.lookup(&req.token).await {
            Some(e) => e,
            None => return cancel("invalid token"),
        };
        if entry.parent_connection_id != req.parent_connection_id {
            return cancel("token does not match parent connection");
        }

        // 2. Resolve the parent's current conversation. Without one the
        //    broker can't link the child row to the parent.
        let parent_conversation_id = match self
            .parent_lookup
            .current_conversation_id(&req.parent_connection_id)
            .await
        {
            Some(id) => id,
            None => return cancel("parent has no active conversation"),
        };

        // 3. Parse the delegate_to_agent arguments. Schema validation lives
        //    on the LLM side; we only enforce what the broker can't.
        let agent_type = match req.input.get("agent_type").and_then(|v| v.as_str()) {
            Some(raw) => match parse_agent_type(raw) {
                Some(t) => t,
                None => return invalid_agent_type(raw),
            },
            None => return invalid_agent_type(""),
        };
        let task = match req.input.get("task").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => {
                return report_failed("invalid_working_dir", "missing or empty task");
            }
        };
        // The `working_dir` the LLM explicitly passed (before defaulting),
        // used by the broker's correlation key. `None` when omitted —
        // symmetric with the ACP `raw_input`, which also omits it then.
        let requested_working_dir = req
            .input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let working_dir = requested_working_dir
            .clone()
            .or_else(|| Some(entry.working_dir.to_string_lossy().to_string()));

        let delegation_req = DelegationRequest {
            parent_connection_id: req.parent_connection_id,
            parent_conversation_id,
            parent_tool_use_id: req.parent_tool_use_id,
            agent_type,
            task,
            working_dir,
            requested_working_dir,
            external_handle: req.external_handle,
        };
        self.broker.start_delegation(delegation_req).await
    }
}

/// Serialize a [`DelegationTaskReport`] into a [`BrokerResponse`] for the wire.
/// Used by the `Call` / `CancelTask` arms, which each resolve to one report.
fn report_response(report: DelegationTaskReport) -> std::io::Result<BrokerResponse> {
    Ok(BrokerResponse {
        outcome: serde_json::to_value(&report).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode: {e}"))
        })?,
    })
}

/// Serialize a batch of [`DelegationTaskReport`]s into a `{ "tasks": [..] }`
/// envelope for the `Status` arm. The companion reads this back and renders it
/// uniformly as a `{ "tasks": [..] }` result — one entry per requested id,
/// whether the poll asked for a single id or a whole fan-out.
fn reports_response(reports: Vec<DelegationTaskReport>) -> std::io::Result<BrokerResponse> {
    Ok(BrokerResponse {
        outcome: serde_json::json!({
            "tasks": serde_json::to_value(&reports).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode: {e}"))
            })?,
        }),
    })
}

/// Serialize the pending feedback notes into a
/// `{ "count": N, "feedback": [..], "_commit_ids": [..] }` envelope for the
/// `Feedback` arm. Only the lean `text` + `created_at` reach the agent; the
/// `_commit_ids` are internal — the companion echoes them back in a
/// `CommitFeedback` once it delivers the result, and `render_feedback_result`
/// strips them from the agent-facing output. `count == 0` is "no new feedback".
fn feedback_response(items: &[PendingFeedback]) -> std::io::Result<BrokerResponse> {
    let notes: Vec<Value> = items
        .iter()
        .map(|p| serde_json::json!({ "text": p.text, "created_at": p.created_at }))
        .collect();
    let ids: Vec<&str> = items.iter().map(|p| p.id.as_str()).collect();
    Ok(BrokerResponse {
        outcome: serde_json::json!({
            "count": notes.len(),
            "feedback": notes,
            "_commit_ids": ids,
        }),
    })
}

/// Serialize a resolved [`QuestionOutcome`] into a [`BrokerResponse`] for the
/// `Ask` arm — the `{ answers, declined }` envelope the companion renders.
fn ask_response(outcome: &QuestionOutcome) -> std::io::Result<BrokerResponse> {
    Ok(BrokerResponse {
        outcome: serde_json::to_value(outcome).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode: {e}"))
        })?,
    })
}

/// Serialize a resolved [`SessionInfo`] into a [`BrokerResponse`] for the
/// `SessionInfo` arm — the companion renders it into the `get_session_info`
/// tool result.
fn session_response(info: SessionInfo) -> std::io::Result<BrokerResponse> {
    Ok(BrokerResponse {
        outcome: serde_json::to_value(&info).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode: {e}"))
        })?,
    })
}

fn memory_append_response(
    result: Result<UserMemoryAppendResult, String>,
) -> std::io::Result<BrokerResponse> {
    let outcome = match result {
        Ok(result) => serde_json::to_value(result).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode: {e}"))
        })?,
        Err(message) => memory_failure_outcome("memory_append_failed", message),
    };
    Ok(BrokerResponse { outcome })
}

fn memory_proposal_response(
    result: Result<BrokerMemoryProposalResult, String>,
) -> std::io::Result<BrokerResponse> {
    let outcome = match result {
        Ok(result) => serde_json::to_value(result).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode: {error}"))
        })?,
        Err(message) => memory_failure_outcome("memory_proposal_failed", message),
    };
    Ok(BrokerResponse { outcome })
}

fn memory_failure_outcome(default_code: &str, message: String) -> Value {
    let unavailable = message.contains("unavailable for this session");
    serde_json::json!({
        "error": message,
        "code": if unavailable { "memory_session_unavailable" } else { default_code },
        "retryable": false,
        "durableChanged": false,
        "fallback": "host_memory_action",
    })
}
/// The `declined` outcome — used when the token is invalid, the connection is
/// gone, or the answer one-shot was dropped without a response. The LLM reads it
/// as "the user didn't answer; proceed with your own judgment".
fn ask_declined_response() -> std::io::Result<BrokerResponse> {
    ask_response(&QuestionOutcome {
        answers: Vec::new(),
        declined: true,
    })
}

/// A `Canceled` report for a setup-side rejection the LLM can't react to (bad
/// token, parent gone). Mirrors the old `cancel(..)` DelegationOutcome.
fn report_canceled(message: &str) -> DelegationTaskReport {
    DelegationTaskReport {
        task_id: None,
        status: TaskStatus::Canceled,
        child_conversation_id: None,
        agent_type: None,
        text: None,
        error_code: Some("canceled".into()),
        message: Some(message.into()),
        duration_ms: None,
    }
}

/// A `Failed` report carrying a wire-stable `error_code` for a bad argument.
fn report_failed(error_code: &str, message: &str) -> DelegationTaskReport {
    DelegationTaskReport {
        task_id: None,
        status: TaskStatus::Failed,
        child_conversation_id: None,
        agent_type: None,
        text: None,
        error_code: Some(error_code.into()),
        message: Some(message.into()),
        duration_ms: None,
    }
}

/// An `Unknown` report — used when a status/cancel request fails the token
/// check (we don't leak whether the task exists).
fn unknown_report(task_id: &str) -> DelegationTaskReport {
    DelegationTaskReport {
        task_id: Some(task_id.to_string()),
        status: TaskStatus::Unknown,
        child_conversation_id: None,
        agent_type: None,
        text: None,
        error_code: None,
        message: Some("unknown task id".into()),
        duration_ms: None,
    }
}

fn cancel(message: &str) -> DelegationTaskReport {
    report_canceled(message)
}

fn invalid_agent_type(raw: &str) -> DelegationTaskReport {
    if raw.is_empty() {
        report_failed("invalid_agent_type", "missing agent_type")
    } else {
        report_failed("invalid_agent_type", &format!("invalid agent_type: {raw}"))
    }
}

fn parse_agent_type(raw: &str) -> Option<AgentType> {
    serde_json::from_value(serde_json::Value::String(raw.to_string())).ok()
}

/// Default socket path for the running process, scoped to PID so multiple
/// iyw-claw instances on the same machine don't collide.
///
/// Unix: a `.sock` file inside `temp_dir`.
/// Windows: a named pipe address `\\.\pipe\iyw-claw-delegation-<pid>`. Windows
/// named pipes live in their own kernel namespace and ignore `temp_dir`; the
/// argument is kept for signature parity across platforms.
#[cfg(unix)]
pub fn default_socket_path(temp_dir: &Path) -> PathBuf {
    temp_dir.join(format!("iyw-claw-delegation-{}.sock", std::process::id()))
}

#[cfg(windows)]
pub fn default_socket_path(_temp_dir: &Path) -> PathBuf {
    PathBuf::from(format!(
        r"\\.\pipe\iyw-claw-delegation-{}",
        std::process::id()
    ))
}
