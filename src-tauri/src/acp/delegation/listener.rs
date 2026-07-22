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

use crate::acp::delegation::broker::{DelegationBroker, StatusWait};
use crate::acp::delegation::transport::{
    read_frame, write_frame, BrokerAskRequest, BrokerCancelRequest, BrokerCancelTaskRequest,
    BrokerCommitFeedbackRequest, BrokerCompanionReadyRequest, BrokerFeedbackRequest,
    BrokerMemoryAppendRequest, BrokerMemoryProposalRequest, BrokerMemoryProposalResult,
    BrokerMessage, BrokerPlatformToolsCallRequest, BrokerPlatformToolsListRequest, BrokerRequest,
    BrokerResponse, BrokerSessionRequest, BrokerStatusRequest,
};
use crate::acp::delegation::types::{DelegationRequest, DelegationTaskReport, TaskStatus};
use crate::acp::feedback::{PendingFeedback, SessionFeedbackAccess};
use crate::acp::platform_mcp::PlatformMcpAccess;
use crate::acp::question::{QuestionOutcome, SessionQuestionAccess};
use crate::acp::session_info::{SessionInfo, SessionInfoAccess};
use crate::models::AgentType;
use crate::user_memory::{
    AgentMemoryAppend, AgentMemoryProposal, CandidateObservationSource, UserMemoryAppendResult,
    UserMemoryService, APPEND_USER_MEMORY_TOOL, PROPOSE_USER_MEMORY_TOOL,
};
use serde_json::Value;

/// Hard ceiling on a *positive* `get_delegation_status` long-poll, so a single
/// MCP tool call can't block the companion's round-trip unbounded. The child
/// keeps running past this; the LLM simply re-issues the wait. An explicit
/// `wait_ms = 0` opts out of the ceiling and blocks until the task is terminal.
const STATUS_WAIT_MAX_MS: u64 = 60_000;

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
    /// Only the dedicated `iyw-platform` forwarder token may relay upstream
    /// platform tools. Built-in companion tokens keep this disabled.
    pub platform_tools_enabled: bool,
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
    pub tools: Vec<String>,
}

struct RegisteredToken {
    entry: TokenEntry,
    ready: watch::Sender<CompanionReadyState>,
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
        let ready = self
            .inner
            .read()
            .await
            .get(&request.token)
            .map(|registered| registered.ready.clone());
        let Some(ready) = ready else {
            return false;
        };
        if request.version != env!("CARGO_PKG_VERSION") {
            ready.send_if_modified(|current| {
                if !matches!(current, CompanionReadyState::Pending) {
                    return false;
                }
                *current = CompanionReadyState::Disabled;
                true
            });
            return false;
        }
        let report = CompanionReadyReport {
            version: request.version,
            tools: request.tools,
        };
        ready.send_if_modified(|current| {
            if !matches!(current, CompanionReadyState::Pending) {
                return false;
            }
            *current = CompanionReadyState::Ready(report);
            true
        })
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
    /// Forwarder to the upstream iyw platform MCP service, backing the
    /// `iyw-platform` companion instance. Token-gated like every other arm;
    /// the platform account credential is attached inside the service and
    /// never crosses this wire.
    pub platform: Arc<dyn PlatformMcpAccess>,
}

impl DelegationListener {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        broker: Arc<DelegationBroker>,
        tokens: Arc<TokenRegistry>,
        parent_lookup: Arc<dyn ParentSessionLookup>,
        feedback: Arc<dyn SessionFeedbackAccess>,
        questions: Arc<dyn SessionQuestionAccess>,
        session_info: Arc<dyn SessionInfoAccess>,
        user_memory: Arc<UserMemoryService>,
        platform: Arc<dyn PlatformMcpAccess>,
    ) -> Arc<Self> {
        Arc::new(Self {
            broker,
            tokens,
            parent_lookup,
            feedback,
            questions,
            session_info,
            user_memory,
            platform,
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
                            tracing::error!("[delegation] connection failed: {e}");
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
                    tracing::error!("[delegation] connection failed: {e}");
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
            BrokerMessage::CompanionReady(req) => {
                self.tokens.record_companion_ready(req).await;
                BrokerResponse {
                    outcome: Value::Null,
                }
            }
            BrokerMessage::PlatformToolsList(req) => BrokerResponse {
                outcome: platform_list_outcome(self.process_platform_tools_list(req).await),
            },
            BrokerMessage::PlatformToolsCall(req) => {
                // An upstream business tool can run long (it may call an LLM
                // internally). Race the relay against peer-close on this
                // one-shot connection: a canceled MCP tools/call drops the
                // companion's round-trip future, closing the socket; observing
                // that here drops the in-flight reqwest future, which aborts
                // the upstream HTTP request. Nothing to tear down beyond that
                // — the upstream tools are stateless.
                let call_fut = self.process_platform_tools_call(req);
                tokio::pin!(call_fut);
                let mut probe = [0u8; 1];
                let outcome = tokio::select! {
                    biased;
                    outcome = &mut call_fut => outcome,
                    _ = conn.read(&mut probe) => return Ok(()),
                };
                BrokerResponse {
                    outcome: platform_call_outcome(outcome),
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
        let Some(entry) = self.tokens.lookup(&req.token).await else {
            return Err("User memory update is unavailable for this session.".into());
        };
        if !entry.memory_write_enabled {
            return Err("User memory update is unavailable for this session.".into());
        }
        self.user_memory
            .append_agent_memory_authorized(AgentMemoryAppend {
                content: req.content,
                agent_type: entry.agent_type,
            })
            .await
            .map_err(|error| error.message)
    }

    /// Authenticate proposal capability and derive all provenance from the
    /// launch token entry. The companion supplies only content and signal.
    async fn process_memory_proposal(
        &self,
        req: BrokerMemoryProposalRequest,
    ) -> Result<BrokerMemoryProposalResult, String> {
        let unavailable = || "User memory proposal is unavailable for this session.".to_string();
        let entry = self
            .tokens
            .lookup(&req.token)
            .await
            .ok_or_else(unavailable)?;
        if !entry.memory_proposal_enabled {
            return Err(unavailable());
        }
        let turn_nonce = entry
            .memory_turn_tracker
            .active_nonce()
            .ok_or_else(unavailable)?;
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
                    opaque_source_id: entry.opaque_source_id,
                    turn_nonce,
                },
                move || turn_tracker.acquire_commit_lease(turn_nonce),
            )
            .await
            .map_err(|error| error.message)?;
        Ok(BrokerMemoryProposalResult {
            observation_added: result.observation_added,
            status: result.candidate.status,
            observation_count: result.candidate.observation_count,
            confirmation_recommended: result.confirmation_recommended,
        })
    }

    /// Validate the token and fetch the upstream platform tool catalog. An
    /// invalid token yields the same generic unavailable error as any other
    /// platform failure — no leak of login state or token validity.
    async fn process_platform_tools_list(
        &self,
        req: BrokerPlatformToolsListRequest,
    ) -> Result<Value, String> {
        if !self
            .tokens
            .lookup(&req.token)
            .await
            .is_some_and(|entry| entry.platform_tools_enabled)
        {
            return Err(PLATFORM_TOOLS_UNAVAILABLE.to_string());
        }
        self.platform.list_tools().await
    }

    /// Validate the token and forward one `tools/call` to the upstream
    /// platform MCP service, passing name/arguments through verbatim.
    async fn process_platform_tools_call(
        &self,
        req: BrokerPlatformToolsCallRequest,
    ) -> Result<Value, String> {
        if !self
            .tokens
            .lookup(&req.token)
            .await
            .is_some_and(|entry| entry.platform_tools_enabled)
        {
            return Err(PLATFORM_TOOLS_UNAVAILABLE.to_string());
        }
        self.platform.call_tool(&req.name, req.arguments).await
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
        Err(message) => serde_json::json!({ "error": message }),
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
        Err(message) => serde_json::json!({ "error": message }),
    };
    Ok(BrokerResponse { outcome })
}

/// Agent-facing error for a platform arm whose token failed validation.
const PLATFORM_TOOLS_UNAVAILABLE: &str = "iyw platform tools are unavailable for this session.";

/// Envelope for the `PlatformToolsList` arm: `{ "tools": [..] }` on success,
/// `{ "error": .. }` otherwise (the companion degrades that to an empty list).
fn platform_list_outcome(result: Result<Value, String>) -> Value {
    match result {
        Ok(tools) => serde_json::json!({ "tools": tools }),
        Err(error) => serde_json::json!({ "error": error }),
    }
}

/// Envelope for the `PlatformToolsCall` arm: the upstream MCP result object
/// verbatim on success, `{ "error": .. }` otherwise (the companion renders
/// that as an `isError` tool result).
fn platform_call_outcome(result: Result<Value, String>) -> Value {
    match result {
        Ok(result) => result,
        Err(error) => serde_json::json!({ "error": error }),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::delegation::broker::{ConversationDepthLookup, DelegationConfig};
    use crate::acp::delegation::spawner::{mock::MockSpawner, ConnectionSpawner, SpawnerError};
    use crate::acp::delegation::types::{DelegationError, DelegationOutcome, DelegationSuccess};
    use serde_json::json;
    use std::time::Duration;
    use tokio::io::duplex;

    struct AlwaysRootLookup;
    #[async_trait]
    impl ConversationDepthLookup for AlwaysRootLookup {
        async fn parent_of(&self, _id: i32) -> Result<Option<i32>, DelegationError> {
            Ok(None)
        }
    }

    struct StaticParentLookup(Option<i32>);
    #[async_trait]
    impl ParentSessionLookup for StaticParentLookup {
        async fn current_conversation_id(&self, _parent_connection_id: &str) -> Option<i32> {
            self.0
        }
    }

    /// In-memory feedback stub. `read_pending_feedback` returns the seeded notes
    /// WITHOUT draining (read-only, matching production), recording the conn id;
    /// `commit_feedback_delivered` records the (conn_id, ids) it was committed
    /// with so tests can assert delivery happens only after a successful write.
    /// Default is empty (the delegation tests don't exercise feedback).
    #[derive(Default)]
    struct StubFeedback {
        items: tokio::sync::Mutex<Vec<PendingFeedback>>,
        read_conn: tokio::sync::Mutex<Option<String>>,
        committed: tokio::sync::Mutex<Vec<(String, Vec<String>)>>,
    }
    #[async_trait]
    impl SessionFeedbackAccess for StubFeedback {
        async fn read_pending_feedback(&self, parent_connection_id: &str) -> Vec<PendingFeedback> {
            *self.read_conn.lock().await = Some(parent_connection_id.to_string());
            self.items.lock().await.clone()
        }
        async fn commit_feedback_delivered(&self, parent_connection_id: &str, ids: Vec<String>) {
            self.committed
                .lock()
                .await
                .push((parent_connection_id.to_string(), ids));
        }
    }

    /// In-memory question stub. `register_question` mints a sequential id,
    /// stashes the answer sender (so a test can resolve it via `answer`), and
    /// records the (parent_conn, questions); `cancel_question` removes the
    /// sender and records the canceled id. Lets the listener's `Ask` arm be
    /// driven without a real `ConnectionManager`.
    #[derive(Default)]
    struct StubQuestion {
        pending: tokio::sync::Mutex<HashMap<String, oneshot::Sender<QuestionOutcome>>>,
        registered: tokio::sync::Mutex<Vec<(String, Vec<crate::acp::question::QuestionSpec>)>>,
        canceled: tokio::sync::Mutex<Vec<String>>,
    }
    #[async_trait]
    impl SessionQuestionAccess for StubQuestion {
        async fn register_question(
            &self,
            parent_connection_id: &str,
            questions: Vec<crate::acp::question::QuestionSpec>,
        ) -> Option<crate::acp::question::RegisteredQuestion> {
            let question_id = format!("q-{}", self.registered.lock().await.len() + 1);
            let (tx, rx) = oneshot::channel();
            self.pending.lock().await.insert(question_id.clone(), tx);
            self.registered
                .lock()
                .await
                .push((parent_connection_id.to_string(), questions));
            Some(crate::acp::question::RegisteredQuestion {
                question_id,
                answer_rx: rx,
            })
        }
        async fn cancel_question(&self, _parent_connection_id: &str, question_id: &str) {
            self.pending.lock().await.remove(question_id);
            self.canceled.lock().await.push(question_id.to_string());
        }
        async fn cancel_questions_by_parent(&self, _parent_connection_id: &str) {
            // Not exercised by the listener unit tests (the teardown sweep lives
            // in connection.rs); drop all parked senders to satisfy the trait.
            self.pending.lock().await.clear();
        }
    }
    impl StubQuestion {
        async fn answer(&self, question_id: &str, outcome: QuestionOutcome) {
            if let Some(tx) = self.pending.lock().await.remove(question_id) {
                let _ = tx.send(outcome);
            }
        }
    }

    /// In-memory session-info stub. Records every `(session_id, max_messages)` it
    /// was asked to resolve and returns a seeded outcome — `found` sessions echo
    /// their id, unknown ids return `not_found`. Default knows about no sessions.
    #[derive(Default)]
    struct StubSessionInfo {
        known: std::collections::HashSet<i32>,
        calls: tokio::sync::Mutex<Vec<(i32, u32)>>,
    }
    #[async_trait]
    impl SessionInfoAccess for StubSessionInfo {
        async fn resolve(&self, session_id: i32, max_messages: u32) -> SessionInfo {
            self.calls.lock().await.push((session_id, max_messages));
            if self.known.contains(&session_id) {
                SessionInfo {
                    found: true,
                    session_id,
                    title: Some(format!("session {session_id}")),
                    ..Default::default()
                }
            } else {
                SessionInfo::not_found(session_id)
            }
        }
    }

    fn stub_user_memory() -> Arc<UserMemoryService> {
        Arc::new(UserMemoryService::new(
            sea_orm::DatabaseConnection::Disconnected,
            PathBuf::new(),
        ))
    }

    /// In-memory platform-forwarder stub. Records every `tools/call` it
    /// relays; `list_tools` / `call_tool` return the seeded results. Default
    /// behaves like an unreachable upstream (both err).
    struct StubPlatform {
        list_result: Result<Value, String>,
        call_result: Result<Value, String>,
        calls: tokio::sync::Mutex<Vec<(String, Value)>>,
    }
    impl Default for StubPlatform {
        fn default() -> Self {
            Self {
                list_result: Err("platform upstream unreachable".into()),
                call_result: Err("platform upstream unreachable".into()),
                calls: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }
    #[async_trait]
    impl crate::acp::platform_mcp::PlatformMcpAccess for StubPlatform {
        async fn list_tools(&self) -> Result<Value, String> {
            self.list_result.clone()
        }
        async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, String> {
            self.calls.lock().await.push((name.to_string(), arguments));
            self.call_result.clone()
        }
    }

    fn stub_platform() -> Arc<StubPlatform> {
        Arc::new(StubPlatform::default())
    }

    fn test_token_entry(parent_connection_id: &str) -> TokenEntry {
        let memory_turn_tracker = Arc::new(crate::acp::memory_turn::MemoryTurnTracker::default());
        TokenEntry {
            parent_connection_id: parent_connection_id.to_string(),
            working_dir: PathBuf::from("/tmp"),
            agent_type: AgentType::Codex,
            memory_write_enabled: false,
            platform_tools_enabled: true,
            memory_proposal_enabled: false,
            opaque_source_id: crate::acp::memory_turn::derive_opaque_source_id(
                "test-token",
                parent_connection_id,
            ),
            memory_turn_tracker,
        }
    }

    use tokio::sync::oneshot;

    async fn make_broker(mock: Arc<MockSpawner>) -> Arc<DelegationBroker> {
        let broker = Arc::new(DelegationBroker::new(
            mock as Arc<dyn ConnectionSpawner>,
            Arc::new(AlwaysRootLookup) as Arc<dyn ConversationDepthLookup>,
        ));
        // Production default is `enabled: false`; listener tests that don't
        // explicitly set their own config need the switch flipped on so
        // `handle_request` parks pending entries instead of returning
        // `Canceled { reason: "delegation disabled" }` straight away.
        broker
            .set_config(DelegationConfig {
                enabled: true,
                ..DelegationConfig::default()
            })
            .await;
        broker
    }

    fn make_listener(
        broker: Arc<DelegationBroker>,
        tokens: Arc<TokenRegistry>,
        parent_conversation: Option<i32>,
    ) -> Arc<DelegationListener> {
        DelegationListener::new(
            broker,
            tokens,
            Arc::new(StaticParentLookup(parent_conversation)),
            Arc::new(StubFeedback::default()),
            Arc::new(StubQuestion::default()),
            Arc::new(StubSessionInfo::default()),
            stub_user_memory(),
            stub_platform(),
        )
    }

    /// Build a listener whose feedback access is the given stub, so feedback
    /// tests can seed notes and assert the drain. Delegation pieces are minimal.
    fn make_feedback_listener(
        tokens: Arc<TokenRegistry>,
        feedback: Arc<StubFeedback>,
    ) -> Arc<DelegationListener> {
        let broker = Arc::new(DelegationBroker::new(
            Arc::new(MockSpawner::new()) as Arc<dyn ConnectionSpawner>,
            Arc::new(AlwaysRootLookup) as Arc<dyn ConversationDepthLookup>,
        ));
        DelegationListener::new(
            broker,
            tokens,
            Arc::new(StaticParentLookup(Some(1))),
            feedback,
            Arc::new(StubQuestion::default()),
            Arc::new(StubSessionInfo::default()),
            stub_user_memory(),
            stub_platform(),
        )
    }

    /// Build a listener whose question access is the given stub, so ask tests
    /// can register/answer questions and assert the round-trip. Delegation and
    /// feedback pieces are minimal.
    fn make_question_listener(
        tokens: Arc<TokenRegistry>,
        questions: Arc<StubQuestion>,
    ) -> Arc<DelegationListener> {
        let broker = Arc::new(DelegationBroker::new(
            Arc::new(MockSpawner::new()) as Arc<dyn ConnectionSpawner>,
            Arc::new(AlwaysRootLookup) as Arc<dyn ConversationDepthLookup>,
        ));
        DelegationListener::new(
            broker,
            tokens,
            Arc::new(StaticParentLookup(Some(1))),
            Arc::new(StubFeedback::default()),
            questions,
            Arc::new(StubSessionInfo::default()),
            stub_user_memory(),
            stub_platform(),
        )
    }

    /// Build a listener whose session-info access is the given stub, so
    /// `get_session_info` tests can seed known sessions and assert the round-trip.
    fn make_session_listener(
        tokens: Arc<TokenRegistry>,
        session_info: Arc<StubSessionInfo>,
    ) -> Arc<DelegationListener> {
        let broker = Arc::new(DelegationBroker::new(
            Arc::new(MockSpawner::new()) as Arc<dyn ConnectionSpawner>,
            Arc::new(AlwaysRootLookup) as Arc<dyn ConversationDepthLookup>,
        ));
        DelegationListener::new(
            broker,
            tokens,
            Arc::new(StaticParentLookup(Some(1))),
            Arc::new(StubFeedback::default()),
            Arc::new(StubQuestion::default()),
            session_info,
            stub_user_memory(),
            stub_platform(),
        )
    }

    async fn make_request(input: serde_json::Value) -> BrokerRequest {
        BrokerRequest {
            token: "tok".into(),
            parent_connection_id: "parent-conn".into(),
            parent_tool_use_id: "pt-1".into(),
            external_handle: None,
            input,
        }
    }

    async fn make_memory_listener(
        memory_write_enabled: bool,
    ) -> (
        tempfile::TempDir,
        Arc<DelegationListener>,
        Arc<UserMemoryService>,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let db = crate::db::init_database(temp.path().join("db"), "test")
            .await
            .unwrap();
        let user_memory = Arc::new(UserMemoryService::new(db.conn, temp.path().join("memory")));
        let tokens = Arc::new(TokenRegistry::default());
        let memory_turn_tracker = Arc::new(crate::acp::memory_turn::MemoryTurnTracker::default());
        tokens
            .register(
                "memory-token".into(),
                TokenEntry {
                    parent_connection_id: "parent-conn".into(),
                    working_dir: PathBuf::from("/tmp"),
                    agent_type: AgentType::Gemini,
                    memory_write_enabled,
                    platform_tools_enabled: false,
                    memory_proposal_enabled: false,
                    opaque_source_id: crate::acp::memory_turn::derive_opaque_source_id(
                        "memory-token",
                        "parent-conn",
                    ),
                    memory_turn_tracker: memory_turn_tracker.clone(),
                },
            )
            .await;
        let broker = Arc::new(DelegationBroker::new(
            Arc::new(MockSpawner::new()) as Arc<dyn ConnectionSpawner>,
            Arc::new(AlwaysRootLookup) as Arc<dyn ConversationDepthLookup>,
        ));
        let listener = DelegationListener::new(
            broker,
            tokens,
            Arc::new(StaticParentLookup(Some(1))),
            Arc::new(StubFeedback::default()),
            Arc::new(StubQuestion::default()),
            Arc::new(StubSessionInfo::default()),
            user_memory.clone(),
            stub_platform(),
        );
        (temp, listener, user_memory)
    }

    #[tokio::test]
    async fn memory_append_uses_authenticated_token_agent_and_persists() {
        let (_temp, listener, user_memory) = make_memory_listener(true).await;

        let result = listener
            .process_memory_append(BrokerMemoryAppendRequest {
                token: "memory-token".into(),
                content: "User prefers compact status updates".into(),
            })
            .await
            .unwrap();
        let snapshot = user_memory.snapshot().await.unwrap();
        let content =
            &snapshot.documents[&crate::user_memory::UserMemoryDocumentId::Memory].content;

        assert!(result.appended);
        assert!(content.contains("[Gemini CLI] User prefers compact status updates"));
    }

    #[tokio::test]
    async fn memory_append_rejects_invalid_or_write_disabled_token() {
        let (_temp, listener, user_memory) = make_memory_listener(false).await;

        for token in ["memory-token", "invalid-token"] {
            let error = listener
                .process_memory_append(BrokerMemoryAppendRequest {
                    token: token.into(),
                    content: "must not persist".into(),
                })
                .await
                .unwrap_err();
            assert_eq!(error, "User memory update is unavailable for this session.");
        }
        let snapshot = user_memory.snapshot().await.unwrap();
        assert!(
            snapshot.documents[&crate::user_memory::UserMemoryDocumentId::Memory]
                .content
                .is_empty()
        );
    }

    #[tokio::test]
    async fn invalid_token_rejected() {
        let listener = make_listener(
            make_broker(Arc::new(MockSpawner::new())).await,
            Arc::new(TokenRegistry::default()),
            Some(1),
        );
        let report = listener
            .process(make_request(json!({"agent_type": "codex", "task": "x"})).await)
            .await;
        assert_eq!(report.status, TaskStatus::Canceled);
        assert_eq!(report.error_code.as_deref(), Some("canceled"));
        assert!(report.message.unwrap().contains("invalid token"));
    }

    /// Build a listener whose platform access is the given stub, with a
    /// pre-registered `platform-token`. Delegation pieces are minimal.
    async fn make_platform_listener(platform: Arc<StubPlatform>) -> Arc<DelegationListener> {
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("platform-token".into(), test_token_entry("parent-conn"))
            .await;
        let broker = Arc::new(DelegationBroker::new(
            Arc::new(MockSpawner::new()) as Arc<dyn ConnectionSpawner>,
            Arc::new(AlwaysRootLookup) as Arc<dyn ConversationDepthLookup>,
        ));
        DelegationListener::new(
            broker,
            tokens,
            Arc::new(StaticParentLookup(Some(1))),
            Arc::new(StubFeedback::default()),
            Arc::new(StubQuestion::default()),
            Arc::new(StubSessionInfo::default()),
            stub_user_memory(),
            platform,
        )
    }

    #[tokio::test]
    async fn platform_tools_list_round_trips_over_serve_one() {
        let platform = Arc::new(StubPlatform {
            list_result: Ok(json!([{ "name": "echo" }])),
            ..StubPlatform::default()
        });
        let listener = make_platform_listener(platform).await;

        let (mut client, mut server) = duplex(64 * 1024);
        let task = tokio::spawn(async move { listener.serve_one(&mut server).await });
        let msg = BrokerMessage::PlatformToolsList(BrokerPlatformToolsListRequest {
            token: "platform-token".into(),
        });
        write_frame(&mut client, &msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        task.await.unwrap().unwrap();

        assert_eq!(resp.outcome["tools"][0]["name"], "echo");
    }

    #[tokio::test]
    async fn platform_tools_list_invalid_token_yields_unavailable_error() {
        let platform = Arc::new(StubPlatform {
            list_result: Ok(json!([{ "name": "echo" }])),
            ..StubPlatform::default()
        });
        let listener = make_platform_listener(platform).await;

        let outcome = listener
            .process_platform_tools_list(BrokerPlatformToolsListRequest {
                token: "wrong-token".into(),
            })
            .await;
        assert_eq!(outcome.unwrap_err(), PLATFORM_TOOLS_UNAVAILABLE);
    }

    #[tokio::test]
    async fn platform_tools_call_forwards_verbatim_and_returns_result() {
        let platform = Arc::new(StubPlatform {
            call_result: Ok(json!({
                "content": [{ "type": "text", "text": "ok" }],
                "isError": false
            })),
            ..StubPlatform::default()
        });
        let listener = make_platform_listener(platform.clone()).await;

        let (mut client, mut server) = duplex(64 * 1024);
        let task = tokio::spawn(async move { listener.serve_one(&mut server).await });
        let msg = BrokerMessage::PlatformToolsCall(BrokerPlatformToolsCallRequest {
            token: "platform-token".into(),
            name: "query_orders".into(),
            arguments: json!({ "order_id": "o-1" }),
        });
        write_frame(&mut client, &msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        task.await.unwrap().unwrap();

        assert_eq!(resp.outcome["content"][0]["text"], "ok");
        let calls = platform.calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "query_orders");
        assert_eq!(calls[0].1["order_id"], "o-1");
    }

    #[tokio::test]
    async fn platform_tools_call_upstream_error_becomes_error_envelope() {
        let listener = make_platform_listener(stub_platform()).await;

        let (mut client, mut server) = duplex(64 * 1024);
        let task = tokio::spawn(async move { listener.serve_one(&mut server).await });
        let msg = BrokerMessage::PlatformToolsCall(BrokerPlatformToolsCallRequest {
            token: "platform-token".into(),
            name: "echo".into(),
            arguments: json!({}),
        });
        write_frame(&mut client, &msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        task.await.unwrap().unwrap();

        assert_eq!(resp.outcome["error"], "platform upstream unreachable");
    }

    #[tokio::test]
    async fn token_parent_mismatch_rejected() {
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("other-parent"))
            .await;
        let listener = make_listener(
            make_broker(Arc::new(MockSpawner::new())).await,
            tokens,
            Some(1),
        );
        let report = listener
            .process(make_request(json!({"agent_type": "codex", "task": "x"})).await)
            .await;
        assert_eq!(report.status, TaskStatus::Canceled);
        assert!(report.message.unwrap().contains("does not match"));
    }

    #[tokio::test]
    async fn missing_parent_conversation_rejected() {
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        // parent_conversation = None: parent has no live conversation.
        let listener = make_listener(
            make_broker(Arc::new(MockSpawner::new())).await,
            tokens,
            None,
        );
        let report = listener
            .process(make_request(json!({"agent_type": "codex", "task": "x"})).await)
            .await;
        assert_eq!(report.status, TaskStatus::Canceled);
        assert!(report.message.unwrap().contains("no active conversation"));
    }

    #[tokio::test]
    async fn invalid_agent_type_rejected() {
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_listener(
            make_broker(Arc::new(MockSpawner::new())).await,
            tokens,
            Some(1),
        );
        let report = listener
            .process(make_request(json!({"agent_type": "garbage", "task": "x"})).await)
            .await;
        assert_eq!(report.status, TaskStatus::Failed);
        assert_eq!(report.error_code.as_deref(), Some("invalid_agent_type"));
    }

    /// Full async round-trip through the listener: `delegate_to_agent` returns a
    /// Running ack, the lifecycle resolves the child via `complete_call`, and a
    /// follow-up `get_delegation_status` collects the Completed result.
    #[tokio::test]
    async fn happy_path_ack_then_status_collects_result() {
        let mock = Arc::new(MockSpawner::new());
        mock.queue_spawn(Ok("child-conn".into())).await;
        mock.queue_send(Ok(42)).await;
        let broker = make_broker(mock.clone()).await;
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;

        // 1. delegate_to_agent → Running ack carrying the child conversation id.
        let listener = make_listener(broker.clone(), tokens.clone(), Some(1));
        let (mut client, mut server) = duplex(16 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let msg = BrokerMessage::Call(BrokerRequest {
            token: "tok".into(),
            parent_connection_id: "parent-conn".into(),
            parent_tool_use_id: "pt-1".into(),
            external_handle: None,
            input: json!({"agent_type": "codex", "task": "do x"}),
        });
        write_frame(&mut client, &msg).await.unwrap();
        let ack: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        assert_eq!(ack.outcome["status"], "running");
        assert_eq!(ack.outcome["child_conversation_id"], 42);
        let task_id = ack.outcome["task_id"].as_str().unwrap().to_string();

        // 2. The lifecycle resolves the child on TurnComplete.
        broker
            .complete_call(
                &task_id,
                DelegationOutcome::Ok(DelegationSuccess {
                    text: "result-text".into(),
                    child_conversation_id: 42,
                    child_agent_type: AgentType::Codex,
                    turn_count: 1,
                    duration_ms: 5,
                    token_usage: None,
                }),
            )
            .await;

        // 3. get_delegation_status → Completed with the result text.
        let listener = make_listener(broker.clone(), tokens, Some(1));
        let (mut client, mut server) = duplex(16 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let status = BrokerMessage::Status(BrokerStatusRequest {
            token: "tok".into(),
            task_ids: vec![task_id.clone()],
            wait_ms: Some(1_000),
        });
        write_frame(&mut client, &status).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        // The Status arm returns a `{ tasks: [..] }` envelope; a single id is
        // the first (only) entry.
        assert_eq!(resp.outcome["tasks"][0]["status"], "completed");
        assert_eq!(resp.outcome["tasks"][0]["text"], "result-text");
        assert_eq!(resp.outcome["tasks"][0]["child_conversation_id"], 42);
    }

    /// Start a running task directly and return `(broker, tokens, task_id)`.
    /// Shared setup for the `wait_ms` mapping tests below.
    async fn running_task_fixture() -> (Arc<DelegationBroker>, Arc<TokenRegistry>, String) {
        let mock = Arc::new(MockSpawner::new());
        mock.queue_spawn(Ok("child-conn".into())).await;
        mock.queue_send(Ok(7)).await;
        let broker = make_broker(mock).await;
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let ack = broker
            .start_delegation(DelegationRequest {
                parent_connection_id: "parent-conn".into(),
                parent_conversation_id: 1,
                parent_tool_use_id: "pt-1".into(),
                agent_type: AgentType::Codex,
                task: "do x".into(),
                working_dir: None,
                requested_working_dir: None,
                external_handle: None,
            })
            .await;
        let task_id = ack.task_id.clone().expect("running task carries an id");
        (broker, tokens, task_id)
    }

    /// Omitted `wait_ms` (the safe default) maps to an immediate snapshot: the
    /// status of a still-running task returns `running` right away rather than
    /// blocking.
    #[tokio::test]
    async fn status_omitted_wait_returns_immediately() {
        let (broker, tokens, task_id) = running_task_fixture().await;
        let listener = make_listener(broker, tokens, Some(1));
        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move { listener.serve_one(&mut server).await });

        let status = BrokerMessage::Status(BrokerStatusRequest {
            token: "tok".into(),
            task_ids: vec![task_id],
            wait_ms: None,
        });
        write_frame(&mut client, &status).await.unwrap();
        // No completion ever happens — an immediate poll must still return.
        let resp: BrokerResponse = tokio::time::timeout(Duration::from_secs(2), async {
            read_frame::<_, BrokerResponse>(&mut client).await.unwrap()
        })
        .await
        .expect("omitted wait_ms must return immediately");
        server_task.await.unwrap().unwrap();
        assert_eq!(resp.outcome["tasks"][0]["status"], "running");
    }

    /// An explicit `wait_ms = 0` maps to an unbounded wait: the call blocks
    /// while the task is running and only resolves once it reaches a terminal
    /// state, returning the completed report through the wire.
    #[tokio::test]
    async fn status_explicit_zero_blocks_until_terminal() {
        let (broker, tokens, task_id) = running_task_fixture().await;
        let listener = make_listener(broker.clone(), tokens, Some(1));
        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move { listener.serve_one(&mut server).await });

        let status = BrokerMessage::Status(BrokerStatusRequest {
            token: "tok".into(),
            task_ids: vec![task_id.clone()],
            wait_ms: Some(0),
        });
        write_frame(&mut client, &status).await.unwrap();

        // While the task runs, the wait must NOT resolve.
        let early = tokio::time::timeout(Duration::from_millis(50), async {
            read_frame::<_, BrokerResponse>(&mut client).await
        })
        .await;
        assert!(
            early.is_err(),
            "wait_ms=0 must block while the task is still running"
        );

        // Resolving the task wakes the parked wait, which returns completed.
        broker
            .complete_call(
                &task_id,
                DelegationOutcome::Ok(DelegationSuccess {
                    text: "done".into(),
                    child_conversation_id: 7,
                    child_agent_type: AgentType::Codex,
                    turn_count: 1,
                    duration_ms: 5,
                    token_usage: None,
                }),
            )
            .await;
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap().unwrap();
        assert_eq!(resp.outcome["tasks"][0]["status"], "completed");
        assert_eq!(resp.outcome["tasks"][0]["text"], "done");
    }

    /// A `wait_ms = 0` status call that the companion cancels (dropping the
    /// request socket) must not leave `serve_one` parked until the task is
    /// terminal. The peer-close race abandons the wait while leaving the task
    /// itself untouched — there's no broker-side side effect from a status
    /// query.
    #[tokio::test]
    async fn infinite_status_wait_abandoned_when_peer_closes() {
        let (broker, tokens, task_id) = running_task_fixture().await;
        let listener = make_listener(broker.clone(), tokens, Some(1));
        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move { listener.serve_one(&mut server).await });

        let status = BrokerMessage::Status(BrokerStatusRequest {
            token: "tok".into(),
            task_ids: vec![task_id],
            wait_ms: Some(0),
        });
        write_frame(&mut client, &status).await.unwrap();

        // Let the server park inside the unbounded wait.
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            !server_task.is_finished(),
            "server must be parked on the unbounded wait"
        );

        // Companion cancels: drop the request socket without completing the task.
        drop(client);

        // serve_one must observe the peer-close and return promptly instead of
        // hanging until the (never-completing) task is terminal.
        let result = tokio::time::timeout(Duration::from_secs(5), server_task)
            .await
            .expect("serve_one must return after the peer closes");
        result.unwrap().unwrap();

        // The task itself was not touched by the abandoned status query.
        assert_eq!(broker.pending_count().await, 1);
    }

    /// Batch status over the listener: two tasks, one completed and one still
    /// running, return as a `{ tasks: [..] }` envelope with both reports in
    /// request order.
    #[tokio::test]
    async fn batch_status_over_listener_multi_id() {
        let mock = Arc::new(MockSpawner::new());
        mock.queue_spawn(Ok("child-1".into())).await;
        mock.queue_send(Ok(1)).await;
        mock.queue_spawn(Ok("child-2".into())).await;
        mock.queue_send(Ok(2)).await;
        let broker = make_broker(mock.clone()).await;
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let start = |tool_use: &'static str| {
            let broker = broker.clone();
            async move {
                broker
                    .start_delegation(DelegationRequest {
                        parent_connection_id: "parent-conn".into(),
                        parent_conversation_id: 1,
                        parent_tool_use_id: tool_use.into(),
                        agent_type: AgentType::Codex,
                        task: "do x".into(),
                        working_dir: None,
                        requested_working_dir: None,
                        external_handle: None,
                    })
                    .await
                    .task_id
                    .unwrap()
            }
        };
        let t1 = start("pt-1").await;
        let t2 = start("pt-2").await;
        broker
            .complete_call(
                &t1,
                DelegationOutcome::Ok(DelegationSuccess {
                    text: "first".into(),
                    child_conversation_id: 1,
                    child_agent_type: AgentType::Codex,
                    turn_count: 1,
                    duration_ms: 3,
                    token_usage: None,
                }),
            )
            .await;

        let listener = make_listener(broker.clone(), tokens, Some(1));
        let (mut client, mut server) = duplex(16 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let status = BrokerMessage::Status(BrokerStatusRequest {
            token: "tok".into(),
            task_ids: vec![t1.clone(), t2.clone()],
            wait_ms: None,
        });
        write_frame(&mut client, &status).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        let tasks = resp.outcome["tasks"].as_array().expect("tasks array");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0]["status"], "completed");
        assert_eq!(tasks[0]["task_id"], t1.as_str());
        assert_eq!(tasks[1]["status"], "running");
        assert_eq!(tasks[1]["task_id"], t2.as_str());
    }

    /// An invalid token over a batch status reports `Unknown` for EACH requested
    /// id (preserving order) rather than collapsing to a single report — so the
    /// companion can still render one row per task.
    #[tokio::test]
    async fn batch_status_invalid_token_returns_unknown_per_id() {
        let listener = make_listener(
            make_broker(Arc::new(MockSpawner::new())).await,
            Arc::new(TokenRegistry::default()),
            Some(1),
        );
        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let status = BrokerMessage::Status(BrokerStatusRequest {
            token: "bad-token".into(),
            task_ids: vec!["a".into(), "b".into()],
            wait_ms: None,
        });
        write_frame(&mut client, &status).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        let tasks = resp.outcome["tasks"].as_array().expect("tasks array");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0]["status"], "unknown");
        assert_eq!(tasks[0]["task_id"], "a");
        assert_eq!(tasks[1]["status"], "unknown");
        assert_eq!(tasks[1]["task_id"], "b");
    }

    /// `cancel_delegation` over the listener: a running task is canceled by id
    /// and reports `canceled`.
    #[tokio::test]
    async fn cancel_task_by_id_over_listener() {
        let mock = Arc::new(MockSpawner::new());
        mock.queue_spawn(Ok("child-conn".into())).await;
        mock.queue_send(Ok(7)).await;
        let broker = make_broker(mock.clone()).await;
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        // Start a task directly so we hold its id.
        let ack = broker
            .start_delegation(DelegationRequest {
                parent_connection_id: "parent-conn".into(),
                parent_conversation_id: 1,
                parent_tool_use_id: "pt-1".into(),
                agent_type: AgentType::Codex,
                task: "do x".into(),
                working_dir: None,
                requested_working_dir: None,
                external_handle: None,
            })
            .await;
        let task_id = ack.task_id.clone().unwrap();

        let listener = make_listener(broker.clone(), tokens, Some(1));
        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let cancel = BrokerMessage::CancelTask(BrokerCancelTaskRequest {
            token: "tok".into(),
            task_id: task_id.clone(),
        });
        write_frame(&mut client, &cancel).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        assert_eq!(resp.outcome["status"], "canceled");
        assert_eq!(broker.pending_count().await, 0);
    }

    #[tokio::test]
    async fn cancel_message_routed_to_broker() {
        let mock = Arc::new(MockSpawner::new());
        mock.queue_spawn(Ok("c-cancel".into())).await;
        mock.queue_send(Ok(99)).await;
        let broker = make_broker(mock.clone()).await;
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_listener(broker.clone(), tokens, Some(1));

        // Park a delegation call with a known external_handle.
        let driver = {
            let broker = broker.clone();
            tokio::spawn(async move {
                let req = DelegationRequest {
                    parent_connection_id: "parent-conn".into(),
                    parent_conversation_id: 1,
                    parent_tool_use_id: "pt-cancel".into(),
                    agent_type: AgentType::Codex,
                    task: "do x".into(),
                    working_dir: None,
                    requested_working_dir: None,
                    external_handle: Some("h-1".into()),
                };
                broker.handle_request(req).await
            })
        };
        while broker.pending_count().await == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // Drive a cancel through the listener — listener should ack with
        // an empty BrokerResponse and the broker should drain the pending.
        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });

        let cancel_msg = BrokerMessage::Cancel(BrokerCancelRequest {
            token: "tok".into(),
            external_handle: "h-1".into(),
            reason: Some("from test".into()),
        });
        write_frame(&mut client, &cancel_msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        assert!(resp.outcome.is_null(), "cancel ack must be null");
        server_task.await.unwrap();

        let outcome = driver.await.unwrap();
        match outcome {
            DelegationOutcome::Err { code, .. } => assert_eq!(code, "canceled"),
            other => panic!("expected canceled, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn token_registry_revoke_and_revoke_by_parent() {
        let registry = TokenRegistry::default();
        registry.register("t1".into(), test_token_entry("p1")).await;
        registry.register("t2".into(), test_token_entry("p1")).await;
        registry.register("t3".into(), test_token_entry("p2")).await;

        registry.revoke("t1").await;
        assert!(registry.lookup("t1").await.is_none());
        assert!(registry.lookup("t2").await.is_some());

        registry.revoke_by_parent("p1").await;
        assert!(registry.lookup("t2").await.is_none());
        assert!(registry.lookup("t3").await.is_some());
    }

    // Sanity: spawn failure surfaces as spawn_failed when the listener path
    // is exercised. Exercises the full process() → broker.handle_request chain.
    #[tokio::test]
    async fn spawn_failure_surfaces_through_listener() {
        let mock = Arc::new(MockSpawner::new());
        mock.queue_spawn(Err(SpawnerError::Spawn("agent missing".into())))
            .await;
        // `make_broker` already enables delegation; this call narrows the
        // depth limit (8 instead of the helper's default) without changing
        // the enable bit.
        let broker = make_broker(mock).await;
        broker
            .set_config(DelegationConfig {
                enabled: true,
                depth_limit: 8,
                ..DelegationConfig::default()
            })
            .await;
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_listener(broker, tokens, Some(1));

        let report = listener
            .process(make_request(json!({"agent_type": "codex", "task": "x"})).await)
            .await;
        assert_eq!(report.status, TaskStatus::Failed);
        assert_eq!(report.error_code.as_deref(), Some("spawn_failed"));
    }

    // --- check_user_feedback over the listener -----------------------------

    use crate::acp::feedback::PendingFeedback;

    fn pending(id: &str, text: &str) -> PendingFeedback {
        PendingFeedback {
            id: id.into(),
            text: text.into(),
            created_at: chrono::Utc::now(),
        }
    }

    /// The manager chunks each response via `bounded_feedback_batch`. The
    /// serialized `feedback_response` of any such chunk must stay under the
    /// transport cap (`MAX_FRAME_BYTES` = 16 MiB) so the companion's `read_frame`
    /// never rejects it after the listener committed delivery — for BOTH
    /// worst-case-escaping notes AND a flood of tiny notes (whose per-note JSON
    /// overhead, not text length, is what a naive text-only bound would miss).
    #[test]
    fn bounded_feedback_response_always_fits_a_transport_frame() {
        use crate::acp::delegation::transport::MAX_FRAME_BYTES;
        use crate::acp::feedback::{bounded_feedback_batch, MAX_FEEDBACK_RESPONSE_BYTES};

        // Worst-case escaping: many MAX_FEEDBACK_CHARS-sized control-char notes.
        let worst = "\u{0001}".repeat(4096);
        let big: Vec<PendingFeedback> = (0..5_000)
            .map(|i| pending(&format!("b{i}"), &worst))
            .collect();
        // A flood of tiny notes: little text, lots of per-note JSON overhead.
        let tiny: Vec<PendingFeedback> = (0..200_000)
            .map(|i| pending(&format!("t{i}"), "x"))
            .collect();

        for (label, set) in [("worst-case", big), ("tiny-flood", tiny)] {
            let total = set.len();
            let batch = bounded_feedback_batch(set, MAX_FEEDBACK_RESPONSE_BYTES);
            assert!(batch.len() < total, "{label}: batch must be chunked");
            let encoded = serde_json::to_vec(&feedback_response(&batch).unwrap()).unwrap();
            assert!(
                encoded.len() < MAX_FRAME_BYTES,
                "{label}: bounded response must fit a transport frame: {} >= {}",
                encoded.len(),
                MAX_FRAME_BYTES
            );
        }
    }

    /// A valid `check_user_feedback` returns the parent's notes in a
    /// `{ count, feedback: [..] }` envelope (lean text, no ids) scoped to the
    /// token's parent connection, and — crucially — commits them delivered ONLY
    /// after the response is written, with the exact note ids.
    #[tokio::test]
    async fn feedback_returns_notes_then_commits_after_write() {
        let feedback = Arc::new(StubFeedback::default());
        *feedback.items.lock().await = vec![
            pending("f1", "use the existing UserService"),
            pending("f2", "skip the migration"),
        ];
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_feedback_listener(tokens, feedback.clone());

        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let msg = BrokerMessage::Feedback(BrokerFeedbackRequest {
            token: "tok".into(),
        });
        write_frame(&mut client, &msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();

        assert_eq!(resp.outcome["count"], 2);
        let notes = resp.outcome["feedback"].as_array().unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0]["text"], "use the existing UserService");
        // The lean note shape carries no internal id...
        assert!(notes[0].get("id").is_none());
        // ...but the envelope carries `_commit_ids` for the companion to echo
        // back in a CommitFeedback after it delivers the result.
        let commit_ids = resp.outcome["_commit_ids"].as_array().unwrap();
        assert_eq!(commit_ids, &vec!["f1", "f2"]);
        // Read was scoped to the token's parent connection id.
        assert_eq!(
            feedback.read_conn.lock().await.as_deref(),
            Some("parent-conn")
        );
        // The Feedback arm is READ-ONLY — it does NOT commit (delivery is
        // committed later, by the companion's CommitFeedback).
        assert!(feedback.committed.lock().await.is_empty());
    }

    /// A valid `get_session_info` resolves the session by id and returns its
    /// metadata; the resolver is called with the requested id + max_messages.
    #[tokio::test]
    async fn session_info_valid_token_resolves_by_id() {
        let session_info = Arc::new(StubSessionInfo {
            known: std::collections::HashSet::from([42]),
            ..Default::default()
        });
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_session_listener(tokens, session_info.clone());

        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let msg = BrokerMessage::SessionInfo(BrokerSessionRequest {
            token: "tok".into(),
            session_id: 42,
            max_messages: Some(15),
        });
        write_frame(&mut client, &msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();

        assert_eq!(resp.outcome["found"], true);
        assert_eq!(resp.outcome["session_id"], 42);
        assert_eq!(resp.outcome["title"], "session 42");
        // The resolver saw the id + the requested message budget.
        assert_eq!(session_info.calls.lock().await.as_slice(), &[(42, 15)]);
    }

    /// Accepted-policy coverage (deliberate single-tenant scope): a single valid
    /// token resolves ANY non-deleted session id — not only ids "referenced" in the
    /// prompt. Three unrelated ids all resolve through one token.
    #[tokio::test]
    async fn session_info_resolves_any_session_id_not_just_referenced() {
        let session_info = Arc::new(StubSessionInfo {
            known: std::collections::HashSet::from([7, 42, 1000]),
            ..Default::default()
        });
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_session_listener(tokens, session_info.clone());

        for id in [7, 42, 1000] {
            let (mut client, mut server) = duplex(8 * 1024);
            let l = listener.clone();
            let server_task = tokio::spawn(async move {
                l.serve_one(&mut server).await.unwrap();
            });
            let msg = BrokerMessage::SessionInfo(BrokerSessionRequest {
                token: "tok".into(),
                session_id: id,
                max_messages: Some(0),
            });
            write_frame(&mut client, &msg).await.unwrap();
            let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
            server_task.await.unwrap();
            assert_eq!(resp.outcome["found"], true, "id {id} should resolve");
            assert_eq!(resp.outcome["session_id"], id);
        }
    }

    /// An invalid token yields a `found:false` outcome WITHOUT touching the
    /// resolver (no leak of whether the session exists).
    #[tokio::test]
    async fn session_info_invalid_token_is_not_found_without_resolving() {
        let session_info = Arc::new(StubSessionInfo {
            known: std::collections::HashSet::from([42]),
            ..Default::default()
        });
        // No token registered.
        let tokens = Arc::new(TokenRegistry::default());
        let listener = make_session_listener(tokens, session_info.clone());

        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let msg = BrokerMessage::SessionInfo(BrokerSessionRequest {
            token: "bogus".into(),
            session_id: 42,
            max_messages: None,
        });
        write_frame(&mut client, &msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();

        assert_eq!(resp.outcome["found"], false);
        assert_eq!(resp.outcome["session_id"], 42);
        // The resolver was never consulted for an unauthenticated caller.
        assert!(session_info.calls.lock().await.is_empty());
    }

    /// `CommitFeedback` marks the named ids delivered, scoped (via the token) to
    /// the parent connection — the companion sends this only after it delivers.
    #[tokio::test]
    async fn commit_feedback_marks_delivered_scoped_to_parent() {
        let feedback = Arc::new(StubFeedback::default());
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_feedback_listener(tokens, feedback.clone());

        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let msg = BrokerMessage::CommitFeedback(BrokerCommitFeedbackRequest {
            token: "tok".into(),
            ids: vec!["f1".into(), "f2".into()],
        });
        write_frame(&mut client, &msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        assert!(resp.outcome.is_null(), "commit ack is empty");

        let committed = feedback.committed.lock().await;
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].0, "parent-conn");
        assert_eq!(committed[0].1, vec!["f1".to_string(), "f2".to_string()]);
    }

    /// An invalid token on `CommitFeedback` is a silent no-op (no commit).
    #[tokio::test]
    async fn commit_feedback_invalid_token_is_noop() {
        let feedback = Arc::new(StubFeedback::default());
        let listener = make_feedback_listener(Arc::new(TokenRegistry::default()), feedback.clone());
        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        write_frame(
            &mut client,
            &BrokerMessage::CommitFeedback(BrokerCommitFeedbackRequest {
                token: "bad".into(),
                ids: vec!["f1".into()],
            }),
        )
        .await
        .unwrap();
        let _: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        assert!(feedback.committed.lock().await.is_empty());
    }

    /// An invalid token returns an empty `{ count: 0 }` envelope (no leak of
    /// whether any feedback exists), never reads the store, and commits nothing.
    #[tokio::test]
    async fn feedback_invalid_token_returns_empty() {
        let feedback = Arc::new(StubFeedback::default());
        *feedback.items.lock().await = vec![pending("f1", "should never be returned")];
        let tokens = Arc::new(TokenRegistry::default());
        let listener = make_feedback_listener(tokens, feedback.clone());

        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        let msg = BrokerMessage::Feedback(BrokerFeedbackRequest {
            token: "bad-token".into(),
        });
        write_frame(&mut client, &msg).await.unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();

        assert_eq!(resp.outcome["count"], 0);
        assert!(resp.outcome["feedback"].as_array().unwrap().is_empty());
        // The store was never read or committed for an unknown token.
        assert!(feedback.read_conn.lock().await.is_none());
        assert!(feedback.committed.lock().await.is_empty());
    }

    // --- ask_user_question over the listener -------------------------------

    fn ask_msg(token: &str) -> BrokerMessage {
        BrokerMessage::Ask(BrokerAskRequest {
            token: token.into(),
            questions: vec![crate::acp::question::QuestionSpec {
                id: "qq-1".into(),
                question: "Which approach?".into(),
                header: "Approach".into(),
                multi_select: false,
                options: vec![
                    crate::acp::question::QuestionOption {
                        label: "Incremental".into(),
                        description: String::new(),
                    },
                    crate::acp::question::QuestionOption {
                        label: "Rewrite".into(),
                        description: String::new(),
                    },
                ],
            }],
        })
    }

    use crate::acp::question::QuestionAnsweredItem;

    /// An `Ask` registers the question, parks, and — once the user answers —
    /// writes the `{ answers, declined }` envelope back over the same socket.
    #[tokio::test]
    async fn ask_registers_then_answer_resolves_response() {
        let questions = Arc::new(StubQuestion::default());
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_question_listener(tokens, questions.clone());

        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        write_frame(&mut client, &ask_msg("tok")).await.unwrap();

        // The server must be parked until an answer arrives — no response yet.
        let early = tokio::time::timeout(Duration::from_millis(40), async {
            read_frame::<_, BrokerResponse>(&mut client).await
        })
        .await;
        assert!(early.is_err(), "ask must block until the user answers");

        // Wait for the stub to record the registration, then answer it.
        while questions.registered.lock().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(questions.registered.lock().await[0].0, "parent-conn");
        questions
            .answer(
                "q-1",
                QuestionOutcome {
                    answers: vec![QuestionAnsweredItem {
                        question: "Which approach?".into(),
                        header: "Approach".into(),
                        multi_select: false,
                        selected: vec!["Incremental".into()],
                    }],
                    declined: false,
                },
            )
            .await;

        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        assert_eq!(resp.outcome["declined"], false);
        assert_eq!(resp.outcome["answers"][0]["selected"][0], "Incremental");
        assert_eq!(resp.outcome["answers"][0]["header"], "Approach");
    }

    /// A canceled tool call drops the request socket; the listener observes the
    /// peer-close, cancels the pending question, and returns without writing.
    #[tokio::test]
    async fn ask_peer_close_cancels_question() {
        let questions = Arc::new(StubQuestion::default());
        let tokens = Arc::new(TokenRegistry::default());
        tokens
            .register("tok".into(), test_token_entry("parent-conn"))
            .await;
        let listener = make_question_listener(tokens, questions.clone());

        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move { listener.serve_one(&mut server).await });
        write_frame(&mut client, &ask_msg("tok")).await.unwrap();

        // Let the server park inside the wait.
        while questions.registered.lock().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Companion cancels: drop the request socket.
        drop(client);

        let result = tokio::time::timeout(Duration::from_secs(5), server_task)
            .await
            .expect("serve_one must return after peer close");
        result.unwrap().unwrap();
        assert_eq!(
            questions.canceled.lock().await.as_slice(),
            &["q-1".to_string()]
        );
    }

    /// An invalid token never registers a question and returns a `declined`
    /// outcome (the LLM proceeds with its own judgment).
    #[tokio::test]
    async fn ask_invalid_token_declined() {
        let questions = Arc::new(StubQuestion::default());
        let listener =
            make_question_listener(Arc::new(TokenRegistry::default()), questions.clone());
        let (mut client, mut server) = duplex(8 * 1024);
        let server_task = tokio::spawn(async move {
            listener.serve_one(&mut server).await.unwrap();
        });
        write_frame(&mut client, &ask_msg("bad-token"))
            .await
            .unwrap();
        let resp: BrokerResponse = read_frame(&mut client).await.unwrap();
        server_task.await.unwrap();
        assert_eq!(resp.outcome["declined"], true);
        assert!(questions.registered.lock().await.is_empty());
    }
}
