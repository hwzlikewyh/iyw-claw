use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};
use std::time::{Duration, Instant};

use futures::future::join_all;
use tokio::sync::Mutex;

use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ActiveValue::Set, DatabaseConnection, EntityTrait,
    TransactionTrait,
};

use crate::acp::agent_image_input::{
    prepare_codex_image_inputs, validate_codex_image_inputs, CodexImageScope,
};
use crate::acp::connection::{spawn_agent_connection, AgentConnection, ConnectionCommand};
use crate::acp::connection_tasks::ConnectionTaskRegistry;
use crate::acp::error::AcpError;
use crate::acp::feedback::{
    bounded_feedback_batch, FeedbackItem, FeedbackStatus, PendingFeedback, SessionFeedbackAccess,
    MAX_FEEDBACK_CHARS, MAX_FEEDBACK_RESPONSE_BYTES,
};
use crate::acp::operation_gate::AgentOperationGate;
pub(crate) use crate::acp::operation_gate::RelaunchClaim;
use crate::acp::operation_guard::active_operation_reason;
use crate::acp::prompt_stall::{assess_prompt_stall, PromptStallAssessment, PromptStallInput};
use crate::acp::question::{
    build_outcome, QuestionAnswer, QuestionOutcome, QuestionSpec, RegisteredQuestion,
    SessionQuestionAccess,
};
use crate::acp::session_state::SessionState;
use crate::acp::types::{
    AcpEvent, AgentOptionsSnapshot, ConfigStaleKind, ConnectionInfo, ConnectionStatus,
    ForkResultInfo, PromptInputBlock,
};
use crate::chat_channel::manager::ChatChannelManager;
use crate::commands::conversation_title::{self, ConversationTitleContext};
use crate::db::entities::conversation::{self, ConversationKind, ConversationStatus};
use crate::db::service::conversation_service;
use crate::db::AppDatabase;

fn combine_prompt_context(launch: Option<Arc<str>>, private: Option<Arc<str>>) -> Option<Arc<str>> {
    match (launch, private) {
        (None, None) => None,
        (Some(context), None) | (None, Some(context)) => Some(context),
        (Some(launch), Some(private)) => Some(Arc::from(format!("{launch}\n\n{private}"))),
    }
}

fn has_prompt_images(blocks: &[PromptInputBlock]) -> bool {
    blocks
        .iter()
        .any(|block| matches!(block, PromptInputBlock::Image { .. }))
}

use crate::models::agent::AgentType;
use crate::web::event_bridge::{emit_with_state, emit_with_state_gated, EventEmitter};

/// Cap on the number of prompt-text chars kept in the `user_prompt_sent`
/// preview. Past this, `truncate_str` keeps this many chars and appends a short
/// `...` marker (so the rendered string can be a few chars longer). Bounds the
/// event payload so a large paste can't bloat the ring buffer, the per-channel
/// IM message, or the webhook body.
const USER_PROMPT_PREVIEW_MAX_CHARS: usize = 500;
const HERMES_HOME_LIVE_COUNT_CAP: usize = 1024;
const CONNECTION_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECTION_CLEANUP_POLL: Duration = Duration::from_millis(10);

pub struct ConnectionShutdownReport {
    pub disconnected: usize,
    pub completed: bool,
}

async fn wait_for_connection_cleanup(
    connection_id: String,
    cleanup_completed: Arc<AtomicBool>,
) -> bool {
    let deadline = Instant::now() + CONNECTION_CLEANUP_TIMEOUT;
    while !cleanup_completed.load(Ordering::Acquire) {
        let now = Instant::now();
        if now >= deadline {
            tracing::warn!(
                connection_id,
                timeout_ms = CONNECTION_CLEANUP_TIMEOUT.as_millis(),
                "[ACP] connection async cleanup timed out during shutdown"
            );
            return false;
        }
        tokio::time::sleep((deadline - now).min(CONNECTION_CLEANUP_POLL)).await;
    }
    true
}

async fn cleanup_connection_authorities(
    connection_id: String,
    builtin_mcp: Option<crate::acp::builtin_mcp::BuiltinMcpClient>,
    delegation: Option<crate::acp::connection::DelegationInjection>,
) -> (String, bool) {
    let cleanup = async {
        if let Some(client) = builtin_mcp {
            client.revoke_parent(&connection_id).await;
        }
        if let Some(injection) = delegation {
            injection.tokens.revoke_by_parent(&connection_id).await;
            injection.broker.cancel_by_parent(&connection_id).await;
            injection
                .questions
                .cancel_questions_by_parent(&connection_id)
                .await;
            injection
                .confirmations
                .cancel_channel_confirmations_by_parent(&connection_id)
                .await;
        }
    };
    let completed = tokio::time::timeout(CONNECTION_CLEANUP_TIMEOUT, cleanup)
        .await
        .is_ok();
    if !completed {
        tracing::error!(
            connection_id,
            timeout_ms = CONNECTION_CLEANUP_TIMEOUT.as_millis(),
            "[ACP] connection authority cleanup timed out during shutdown"
        );
    }
    (connection_id, completed)
}

/// True for ids in the parsers' turn-id namespace (`turn-<digits>`), which every
/// parser assigns via `format!("turn-{}", n)`. A broadcast `message_id` must
/// never land here: it would collide with a persisted transcript turn id and let
/// id-keyed cross-client dedup suppress or hide a prompt. Used to reject an
/// untrusted client-supplied `message_id` of that shape.
fn is_reserved_turn_id(id: &str) -> bool {
    matches!(id.strip_prefix("turn-"), Some(rest)
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

/// Build the bounded preview string for a `user_prompt_sent` notification from
/// the `Text` blocks of a user prompt. Joins the (trimmed, non-empty) text
/// blocks with a space and caps the kept text at `USER_PROMPT_PREVIEW_MAX_CHARS`
/// chars (a `...` marker is appended past the cap). Returns `None` when the
/// prompt carries no text (e.g. image-only) — the notification fires for text
/// messages only.
fn user_prompt_text_preview(blocks: &[PromptInputBlock]) -> Option<String> {
    let joined = blocks
        .iter()
        .filter_map(|b| match b {
            PromptInputBlock::Text { text } => {
                let t = text.trim();
                (!t.is_empty()).then_some(t)
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(crate::parsers::truncate_str(
            trimmed,
            USER_PROMPT_PREVIEW_MAX_CHARS,
        ))
    }
}

async fn wait_for_launch_finalization(
    cmd_tx: &tokio::sync::mpsc::Sender<ConnectionCommand>,
    state: &Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
) -> Result<(), AcpError> {
    loop {
        let notified = {
            let snapshot = state.read().await;
            if snapshot.launch_finalized {
                return Ok(());
            }
            snapshot.launch_ready.clone().notified_owned()
        };
        tokio::select! {
            _ = notified => {}
            _ = cmd_tx.closed() => return Err(AcpError::ProcessExited),
        }
    }
}

struct UserMemoryStalenessSnapshot {
    agent_type: AgentType,
    origin: crate::user_memory::UserMemoryOrigin,
    fingerprint: String,
    context_known: bool,
    launch_finalized: bool,
    status: ConnectionStatus,
}

impl UserMemoryStalenessSnapshot {
    fn eligible(&self) -> bool {
        self.launch_finalized
            && !matches!(
                self.status,
                ConnectionStatus::Disconnected | ConnectionStatus::Error
            )
    }
}

async fn user_memory_staleness_snapshot(
    state: &Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
) -> UserMemoryStalenessSnapshot {
    let state = state.read().await;
    UserMemoryStalenessSnapshot {
        agent_type: state.agent_type,
        origin: state.user_memory_context.origin,
        fingerprint: state.user_memory_context.effective_fingerprint.clone(),
        context_known: state.user_memory_capabilities.read_context.reason
            != crate::user_memory::UserMemoryCapabilityReason::NotEvaluated,
        launch_finalized: state.launch_finalized,
        status: state.status.clone(),
    }
}

/// Seed title for a freshly-created delegation child row, derived from the
/// delegating prompt's text blocks (the sub-agent's task). Uses the parser's own
/// `title_from_user_text` (folds reference links, caps at 100 chars) so the value
/// matches what the fallback-title refresh later computes from that same first
/// turn — the conditional UPDATE then sees no change and doesn't churn. Returns
/// `None` for a textless prompt, leaving the title unset to be backfilled on
/// first detail load as before. Kept unlocked by the caller so an AI-generated
/// title can still replace it later.
fn delegation_child_title_seed(blocks: &[PromptInputBlock]) -> Option<String> {
    let joined = blocks
        .iter()
        .filter_map(|b| match b {
            PromptInputBlock::Text { text } => {
                let t = text.trim();
                (!t.is_empty()).then_some(t)
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(crate::parsers::title_from_user_text(trimmed))
    }
}

/// Composite key identifying a logical agent session for spawn-time dedup.
/// Two `acp_connect` calls with the same triple race for the same `Mutex`,
/// so the second one observes the first's freshly-spawned connection in
/// `find_connection_for_reuse` instead of starting a duplicate process.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SpawnDedupKey {
    agent_type: AgentType,
    working_dir: Option<PathBuf>,
    session_id: String,
}

/// Default upper bound on how long `spawn_agent` will hold the per-session
/// dedup lock waiting for `SessionStarted`. Picked to comfortably cover
/// cold-start agents, including a full Codex profile scan, without deadlocking
/// the next concurrent acp_connect when an agent is genuinely broken.
pub(crate) const SPAWN_HANDSHAKE_TIMEOUT_SECS: u64 = 150;

/// Read the spawn-handshake timeout from `IYW_CLAW_ACP_SPAWN_HANDSHAKE_TIMEOUT_SECS`,
/// falling back to `SPAWN_HANDSHAKE_TIMEOUT_SECS`. Returns the configured
/// `Duration`. Tests can construct the manager with a custom value via
/// `with_spawn_handshake_timeout` instead of mutating env.
fn spawn_handshake_timeout_from_env() -> Duration {
    let secs = std::env::var("IYW_CLAW_ACP_SPAWN_HANDSHAKE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(SPAWN_HANDSHAKE_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

/// Outcome of the `spawn_agent` dedup wait. Logged so production can audit
/// how often the timeout fires vs. the agent handshake completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandshakeWaitOutcome {
    /// `SessionStarted` applied; `external_id` is now set on the state.
    Ready,
    /// Sender was dropped before SessionStarted fired (typically the
    /// connection died during init — `run_connection` returned Err).
    Aborted,
    /// Timeout elapsed before either of the above. Releases the dedup lock
    /// so the next caller can proceed; the slow agent is no worse off.
    TimedOut,
}

impl HandshakeWaitOutcome {
    fn as_str(self) -> &'static str {
        match self {
            HandshakeWaitOutcome::Ready => "ready",
            HandshakeWaitOutcome::Aborted => "aborted",
            HandshakeWaitOutcome::TimedOut => "timeout",
        }
    }
}

/// Wait for the spawn-time `SessionStarted` signal, bounded by `timeout`.
/// Extracted so the outcome enum can be unit-tested without spawning a
/// real agent process.
async fn wait_for_session_started(
    rx: tokio::sync::oneshot::Receiver<()>,
    timeout: Duration,
) -> (HandshakeWaitOutcome, Duration) {
    let start = std::time::Instant::now();
    let outcome = match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(())) => HandshakeWaitOutcome::Ready,
        Ok(Err(_)) => HandshakeWaitOutcome::Aborted,
        Err(_) => HandshakeWaitOutcome::TimedOut,
    };
    (outcome, start.elapsed())
}

pub struct ConnectionManager {
    pub(crate) connections: Arc<Mutex<HashMap<String, AgentConnection>>>,
    connection_tasks: Arc<ConnectionTaskRegistry>,
    shutdown_cleanup_pending: Arc<Mutex<HashSet<String>>>,
    runtime_hosts: Arc<crate::acp::runtime_host::RuntimeHostRegistry>,
    /// Per-(agent, working_dir, session_id) async mutex. Held across the
    /// dedup-lookup + spawn + SessionStarted-wait critical section so two
    /// concurrent `spawn_agent` calls for the same logical session can't
    /// both miss dedup during the handshake window. Weak entries are pruned on
    /// lookup once no active/waiting spawn still owns the per-session lock.
    spawn_locks: Arc<Mutex<HashMap<SpawnDedupKey, Weak<Mutex<()>>>>>,
    /// Bound on how long `spawn_agent` waits for the agent's handshake
    /// before releasing the dedup lock. Configurable per-instance for
    /// tests; in production initialized from env via
    /// `spawn_handshake_timeout_from_env`.
    spawn_handshake_timeout: Duration,
    /// Delegation services installed during app bootstrap (`install_delegation`).
    /// When present, `spawn_agent` propagates them to `spawn_agent_connection`,
    /// which issues a per-connection built-in HTTP MCP lease during ACP init.
    /// `Arc<OnceLock>` ensures inner clones see the one-time installation.
    delegation_injection: Arc<std::sync::OnceLock<crate::acp::connection::DelegationInjection>>,
    /// Ready process-wide HTTP MCP client installed by desktop/server bootstrap.
    /// The lifecycle owner stays at the entrypoint; connection clones can only
    /// issue and revoke their own session authority.
    builtin_mcp: Arc<std::sync::OnceLock<crate::acp::builtin_mcp::BuiltinMcpClient>>,
    /// Canonical user-memory service installed once during bootstrap. Shared by
    /// every shallow manager clone so all session entry points capture context
    /// from the same backend-owned store.
    user_memory_service: Arc<std::sync::OnceLock<Arc<crate::user_memory::UserMemoryService>>>,
    version_center_db: Arc<std::sync::OnceLock<DatabaseConnection>>,
    version_center_data_dir: Arc<std::sync::OnceLock<PathBuf>>,
    capability_policy:
        Arc<std::sync::OnceLock<crate::acp::capability_policy::CapabilityPolicyStore>>,
    memory_pressure_tracker: Arc<Mutex<crate::acp::resource_governor::MemoryPressureTracker>>,
    stalled_prompt_observations: Arc<Mutex<HashSet<(String, i64)>>>,
    operation_gate: Arc<AgentOperationGate>,
    agent_activation_locks: Arc<Mutex<HashMap<AgentType, Arc<Mutex<()>>>>>,
    /// Per-agent-type serialization for `probe_agent_options`. Without
    /// this, rapid agent-tab clicks in the settings UI would fan out one
    /// real CLI process per click — each one running up to 60s. The
    /// mutex bounds concurrent probes for the same agent_type to one;
    /// different agent_types remain parallel.
    probe_locks: Arc<Mutex<HashMap<AgentType, Arc<tokio::sync::Mutex<()>>>>>,
    /// In-flight `ask_user_question` calls awaiting the user's answer, keyed by
    /// the globally-unique `question_id`. The listener parks on the receiver;
    /// the answer / cancel path resolves (and removes) the matching sender.
    /// Shared across `clone_ref` clones so the listener-facing
    /// `register_question` and the command-facing `answer_question` touch the
    /// same map. Size tracks live concurrency (the agent is blocked per ask) —
    /// no cap, no cumulative growth; entries are removed on answer / cancel /
    /// connection teardown.
    pending_questions: Arc<Mutex<HashMap<String, PendingQuestionEntry>>>,
    pub(crate) pending_channel_confirmations:
        Arc<Mutex<HashMap<String, PendingChannelConfirmationEntry>>>,
    pub(crate) agent_input_runtime: Arc<crate::acp::agent_input_dispatch::AgentInputRuntime>,
}

/// A parked `ask_user_question` awaiting its answer. The `sender` resolves the
/// blocked listener round-trip; `questions` is retained so `answer_question` can
/// build the self-describing outcome without a `SessionState` read (race-free).
struct PendingQuestionEntry {
    parent_connection_id: String,
    questions: Vec<QuestionSpec>,
    sender: tokio::sync::oneshot::Sender<QuestionOutcome>,
}

type StalledPromptObservation = (String, i64, Option<u32>, PromptStallAssessment);

pub(crate) struct PendingChannelConfirmationEntry {
    pub parent_connection_id: String,
    pub sender: tokio::sync::oneshot::Sender<
        crate::acp::channel_tools::confirmation::ChannelConfirmationOutcome,
    >,
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionManager {
    const VISIBLE_LEASE_SECS: i64 = 45;
    const PENDING_INPUT_LEASE_SECS: i64 = 120;

    pub(crate) async fn acquire_operation_read(
        &self,
    ) -> Result<tokio::sync::OwnedRwLockReadGuard<()>, AcpError> {
        self.operation_gate.acquire_read().await
    }

    pub(crate) async fn claim_relaunch(&self) -> Result<RelaunchClaim, AcpError> {
        let claim = self.operation_gate.try_claim_relaunch()?;
        if self.has_active_agent_operations().await {
            return Err(AcpError::protocol(
                "Active Agent operations must finish before restarting",
            ));
        }
        Ok(claim)
    }
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            connection_tasks: Arc::new(Default::default()),
            shutdown_cleanup_pending: Arc::new(Mutex::new(HashSet::new())),
            runtime_hosts: Arc::new(Default::default()),
            spawn_locks: Arc::new(Mutex::new(HashMap::new())),
            spawn_handshake_timeout: spawn_handshake_timeout_from_env(),
            delegation_injection: Arc::new(std::sync::OnceLock::new()),
            builtin_mcp: Arc::new(std::sync::OnceLock::new()),
            user_memory_service: Arc::new(std::sync::OnceLock::new()),
            version_center_db: Arc::new(std::sync::OnceLock::new()),
            version_center_data_dir: Arc::new(std::sync::OnceLock::new()),
            capability_policy: Arc::new(std::sync::OnceLock::new()),
            memory_pressure_tracker: Arc::new(Mutex::new(Default::default())),
            stalled_prompt_observations: Arc::new(Mutex::new(HashSet::new())),
            operation_gate: Arc::new(AgentOperationGate::default()),
            agent_activation_locks: Arc::new(Mutex::new(HashMap::new())),
            probe_locks: Arc::new(Mutex::new(HashMap::new())),
            pending_questions: Arc::new(Mutex::new(HashMap::new())),
            pending_channel_confirmations: Arc::new(Mutex::new(HashMap::new())),
            agent_input_runtime: Arc::new(Default::default()),
        }
    }

    /// Returns a shallow clone sharing the same underlying connection map.
    pub fn clone_ref(&self) -> Self {
        Self {
            connections: self.connections.clone(),
            connection_tasks: self.connection_tasks.clone(),
            shutdown_cleanup_pending: self.shutdown_cleanup_pending.clone(),
            runtime_hosts: self.runtime_hosts.clone(),
            spawn_locks: self.spawn_locks.clone(),
            spawn_handshake_timeout: self.spawn_handshake_timeout,
            delegation_injection: self.delegation_injection.clone(),
            builtin_mcp: self.builtin_mcp.clone(),
            user_memory_service: self.user_memory_service.clone(),
            version_center_db: self.version_center_db.clone(),
            version_center_data_dir: self.version_center_data_dir.clone(),
            capability_policy: self.capability_policy.clone(),
            memory_pressure_tracker: self.memory_pressure_tracker.clone(),
            stalled_prompt_observations: self.stalled_prompt_observations.clone(),
            operation_gate: self.operation_gate.clone(),
            agent_activation_locks: self.agent_activation_locks.clone(),
            probe_locks: self.probe_locks.clone(),
            pending_questions: self.pending_questions.clone(),
            pending_channel_confirmations: self.pending_channel_confirmations.clone(),
            agent_input_runtime: self.agent_input_runtime.clone(),
        }
    }

    /// Set the delegation injection context exactly once during bootstrap.
    /// Calling twice is a no-op — protects against accidental re-init in
    /// the unlikely event a second `build_delegation_stack` runs.
    pub fn install_delegation(&self, injection: crate::acp::connection::DelegationInjection) {
        let _ = self.delegation_injection.set(injection);
    }

    pub fn install_builtin_mcp(&self, client: crate::acp::builtin_mcp::BuiltinMcpClient) {
        let _ = self.builtin_mcp.set(client);
    }

    pub fn install_user_memory(&self, service: Arc<crate::user_memory::UserMemoryService>) {
        let _ = self.user_memory_service.set(service);
    }

    pub fn install_version_center(&self, conn: DatabaseConnection, data_dir: PathBuf) {
        let _ = self.version_center_db.set(conn);
        let _ = self.version_center_data_dir.set(data_dir);
    }

    pub fn install_capability_policy(
        &self,
        store: crate::acp::capability_policy::CapabilityPolicyStore,
    ) {
        let _ = self.capability_policy.set(store);
    }

    pub(crate) async fn authorize_agent_install(
        &self,
        agent_type: AgentType,
    ) -> Result<(), AcpError> {
        self.require_agent_launch_policy(agent_type, true).await
    }

    async fn require_agent_launch_policy(
        &self,
        agent_type: AgentType,
        runtime_verified: bool,
    ) -> Result<(), AcpError> {
        let conn = self
            .version_center_db
            .get()
            .ok_or_else(|| AcpError::protocol("Agent platform authorization is not initialized"))?;
        let store = self
            .capability_policy
            .get()
            .ok_or_else(|| AcpError::protocol("Agent capability policy is not initialized"))?;
        let result = crate::commands::capability_policy::require_existing_agent_capability_core(
            conn,
            store,
            agent_type,
            crate::acp::capability_policy::Capability::AgentLaunch,
            runtime_verified,
        )
        .await;
        result.map_err(|error| {
            let mapped = AcpError::from_capability_error(error);
            tracing::warn!(agent = %agent_type, error = %mapped, "[capability-policy] Agent launch denied");
            mapped
        })
    }

    pub(super) fn agent_image_data_dir(&self) -> Result<&Path, AcpError> {
        self.version_center_data_dir
            .get()
            .map(PathBuf::as_path)
            .ok_or_else(|| AcpError::protocol("Agent platform data directory is not initialized"))
    }

    pub(super) async fn codex_image_scope(
        state: &Arc<tokio::sync::RwLock<SessionState>>,
        conversation_id: Option<i32>,
    ) -> CodexImageScope {
        let snapshot = state.read().await;
        CodexImageScope {
            connection_id: snapshot.connection_id.clone(),
            conversation_id: snapshot.conversation_id.or(conversation_id),
            working_dir: snapshot.working_dir.clone(),
        }
    }

    pub(crate) async fn prepare_agent_image_inputs(
        &self,
        agent_type: AgentType,
        state: &Arc<tokio::sync::RwLock<SessionState>>,
        conversation_id: Option<i32>,
        blocks: Vec<PromptInputBlock>,
    ) -> Result<Vec<PromptInputBlock>, AcpError> {
        crate::acp::capability_policy::require_prompt_file_upload(&blocks)
            .await
            .map_err(AcpError::from_capability_error)?;
        if agent_type != AgentType::Codex || !has_prompt_images(&blocks) {
            return Ok(blocks);
        }
        let scope = Self::codex_image_scope(state, conversation_id).await;
        prepare_codex_image_inputs(self.agent_image_data_dir()?, scope, blocks).await
    }

    async fn validate_agent_image_inputs(
        &self,
        agent_type: AgentType,
        state: &Arc<tokio::sync::RwLock<SessionState>>,
        blocks: &[PromptInputBlock],
    ) -> Result<(), AcpError> {
        crate::acp::capability_policy::require_prompt_file_upload(blocks)
            .await
            .map_err(AcpError::from_capability_error)?;
        if agent_type != AgentType::Codex || !has_prompt_images(blocks) {
            return Ok(());
        }
        let scope = Self::codex_image_scope(state, None).await;
        validate_codex_image_inputs(self.agent_image_data_dir()?, scope, blocks).await
    }

    pub(crate) async fn begin_agent_activation(
        &self,
        agent_type: AgentType,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.agent_activation_locks.lock().await;
            locks
                .entry(agent_type)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        lock.lock_owned().await
    }

    async fn user_memory_context_for(
        &self,
        agent_type: AgentType,
        origin: crate::user_memory::UserMemoryOrigin,
    ) -> crate::user_memory::UserMemoryContextSnapshot {
        let Some(service) = self.user_memory_service.get() else {
            return crate::user_memory::UserMemoryContextSnapshot::pending(origin);
        };
        match service.launch_context_for(agent_type, origin).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::warn!("[user-memory] failed to build launch context: {error}");
                crate::user_memory::UserMemoryContextSnapshot::pending(origin)
            }
        }
    }

    fn delegation_snapshot(&self) -> Option<crate::acp::connection::DelegationInjection> {
        self.delegation_injection.get().cloned()
    }

    fn builtin_mcp_snapshot(&self) -> Option<crate::acp::builtin_mcp::BuiltinMcpClient> {
        self.builtin_mcp.get().cloned()
    }

    pub(crate) fn builtin_mcp_health_snapshot(
        &self,
    ) -> crate::user_memory::CompanionHealthSnapshot {
        crate::acp::companion_health::builtin_mcp_health(self.builtin_mcp.get())
    }

    pub(crate) fn user_memory_host_bridge_available(&self) -> bool {
        self.delegation_injection
            .get()
            .is_some_and(|injection| injection.tokens.listener_ready())
    }

    pub async fn prewarm_codex_runtime(&self) -> Result<bool, AcpError> {
        let _operation_guard = self.acquire_operation_read().await?;
        if crate::acp::agent_storage_work::has_active_agent_storage_work() {
            tracing::info!("[ACP] skipping Codex runtime prewarm during Agent storage work");
            return Ok(false);
        }
        let _storage_read_guard = crate::acp::agent_storage_work::begin_agent_storage_read().await;
        let resources = crate::acp::resource_governor::ResourceSnapshot::capture();
        if matches!(
            resources.memory.pressure,
            crate::acp::resource_governor::MemoryPressure::Shrinking
                | crate::acp::resource_governor::MemoryPressure::Emergency
        ) {
            tracing::info!(
                pressure = resources.memory.pressure.as_str(),
                available_bytes = resources.memory.available_bytes,
                total_bytes = resources.memory.total_bytes,
                "[ACP] skipping runtime prewarm under memory pressure"
            );
            return Ok(false);
        }
        let version_center_db = self
            .version_center_db
            .get()
            .ok_or_else(|| AcpError::protocol("Agent platform authorization is not initialized"))?;
        let data_dir = self.version_center_data_dir.get().ok_or_else(|| {
            AcpError::protocol("Agent platform data directory is not initialized")
        })?;
        let agent_type = AgentType::Codex;
        let runtime_env = match crate::commands::acp::build_session_runtime_env(
            &AppDatabase {
                conn: version_center_db.clone(),
            },
            agent_type,
            None,
            data_dir,
        )
        .await
        {
            Ok(environment) => environment,
            Err(AcpError::SdkNotInstalled(_)) => {
                tracing::info!("[ACP] skipping Codex runtime prewarm because it is not installed");
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        crate::commands::acp::verify_agent_installed(agent_type, &runtime_env)?;
        self.require_agent_launch_policy(agent_type, true).await?;
        crate::acp::connection::prewarm_agent_runtime(
            agent_type,
            runtime_env,
            self.runtime_hosts.clone(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_agent(
        &self,
        agent_type: AgentType,
        working_dir: Option<String>,
        session_id: Option<String>,
        runtime_env: BTreeMap<String, String>,
        owner_window_label: String,
        emitter: EventEmitter,
        preferred_mode_id: Option<String>,
        preferred_config_values: BTreeMap<String, String>,
    ) -> Result<String, AcpError> {
        let startup_trace = crate::acp::startup_trace::StartupTrace::new(
            agent_type,
            session_id.is_some(),
            "internal",
        );
        self.spawn_agent_with_origin_traced(
            agent_type,
            working_dir,
            session_id,
            runtime_env,
            owner_window_label,
            emitter,
            preferred_mode_id,
            preferred_config_values,
            crate::user_memory::UserMemoryOrigin::Root,
            startup_trace,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn spawn_agent_traced(
        &self,
        agent_type: AgentType,
        working_dir: Option<String>,
        session_id: Option<String>,
        runtime_env: BTreeMap<String, String>,
        owner_window_label: String,
        emitter: EventEmitter,
        preferred_mode_id: Option<String>,
        preferred_config_values: BTreeMap<String, String>,
        startup_trace: crate::acp::startup_trace::StartupTrace,
    ) -> Result<String, AcpError> {
        self.spawn_agent_with_origin_traced(
            agent_type,
            working_dir,
            session_id,
            runtime_env,
            owner_window_label,
            emitter,
            preferred_mode_id,
            preferred_config_values,
            crate::user_memory::UserMemoryOrigin::Root,
            startup_trace,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn_agent_with_origin(
        &self,
        agent_type: AgentType,
        working_dir: Option<String>,
        session_id: Option<String>,
        runtime_env: BTreeMap<String, String>,
        owner_window_label: String,
        emitter: EventEmitter,
        preferred_mode_id: Option<String>,
        preferred_config_values: BTreeMap<String, String>,
        user_memory_origin: crate::user_memory::UserMemoryOrigin,
    ) -> Result<String, AcpError> {
        let source = match user_memory_origin {
            crate::user_memory::UserMemoryOrigin::Root => "internal",
            crate::user_memory::UserMemoryOrigin::Delegation => "delegation",
            crate::user_memory::UserMemoryOrigin::Probe => "probe",
        };
        let startup_trace =
            crate::acp::startup_trace::StartupTrace::new(agent_type, session_id.is_some(), source);
        self.spawn_agent_with_origin_traced(
            agent_type,
            working_dir,
            session_id,
            runtime_env,
            owner_window_label,
            emitter,
            preferred_mode_id,
            preferred_config_values,
            user_memory_origin,
            startup_trace,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn_agent_with_origin_traced(
        &self,
        agent_type: AgentType,
        working_dir: Option<String>,
        session_id: Option<String>,
        runtime_env: BTreeMap<String, String>,
        owner_window_label: String,
        emitter: EventEmitter,
        preferred_mode_id: Option<String>,
        preferred_config_values: BTreeMap<String, String>,
        user_memory_origin: crate::user_memory::UserMemoryOrigin,
        startup_trace: crate::acp::startup_trace::StartupTrace,
    ) -> Result<String, AcpError> {
        let _operation_guard = self.acquire_operation_read().await?;
        let storage_read_guard = crate::acp::agent_storage_work::begin_agent_storage_read().await;
        let mut runtime_env = runtime_env;
        crate::acp::trusted_agents::restrict_configured_runtime_env(agent_type, &mut runtime_env);
        if let Some(required) = crate::acp::trusted_agents::minimum_node_version(agent_type) {
            crate::acp::preflight::enforce_minimum_node_version(&runtime_env, required)
                .await
                .map_err(|error| {
                    AcpError::protocol(format!(
                        "{agent_type} launch blocked: {error}; requires Node.js >={required}"
                    ))
                })?;
        }
        if !crate::acp::registry::is_executable_identity(agent_type) {
            return Err(AcpError::protocol(
                "Agent identity is not trusted for execution",
            ));
        }
        // Connection dedup: when resuming an agent session (session_id is
        // Some), look for a live AgentConnection that already represents
        // the same external session in the same working_dir for the same
        // agent_type and is not torn down. If found, reuse it instead of
        // spawning a fresh process — this is what makes a browser refresh
        // mid-turn re-attach to the existing live state rather than orphan it.
        let working_dir_path = working_dir.as_ref().map(PathBuf::from);

        // Acquire a per-(agent, working_dir, session_id) async mutex so two
        // concurrent connects for the same logical session can't both miss
        // dedup during the handshake window. The lookup → spawn → wait-for-
        // SessionStarted critical section runs under this lock; the second
        // waiter, on entry, observes the first call's connection with
        // `state.external_id` already populated and returns its id via
        // `find_connection_for_reuse`. Skipped entirely when `session_id`
        // is None (fresh sessions can't dedup — by design — since the
        // agent assigns the id).
        let session_id_for_log = session_id.clone();
        let dedup_lock = if let Some(sid) = session_id.as_deref() {
            let key = SpawnDedupKey {
                agent_type,
                working_dir: working_dir_path.clone(),
                session_id: sid.to_string(),
            };
            let mu = {
                let mut locks = self.spawn_locks.lock().await;
                locks.retain(|_, lock| lock.strong_count() > 0);
                if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
                    lock
                } else {
                    let lock = Arc::new(Mutex::new(()));
                    locks.insert(key, Arc::downgrade(&lock));
                    lock
                }
            };
            Some(mu.lock_owned().await)
        } else {
            None
        };

        if let Some(existing) = self
            .find_connection_for_reuse(agent_type, working_dir_path.as_ref(), session_id.as_deref())
            .await
        {
            startup_trace.bind_connection(existing.clone());
            startup_trace.record("connection_reuse", "reused", Duration::ZERO);
            tracing::info!(
                connection_id = existing,
                session_id = session_id.as_deref().unwrap_or(""),
                agent = %agent_type,
                origin = ?user_memory_origin,
                "[ACP] connection reused"
            );
            return Ok(existing);
        }

        let verification_stage = startup_trace.stage("local_runtime_verify");

        let Some(version_center_db) = self.version_center_db.get() else {
            verification_stage.finish("version_center_uninitialized");
            return Err(AcpError::protocol(
                "Agent platform authorization is not initialized",
            ));
        };
        if let Err(error) = crate::commands::acp::verify_agent_installed(agent_type, &runtime_env) {
            verification_stage.finish("inventory_verification_error");
            return Err(error);
        }
        verification_stage.finish("ok");

        let user_memory_context = self
            .user_memory_context_for(agent_type, user_memory_origin)
            .await;

        let connection_id = uuid::Uuid::new_v4().to_string();
        startup_trace.bind_connection(connection_id.clone());
        let managed_version = runtime_env
            .get(crate::commands::acp::MANAGED_AGENT_VERSION_ENV)
            .map(String::as_str)
            .unwrap_or("");
        tracing::info!(
            connection_id,
            agent = %agent_type,
            resumed = session_id.is_some(),
            origin = ?user_memory_origin,
            owner_window = owner_window_label,
            managed_version,
            "[ACP] spawning connection"
        );

        // `spawn_agent_connection` inserts the entry into `self.connections`,
        // installs the SessionStarted dedup signal on the state, registers
        // a cleanup hook, and returns the rx half of the signal. Any spawn
        // failure short-circuits before we touch the rx wait.
        self.require_agent_launch_policy(agent_type, true).await?;
        let session_started_rx = spawn_agent_connection(
            connection_id.clone(),
            agent_type,
            working_dir,
            session_id,
            runtime_env,
            owner_window_label,
            emitter,
            self.connections.clone(),
            self.connection_tasks.clone(),
            self.runtime_hosts.clone(),
            startup_trace,
            preferred_mode_id,
            preferred_config_values,
            user_memory_context,
            matches!(
                user_memory_origin,
                crate::user_memory::UserMemoryOrigin::Delegation
            ),
            self.delegation_snapshot(),
            self.builtin_mcp_snapshot(),
            Some(version_center_db.clone()),
            storage_read_guard,
        )
        .await?;

        // When dedup is active, hold the lock until the agent's
        // SessionStarted has applied (so external_id is populated for the
        // next waiter), aborted (connection died), or the timeout fires.
        // Logged on every wait so production can audit real-world handshake
        // latencies and tune `IYW_CLAW_ACP_SPAWN_HANDSHAKE_TIMEOUT_SECS`.
        if dedup_lock.is_some() {
            let timeout = self.spawn_handshake_timeout;
            let (outcome, elapsed) = wait_for_session_started(session_started_rx, timeout).await;
            tracing::info!(
                connection_id,
                session_id = session_id_for_log.as_deref().unwrap_or(""),
                outcome = outcome.as_str(),
                elapsed_ms = elapsed.as_millis(),
                timeout_ms = timeout.as_millis(),
                "[ACP] deduplication wait completed"
            );
        }
        // session_started_rx (in the no-dedup branch) is dropped here. tx
        // staying inside SessionState gets dropped naturally when the
        // connection terminates, no leak.

        drop(dedup_lock);

        Ok(connection_id)
    }

    /// Renew frontend-owned protection leases for a live connection.
    /// Lease renewal intentionally does not change `last_activity_at`: an open
    /// tab must stay protected while its short lease is alive without making
    /// the session look recently used forever after the renderer disappears.
    /// Silently no-ops if the connection is missing or already terminal.
    pub async fn touch(&self, conn_id: &str) -> bool {
        self.touch_with_leases(conn_id, true, false).await
    }

    pub async fn touch_with_leases(
        &self,
        conn_id: &str,
        visible: bool,
        pending_input: bool,
    ) -> bool {
        let state_arc = {
            let connections = self.connections.lock().await;
            match connections.get(conn_id) {
                Some(conn) => conn.state.clone(),
                None => return false,
            }
        };
        let mut state = state_arc.write().await;
        if matches!(
            state.status,
            ConnectionStatus::Disconnected | ConnectionStatus::Error
        ) {
            return false;
        }
        let now = chrono::Utc::now();
        if visible {
            state.visible_lease_until =
                Some(now + chrono::Duration::seconds(Self::VISIBLE_LEASE_SECS));
        }
        if pending_input {
            state.pending_input_lease_until =
                Some(now + chrono::Duration::seconds(Self::PENDING_INPUT_LEASE_SECS));
        }
        true
    }

    /// Returns true when every live Agent connection has been quiet for at
    /// least `min_idle`. Frontend keepalives do not affect this decision.
    pub async fn is_globally_idle(&self, min_idle: Duration) -> bool {
        let Ok(min_idle) = chrono::Duration::from_std(min_idle) else {
            return false;
        };
        let now = chrono::Utc::now();
        let connections = self.connections.lock().await;
        for connection in connections.values() {
            let Ok(state) = connection.state.try_read() else {
                return false;
            };
            if matches!(
                state.status,
                ConnectionStatus::Disconnected | ConnectionStatus::Error
            ) {
                continue;
            }
            if state.status != ConnectionStatus::Connected
                || state.turn_in_flight
                || state.pending_permission.is_some()
                || state.pending_question.is_some()
                || state.has_active_background_work(now)
                || now.signed_duration_since(state.last_agent_event_at) < min_idle
            {
                return false;
            }
        }
        true
    }

    /// Disconnect connections that have been idle longer than `idle_timeout`.
    /// "Idle" means: status is `Connected`, no `pending_permission`, no
    /// activity (no events, no commands) for at least `idle_timeout`.
    /// `Prompting` connections are always preserved (a turn is in flight).
    /// Returns the number of connections that were disconnected.
    pub async fn sweep_idle(&self, idle_timeout: Duration) -> usize {
        let now = chrono::Utc::now();
        let timeout = match chrono::Duration::from_std(idle_timeout) {
            Ok(d) => d,
            Err(_) => return 0,
        };
        let to_disconnect: Option<String> = {
            let connections = self.connections.lock().await;
            let mut victims = Vec::new();
            for (id, conn) in connections.iter() {
                let Ok(state) = conn.state.try_read() else {
                    // Per-state writer holds the lock; a future tick will
                    // re-evaluate this entry. Don't block the connections
                    // mutex on it.
                    continue;
                };
                if crate::acp::resource_governor::reclaim_block_reason(
                    &state,
                    now,
                    Duration::ZERO,
                    false,
                )
                .is_some()
                {
                    continue;
                }
                let elapsed = now.signed_duration_since(state.last_activity_at);
                if elapsed >= timeout {
                    victims.push((id.clone(), state.last_activity_at));
                }
            }
            victims.sort_by(|left, right| left.1.cmp(&right.1));
            victims.into_iter().next().map(|(id, _)| id)
        };
        let mut disconnected = 0;
        if let Some(id) = to_disconnect {
            if self
                .disconnect_if_reclaimable(&id, Duration::ZERO, false, "idle_timeout")
                .await
                .unwrap_or(false)
            {
                disconnected += 1;
            }
        }
        disconnected
    }

    /// Reclaim at most one recoverable idle session per tick. A user-selected
    /// count cap applies in normal conditions; pressure overrides it to 1 or 0.
    /// Active work, pending input, and unreliable sessions are excluded.
    pub async fn sweep_excess_idle(&self, max_keep: Option<usize>) -> usize {
        let now = chrono::Utc::now();
        let mut resources = crate::acp::resource_governor::ResourceSnapshot::capture();
        let (previous_pressure, pressure) = {
            let mut tracker = self.memory_pressure_tracker.lock().await;
            let previous = tracker.current();
            let pressure = tracker.observe(
                resources.memory.total_bytes,
                resources.memory.available_bytes,
            );
            (previous, pressure)
        };
        resources.memory.pressure = pressure;
        if pressure != previous_pressure
            && pressure != crate::acp::resource_governor::MemoryPressure::Unknown
        {
            tracing::info!(
                previous = previous_pressure.as_str(),
                current = pressure.as_str(),
                available_bytes = resources.memory.available_bytes,
                total_bytes = resources.memory.total_bytes,
                "[ACP] memory pressure state changed"
            );
        }
        let pressure_limit =
            crate::acp::resource_governor::idle_keep_limit(resources.memory.pressure);
        let keep_limit = match (max_keep, pressure_limit) {
            (Some(user_limit), Some(pressure_limit)) => Some(user_limit.min(pressure_limit)),
            (None, pressure_limit) => pressure_limit,
            (user_limit, None) => user_limit,
        };
        let private_budget =
            crate::acp::resource_governor::idle_private_budget(resources.memory.total_bytes);
        let allow_recent_activity = crate::acp::resource_governor::is_hard_emergency(
            resources.memory.total_bytes,
            resources.memory.available_bytes,
        );
        let to_disconnect: Vec<String> = {
            let connections = self.connections.lock().await;
            let mut idle = Vec::new();
            for (id, conn) in connections.iter() {
                let Ok(state) = conn.state.try_read() else {
                    continue;
                };
                if crate::acp::resource_governor::reclaim_block_reason(
                    &state,
                    now,
                    crate::acp::resource_governor::completion_grace(),
                    allow_recent_activity,
                )
                .is_some()
                {
                    continue;
                }
                let memory = resources.connection_memory(&state);
                idle.push((id.clone(), state.last_activity_at, memory.private_bytes));
            }
            let private_total = idle
                .iter()
                .map(|(_, _, bytes)| *bytes)
                .collect::<Option<Vec<_>>>()
                .map(|values| values.iter().sum::<u64>());
            let count_limit_exceeded = keep_limit.is_some_and(|limit| idle.len() > limit);
            // An unlimited preference must remain unlimited while the system is
            // comfortable. Under pressure, private memory becomes a second
            // reclaim signal even when the count has not exceeded its cap.
            let private_budget_exceeded = matches!(
                resources.memory.pressure,
                crate::acp::resource_governor::MemoryPressure::Shrinking
                    | crate::acp::resource_governor::MemoryPressure::Emergency
            ) && private_total
                .is_some_and(|total| total > private_budget);
            if !count_limit_exceeded && !private_budget_exceeded {
                return 0;
            }
            idle.sort_by(|left, right| {
                left.1.cmp(&right.1).then_with(|| {
                    crate::acp::resource_governor::compare_reclaim_memory(left.2, right.2)
                })
            });
            idle.first()
                .map(|(id, _, _)| vec![id.clone()])
                .unwrap_or_default()
        };

        let mut disconnected = 0;
        for id in to_disconnect {
            tracing::info!(
                connection_id = id,
                pressure = resources.memory.pressure.as_str(),
                available_bytes = resources.memory.available_bytes,
                total_bytes = resources.memory.total_bytes,
                shrinking_reserve_bytes = resources.memory.shrinking_reserve_bytes,
                emergency_reserve_bytes = resources.memory.emergency_reserve_bytes,
                keep_limit = ?keep_limit,
                private_budget,
                "[ACP] resource governor disconnecting idle connection"
            );
            if self
                .disconnect_if_reclaimable(
                    &id,
                    crate::acp::resource_governor::completion_grace(),
                    allow_recent_activity,
                    "memory_governor",
                )
                .await
                .unwrap_or(false)
            {
                disconnected += 1;
            }
        }
        disconnected
    }

    /// Observe prompting connections whose agent has gone silent. Silence is
    /// diagnostic evidence, not proof that the operation is dead: a model,
    /// tool, or upstream can remain valid without emitting ACP events. This
    /// watchdog therefore never owns cancellation. Confirmed process/transport
    /// failures are settled by their lifecycle paths; the user can still stop.
    pub async fn observe_stalled_prompts(&self, stall_timeout: Duration) -> usize {
        let observations = self
            .collect_stalled_prompt_observations(stall_timeout)
            .await;
        let active_keys = observations
            .iter()
            .map(|(id, generation, _, _)| (id.clone(), *generation))
            .collect::<HashSet<_>>();
        let mut observed = self.stalled_prompt_observations.lock().await;
        observed.retain(|key| active_keys.contains(key));
        let mut newly_observed = 0;
        for (id, turn_generation, agent_pid, assessment) in observations {
            if !observed.insert((id.clone(), turn_generation)) {
                continue;
            }
            newly_observed += 1;
            tracing::warn!(
                connection_id = %id,
                turn_generation,
                agent_pid = ?agent_pid,
                effective_timeout_secs = assessment.effective_timeout.as_secs(),
                active_foreground_tools = assessment.active_tool_count,
                timeout_source = assessment.timeout_source.as_str(),
                longest_tool_runtime_secs = assessment.longest_tool_runtime.as_secs(),
                silent_for_secs = assessment.silent_for.as_secs(),
                action = "observe_only",
                "[ACP] prompt is silent; retaining active turn until independent failure evidence"
            );
        }
        newly_observed
    }

    async fn collect_stalled_prompt_observations(
        &self,
        stall_timeout: Duration,
    ) -> Vec<StalledPromptObservation> {
        let now = chrono::Utc::now();
        let monotonic_now = Instant::now();
        let connections = self.connections.lock().await;
        let mut observations = Vec::new();
        for (id, conn) in connections.iter() {
            let Ok(state) = conn.state.try_read() else {
                continue;
            };
            if state.status != ConnectionStatus::Prompting
                || state.pending_permission.is_some()
                || state.pending_question.is_some()
                || state.has_active_background_work(now)
            {
                continue;
            }
            let assessment = assess_prompt_stall(PromptStallInput {
                now,
                monotonic_now,
                last_agent_event_at: state.last_agent_event_at,
                base_timeout: stall_timeout,
                active_tool_calls: &state.active_tool_calls,
            });
            if assessment.stalled {
                observations.push((
                    id.clone(),
                    state.turn_generation,
                    state.agent_pid,
                    assessment,
                ));
            }
        }
        observations
    }

    /// Compare each running connection's spawn-time config fingerprint against a
    /// freshly recomputed one (keyed by agent type in `fresh`) and notify those
    /// that drifted. Drives the conversation-side "restart to apply" banner after
    /// a settings save.
    ///
    /// Emit policy, per connection:
    /// - emit `SessionConfigStale { stale }` only when the current fingerprint
    ///   differs from the one we last observed for it — a no-op save (identical
    ///   values) stays silent, while a second real change re-emits so a dismissed
    ///   banner reappears.
    /// - `stale = (current != spawn)`, so reverting a setting back to its
    ///   launch-time value emits `stale = false` and clears the banner.
    ///
    /// Returns the count of running connections currently on stale config across
    /// the affected agents (for the settings-side "N sessions need restart"
    /// toast). Connections whose agent type isn't in `fresh` are left untouched.
    ///
    /// `emit_with_state` is deferred until AFTER the connections-map lock is
    /// released (we collect targets first) so the SessionState write lock is
    /// never taken while holding the map lock.
    pub async fn refresh_connection_staleness(
        &self,
        fresh: &HashMap<AgentType, String>,
        kind: ConfigStaleKind,
    ) -> usize {
        let mut targets = Vec::new();
        let mut stale_count = 0usize;
        {
            let mut connections = self.connections.lock().await;
            for conn in connections.values_mut() {
                let Some(current) = fresh.get(&conn.agent_type) else {
                    continue;
                };
                let stale = *current != conn.config_fingerprint;
                if stale {
                    stale_count += 1;
                }
                if *current != conn.last_observed_fingerprint {
                    conn.last_observed_fingerprint = current.clone();
                    targets.push((Arc::clone(&conn.state), conn.emitter.clone(), stale));
                }
            }
        }
        for (state, emitter, stale) in targets {
            emit_with_state(
                &state,
                &emitter,
                AcpEvent::SessionConfigStale { stale, kind },
            )
            .await;
        }
        stale_count
    }

    /// Look up an existing live connection that we can reuse instead of
    /// spawning a new process. Reuse criteria, ALL must hold:
    /// - `session_id` is Some (we never dedup speculative / fresh connects)
    /// - the connection's `state.external_id` equals `session_id`
    /// - the connection's `agent_type` equals the requested one
    /// - the connection's `working_dir` equals the requested one (compared as
    ///   `Option<PathBuf>` so canonicalization is the caller's concern)
    /// - the connection's `state.status` is neither `Disconnected` nor `Error`
    ///
    /// Per-session state is acquired via `read().await` rather than `try_read`:
    /// the only writer is `emit_with_state`, whose critical section is
    /// microseconds (apply_event + seq++ + broadcast::send), so contention
    /// resolves quickly and the previous "skip on writer" behavior was just
    /// trading correctness (false-negative dedup → duplicate process spawn)
    /// for an imperceptible latency win. The connections-map mutex is held
    /// across the awaits — fine because no path takes `state.write()` while
    /// holding the connections mutex (no lock-cycle).
    pub(crate) async fn find_connection_for_reuse(
        &self,
        agent_type: AgentType,
        working_dir: Option<&PathBuf>,
        session_id: Option<&str>,
    ) -> Option<String> {
        // No session_id → caller is opening a fresh session; never dedup.
        let session_id = session_id?;
        // Collect (id, state_arc) pairs for matching agent_type while holding
        // the connections lock for as short as possible — do NOT await the state
        // read lock while holding the connections lock (that serialises ALL
        // connection map operations for the duration of the loop).
        let candidates: Vec<(
            String,
            Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
        )> = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .filter(|(_, conn)| conn.agent_type == agent_type)
                .map(|(id, conn)| (id.clone(), conn.state.clone()))
                .collect()
        };
        for (id, state_arc) in candidates {
            let state = state_arc.read().await;
            if state.external_id.as_deref() != Some(session_id) {
                continue;
            }
            if state.working_dir.as_ref() != working_dir {
                continue;
            }
            if matches!(
                state.status,
                ConnectionStatus::Disconnected | ConnectionStatus::Error
            ) {
                continue;
            }
            return Some(id);
        }
        None
    }

    /// Forwards a prompt to the connection's command channel without
    /// touching `prompt_lock`. Internal helper — both `send_prompt` and
    /// `send_prompt_linked` acquire the lock externally and then call
    /// this. Re-entering through `send_prompt` from `send_prompt_linked`
    /// while holding the lock would deadlock, hence the split.
    async fn send_prompt_inner(
        &self,
        conn_id: &str,
        blocks: Vec<PromptInputBlock>,
        user_messages: Vec<(String, Vec<crate::acp::UserMessageBlock>)>,
        private_context: Option<Arc<str>>,
        accepted: Option<tokio::sync::oneshot::Sender<()>>,
        upload_monitor: Option<&crate::acp::capability_policy::CapabilityRevocationMonitor>,
    ) -> Result<(), AcpError> {
        // Reject an empty prompt BEFORE touching the concurrency gate. An empty
        // prompt produces no turn — and thus no `TurnComplete` to clear the gate
        // — so enqueuing one with the gate set would wedge the connection into
        // rejecting every future send. `map_prompt_blocks` is 1:1, so empty
        // input blocks is the only way the loop could see an empty prompt; we
        // stop it here at the single shared enqueue path.
        if blocks.is_empty() && private_context.is_none() {
            return Err(AcpError::protocol(
                "prompt must contain at least one content block".to_string(),
            ));
        }
        crate::acp::capability_policy::require_prompt_file_upload(&blocks)
            .await
            .map_err(AcpError::from_capability_error)?;
        let (cmd_tx, state_arc) = self.wait_for_connection_launch(conn_id).await?;
        let agent_type = state_arc.read().await.agent_type;
        self.require_agent_launch_policy(agent_type, true).await?;
        self.validate_agent_image_inputs(agent_type, &state_arc, &blocks)
            .await?;
        let artifact_context =
            crate::acp::task_artifact_delivery::prepare_turn_context(&state_arc, conn_id).await;
        let private_context = combine_prompt_context(private_context, artifact_context);
        // Concurrency gate: reject a second prompt while a turn is already in
        // flight on this connection. Reserve channel capacity FIRST — that
        // `reserve().await` is the only point that can block or be cancelled.
        // Then check+set the gate and hand the command to the permit
        // synchronously: because there is NO await between setting
        // `turn_in_flight` and the infallible `permit.send`, a dropped/cancelled
        // future can never strand the flag true (it is either never set — when
        // cancelled during reserve or the lock acquisition — or always followed
        // by the enqueue). The flag is set BEFORE the enqueue so it is in place
        // before the loop can dequeue and clear it on `TurnComplete`. Without
        // the gate the second `Prompt` would queue behind the active turn and be
        // silently dropped by the loop's in-turn handler (`_ => {}`) while the
        // caller saw success. `send_prompt` callers (e.g. the chat channel)
        // rely on this gate alone (no `prompt_lock`).
        let permit = cmd_tx
            .reserve()
            .await
            .map_err(|_| AcpError::ProcessExited)?;
        let hermes_shared_home_connections =
            self.hermes_shared_home_connection_count(conn_id).await;
        if let Some(monitor) = upload_monitor {
            monitor
                .require_current()
                .await
                .map_err(AcpError::from_capability_error)?;
        }
        let launch_context = {
            let mut s = state_arc.write().await;
            if let Some(monitor) = upload_monitor {
                monitor
                    .error_if_revoked()
                    .map_err(AcpError::from_capability_error)?;
            }
            if s.turn_in_flight || s.turn_completion_pending {
                return Err(AcpError::TurnInProgress);
            }
            s.turn_in_flight = true;
            s.turn_generation = s.turn_generation.saturating_add(1);
            let turn_nonce = s.memory_turn_tracker.begin_accepted_turn();
            s.begin_context_plan_receipt(turn_nonce, hermes_shared_home_connections);
            if s.user_context_injected {
                None
            } else {
                s.user_context_injected = true;
                s.user_memory_context
                    .rendered
                    .clone()
                    .filter(|context| !context.trim().is_empty())
            }
        };
        let user_context = combine_prompt_context(launch_context, private_context);
        permit.send(ConnectionCommand::Prompt {
            blocks,
            user_context,
            user_messages,
            accepted,
        });
        Ok(())
    }

    pub(super) async fn hermes_shared_home_connection_count(&self, conn_id: &str) -> Option<u16> {
        let connections = self.connections.lock().await;
        let home_hash = connections.get(conn_id)?.hermes_home_hash.as_ref()?;
        let count = connections
            .values()
            .filter(|connection| connection.hermes_home_hash.as_ref() == Some(home_hash))
            .take(HERMES_HOME_LIVE_COUNT_CAP)
            .count();
        Some(count as u16)
    }

    /// Clone the connection's `prompt_lock` under a short connections-map lock.
    /// Returned Arc allows the caller to hold the prompt lock without
    /// keeping the connections map locked.
    async fn clone_prompt_lock(
        &self,
        conn_id: &str,
    ) -> Result<Arc<tokio::sync::Mutex<()>>, AcpError> {
        let connections = self.connections.lock().await;
        let conn = connections
            .get(conn_id)
            .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
        Ok(conn.prompt_lock.clone())
    }

    async fn wait_for_connection_launch(
        &self,
        conn_id: &str,
    ) -> Result<
        (
            tokio::sync::mpsc::Sender<ConnectionCommand>,
            Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
        ),
        AcpError,
    > {
        let (cmd_tx, state) = {
            let connections = self.connections.lock().await;
            let connection = connections
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
            (connection.cmd_tx.clone(), connection.state.clone())
        };
        wait_for_launch_finalization(&cmd_tx, &state).await?;
        Ok((cmd_tx, state))
    }

    pub async fn send_prompt(
        &self,
        conn_id: &str,
        blocks: Vec<PromptInputBlock>,
    ) -> Result<(), AcpError> {
        let _operation_guard = self.acquire_operation_read().await?;
        let upload_monitor = crate::acp::capability_policy::monitor_prompt_file_upload(&blocks)
            .await
            .map_err(AcpError::from_capability_error)?;
        let prompt_lock = self.clone_prompt_lock(conn_id).await?;
        let _guard = prompt_lock.lock_owned().await;
        if let Some(monitor) = upload_monitor.as_ref() {
            monitor
                .require_current()
                .await
                .map_err(AcpError::from_capability_error)?;
        }
        let (_, state) = self.wait_for_connection_launch(conn_id).await?;
        let agent_type = state.read().await.agent_type;
        let blocks = self
            .prepare_agent_image_inputs(agent_type, &state, None, blocks)
            .await?;
        self.send_prompt_inner(
            conn_id,
            blocks,
            Vec::new(),
            None,
            None,
            upload_monitor.as_ref(),
        )
        .await
    }

    /// Send a prompt while ensuring a `Conversation` DB row is bound to this
    /// connection. On the first call (when `state.conversation_id` is None),
    /// either:
    /// - **Caller-supplied path** — if `conversation_id` is `Some(id)`, the
    ///   caller (the frontend) has already created the row and we adopt it via
    ///   `ConversationLinked`. Requires `folder_id` to be `Some` so the event
    ///   carries both ids without forcing subscribers to re-query the DB.
    /// - **Backend-creates path** — if `conversation_id` is `None`, we create
    ///   the row from `folder_id` (required) and emit `ConversationLinked`.
    ///   Returns an error if `folder_id` is also `None`.
    ///
    /// Subsequent calls (when state is already linked) ignore both
    /// `folder_id` and `conversation_id` and just forward the prompt.
    ///
    /// Back-compat wrapper for callers that don't supply a client message id
    /// (the delegation broker, internal/test paths). The UI send path uses
    /// [`send_prompt_linked_with_message_id`] so the sender's optimistic turn
    /// dedups against the broadcast `UserMessage` echo by exact id.
    pub async fn send_prompt_linked(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        blocks: Vec<PromptInputBlock>,
        folder_id: Option<i32>,
        conversation_id: Option<i32>,
        delegation: Option<crate::acp::delegation::spawner::DelegationLink>,
    ) -> Result<Option<i32>, AcpError> {
        self.send_prompt_linked_with_message_id(
            db,
            conn_id,
            blocks,
            folder_id,
            conversation_id,
            delegation,
            None,
        )
        .await
    }

    /// As [`send_prompt_linked`], plus an optional `client_message_id`: the
    /// id the sending UI assigned to its own optimistic user turn. When the
    /// user prompt is broadcast as [`AcpEvent::UserMessage`] (for cross-client
    /// viewers), this id becomes the event's `message_id`, so the sender's
    /// runtime dedups the echo against its optimistic turn by EXACT id rather
    /// than a heuristic — and an unrelated optimistic turn on another client
    /// never suppresses a different sender's prompt. `None` falls back to a
    /// connection-scoped id for non-UI senders.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_prompt_linked_with_message_id(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        blocks: Vec<PromptInputBlock>,
        folder_id: Option<i32>,
        conversation_id: Option<i32>,
        delegation: Option<crate::acp::delegation::spawner::DelegationLink>,
        client_message_id: Option<String>,
    ) -> Result<Option<i32>, AcpError> {
        self.send_prompt_linked_with_user_messages(
            db,
            conn_id,
            blocks,
            folder_id,
            conversation_id,
            delegation,
            client_message_id,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_prompt_linked_with_user_messages(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        blocks: Vec<PromptInputBlock>,
        folder_id: Option<i32>,
        conversation_id: Option<i32>,
        delegation: Option<crate::acp::delegation::spawner::DelegationLink>,
        client_message_id: Option<String>,
        user_messages_override: Option<Vec<(String, Vec<crate::acp::UserMessageBlock>)>>,
        accepted: Option<tokio::sync::oneshot::Sender<()>>,
    ) -> Result<Option<i32>, AcpError> {
        let _operation_guard = self.acquire_operation_read().await?;
        let upload_monitor = crate::acp::capability_policy::monitor_prompt_file_upload(&blocks)
            .await
            .map_err(AcpError::from_capability_error)?;
        log_prompt_image_summary(conn_id, client_message_id.as_deref(), &blocks);
        // Reject an empty prompt up front, BEFORE any side effects: linking /
        // creating the conversation row, flipping it to InProgress, or emitting
        // events. An empty prompt is never accepted, so it must not mutate
        // persisted state (create a row, or flip an existing one — which would
        // then be rolled back to Cancelled). `send_prompt_inner` keeps a
        // defensive copy of this guard for the non-linked `send_prompt` path.
        if blocks.is_empty() {
            return Err(AcpError::protocol(
                "prompt must contain at least one content block".to_string(),
            ));
        }
        // Caller-supplied conversation_id requires folder_id (we include it in
        // the emitted ConversationLinked event so subscribers don't have to
        // re-query the DB). Validate before touching any state.
        if conversation_id.is_some() && folder_id.is_none() {
            return Err(AcpError::protocol(
                "conversation_id provided without folder_id".to_string(),
            ));
        }
        // Delegation is only meaningful on the create-new-row branch — adopting
        // an existing caller-supplied row already has its own (or no) parent
        // linkage. Reject the combination loudly so a misuse from the broker
        // doesn't silently drop the linkage.
        if delegation.is_some() && conversation_id.is_some() {
            return Err(AcpError::protocol(
                "delegation link is incompatible with caller-supplied conversation_id".to_string(),
            ));
        }

        // Acquire the per-connection prompt lock for the entire link-check
        // + DB write + emit + cmd_tx.send sequence. Two concurrent prompts
        // (multiple browser tabs of the same conversation; chat-channel
        // racing the UI) are now strictly serialized — the second waiter
        // observes `already_linked == true` after the first commits, so
        // it can't double-create a conversation row.
        let prompt_lock = self.clone_prompt_lock(conn_id).await?;
        let _prompt_guard = prompt_lock.lock_owned().await;
        self.wait_for_connection_launch(conn_id).await?;
        if let Some(monitor) = upload_monitor.as_ref() {
            monitor
                .require_current()
                .await
                .map_err(AcpError::from_capability_error)?;
        }

        // Snapshot what we need from the connection map under one short lock.
        // The conversation-linked check happens INSIDE the prompt lock so
        // any racing send sees a consistent post-link state.
        let (state_arc, emitter, agent_type, already_linked, turn_in_flight) = {
            let connections = self.connections.lock().await;
            let conn = connections
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
            let (already, in_flight) = {
                let s = conn.state.read().await;
                (
                    s.conversation_id.is_some(),
                    s.turn_in_flight || s.turn_completion_pending,
                )
            };
            (
                conn.state.clone(),
                conn.emitter.clone(),
                conn.agent_type,
                already,
                in_flight,
            )
        };

        // Reject a concurrent prompt while a turn is already in flight, BEFORE
        // any side effects (row creation, InProgress emit, user-message
        // broadcast). `send_prompt_inner` re-checks and sets the flag
        // authoritatively below; doing it here too — while still holding
        // `prompt_lock`, so the value can't change underneath us (the loop only
        // ever clears it) — keeps a rejected prompt from flipping the row to
        // InProgress or broadcasting a phantom user message. The frontend turns
        // this rejection into a queued message above the input box.
        if turn_in_flight {
            return Err(AcpError::TurnInProgress);
        }

        let codex_blocks = if agent_type == AgentType::Codex {
            Some(
                self.prepare_agent_image_inputs(
                    agent_type,
                    &state_arc,
                    conversation_id,
                    blocks.clone(),
                )
                .await?,
            )
        } else {
            None
        };
        let image_context = if agent_type == AgentType::Codex {
            None
        } else {
            crate::acp::image_analysis::prepare_prompt_images(self, db, conn_id, &blocks).await?
        };
        let prepared_agent_blocks = if let Some(codex_blocks) = codex_blocks {
            Some(codex_blocks)
        } else if image_context.is_some() {
            Some(
                blocks
                    .iter()
                    .filter(|block| !matches!(block, PromptInputBlock::Image { .. }))
                    .cloned()
                    .collect(),
            )
        } else if agent_type != AgentType::Codex
            && blocks
                .iter()
                .any(|block| matches!(block, PromptInputBlock::Image { uri: Some(_), .. }))
        {
            Some(
                crate::acp::image_analysis::normalize_prompt_images_for_agent(blocks.clone())
                    .await?,
            )
        } else {
            None
        };

        if !already_linked {
            match (conversation_id, folder_id) {
                // Branch A: caller already owns a row — adopt it. No DB write.
                (Some(caller_conv_id), Some(caller_folder_id)) => {
                    emit_with_state(
                        &state_arc,
                        &emitter,
                        AcpEvent::ConversationLinked {
                            conversation_id: caller_conv_id,
                            folder_id: caller_folder_id,
                            parent_conversation_id: None,
                            parent_tool_use_id: None,
                        },
                    )
                    .await;
                }
                // Function-entry guard rejects this combination.
                (Some(_), None) => unreachable!(
                    "conversation_id without folder_id should have been rejected at function entry"
                ),
                // Branch B: backend creates the row from caller-supplied
                // folder_id. Phase 3c-1 made folder_id required here — every
                // production caller that reaches this branch passes one, and
                // silent fallback to working_dir-based find-or-create masked
                // contract violations.
                (None, Some(folder_id)) => {
                    // Snapshot the delegation link before move-into-create: we
                    // still need the parent ids for the ConversationLinked
                    // event payload.
                    let parent_conversation_id_for_event =
                        delegation.as_ref().map(|d| d.parent_conversation_id);
                    let parent_tool_use_id_for_event =
                        delegation.as_ref().map(|d| d.parent_tool_use_id.clone());
                    // Seed a delegation child's title from the task prompt so the
                    // sidebar shows a meaningful label immediately. `list_children`
                    // returns the raw DB title, so a child born with NULL reads
                    // "Untitled" until the first detail load backfills it. Roots
                    // (no delegation) keep `None` and follow the existing backfill.
                    let seed_title = if delegation.is_some() {
                        delegation_child_title_seed(&blocks)
                    } else {
                        None
                    };
                    let row = conversation_service::create_with_delegation(
                        &db.conn,
                        folder_id,
                        agent_type,
                        seed_title,
                        None,
                        delegation.clone(),
                    )
                    .await
                    .map_err(|e| AcpError::protocol(e.to_string()))?;
                    emit_with_state(
                        &state_arc,
                        &emitter,
                        AcpEvent::ConversationLinked {
                            conversation_id: row.id,
                            folder_id,
                            parent_conversation_id: parent_conversation_id_for_event,
                            parent_tool_use_id: parent_tool_use_id_for_event,
                        },
                    )
                    .await;
                    // Sidebar sync: a conversation born here (agent path — a
                    // prompt sent without a pre-created row, not the create
                    // button) must reach every client immediately via the global
                    // `conversation://changed` channel. Roots land in the sidebar
                    // list; delegation children (parent set) are routed into their
                    // parent's expanded sub-session subtree and bump its chevron.
                    // Both carry `external_id: null` here (no session yet) — the
                    // external_id write below re-broadcasts the full summary.
                    crate::commands::conversations::emit_conversation_upsert(
                        &emitter, &db.conn, row.id,
                    )
                    .await;
                    // A new delegation child changes its parent's child_count
                    // (0 → >0 makes the parent's expand chevron appear). Re-emit
                    // the parent so every client converges its count from the
                    // authoritative DB aggregate rather than a drift-prone
                    // per-client increment. The parent may itself be a root or a
                    // nested child — the upsert routes correctly either way by its
                    // own parent_id.
                    if let Some(parent_id) = parent_conversation_id_for_event {
                        crate::commands::conversations::emit_conversation_upsert(
                            &emitter, &db.conn, parent_id,
                        )
                        .await;
                    }
                }
                (None, None) => {
                    return Err(AcpError::protocol(
                        "folder_id required for new conversation row".to_string(),
                    ));
                }
            }

            // UI new-conversation path: SessionStarted applied state.external_id
            // back during acp_connect, but conversation_id was None then so the
            // lifecycle subscriber's SessionStarted handler skipped the DB write.
            // Now that we just linked the row in the same prompt_lock critical
            // section, snapshot external_id and persist it synchronously — no
            // dependence on broadcaster eventual consistency. The chat_channel
            // reverse-order path (link before SessionStarted) is unaffected and
            // continues to be handled by the lifecycle subscriber.
            let (cid_opt, eid_opt, expected_external_id, channel_owned) = {
                let s = state_arc.read().await;
                (
                    s.conversation_id,
                    s.external_id.clone(),
                    s.requested_external_id.clone(),
                    s.owner_window_label.starts_with("chat_channel:"),
                )
            };
            if channel_owned {
                tracing::debug!(
                    connection_id = %conn_id,
                    "[manager] channel session external id is persisted with its route"
                );
            } else if let (Some(cid), Some(eid)) = (cid_opt, eid_opt) {
                let updated = conversation_service::update_external_id_if_matches(
                    &db.conn,
                    cid,
                    expected_external_id.as_deref(),
                    &eid,
                )
                .await
                .map_err(|e| AcpError::protocol(e.to_string()))?;
                if !updated {
                    return Err(AcpError::protocol(
                        "conversation session was replaced by a newer connection".to_string(),
                    ));
                }
                // SessionStarted arrived BEFORE this link, so the lifecycle
                // subscriber skipped its broadcast (no conversation_id then).
                // Now that external_id is persisted, converge every client's
                // sidebar with the complete summary — this also corrects a
                // Branch B upsert above that necessarily carried
                // `external_id: null`. Root-only via the helper.
                crate::commands::conversations::emit_conversation_upsert(&emitter, &db.conn, cid)
                    .await;
            } else if cid_opt.is_some() {
                tracing::info!(
                    "[manager] send_prompt_linked: conversation linked but \
                     external_id not yet on state (conn={conn_id}); lifecycle \
                     subscriber will catch up when SessionStarted arrives"
                );
            }
        }

        // Centralized status transition: every prompt send flips the
        // conversation row to InProgress. This MUST happen on every call
        // (including the already-linked path) so that a follow-up turn whose
        // row is currently `pending_review` correctly transitions back. The
        // DB write precedes the event emit so any subscriber observing
        // `ConversationStatusChanged` can assume the row is consistent.
        // `update_status` is a single UPDATE — idempotent with respect to
        // the same status value, so re-writing `InProgress` is a benign no-op
        // on the row (touches `updated_at` only).
        let conversation_id_for_status = state_arc.read().await.conversation_id;
        if let Some(cid) = conversation_id_for_status {
            conversation_service::update_status(&db.conn, cid, ConversationStatus::InProgress)
                .await
                .map_err(|e| AcpError::protocol(e.to_string()))?;
            emit_with_state(
                &state_arc,
                &emitter,
                AcpEvent::ConversationStatusChanged {
                    conversation_id: cid,
                    status: ConversationStatus::InProgress,
                },
            )
            .await;
        }

        // Capture a bounded preview of the user's message BEFORE `blocks` is
        // moved into `send_prompt_inner`. Only on the genuine UI path
        // (`delegation.is_none()`): delegation / sub-agent prompts are not user
        // messages. Emitted after the send succeeds (below) so a prompt that
        // never reached the agent produces no "user message" notification.
        let user_prompt_preview = if delegation.is_none() {
            user_prompt_text_preview(&blocks)
        } else {
            None
        };

        // Project the user's prompt blocks for the cross-client viewer
        // broadcast BEFORE `send_prompt_inner` consumes `blocks`, and hand the
        // payload to the connection loop (via `ConnectionCommand::Prompt`) so it
        // emits the `UserMessage` event in-order, right before the agent
        // request — guaranteeing its seq precedes the turn's agent events and
        // that it only fires for a prompt actually processed (a failed enqueue
        // delivers no command, so nothing strands a `pending_user_message`).
        // Gated on `delegation.is_none()` (children surface kickoff text
        // separately) and a bound conversation row (a sidebar-visible turn). The
        // `message_id` prefers the sender's client-supplied id (exact echo
        // dedup), falling back to a connection-scoped id for non-UI senders.
        let user_messages = if let Some(messages) = user_messages_override {
            messages
        } else if delegation.is_none() && conversation_id_for_status.is_some() {
            let user_blocks = crate::acp::user_blocks_from_prompt(&blocks);
            if user_blocks.is_empty() {
                Vec::new()
            } else {
                // A client-supplied id in the parsers' turn-id namespace
                // (`turn-<digits>`, which every parser assigns) would collide
                // with a persisted transcript turn id and break id-keyed dedup
                // — a colliding id can suppress or hide a prompt. The id is
                // untrusted (the web/Tauri prompt API accepts it verbatim), so
                // reject that shape and fall back to a connection-scoped id;
                // legitimate UI senders use `optimistic-<uuid>`.
                let message_id = match client_message_id {
                    Some(id) if !is_reserved_turn_id(&id) => id,
                    _ => format!("user-{}-{}", conn_id, state_arc.read().await.event_seq),
                };
                vec![(message_id, user_blocks)]
            }
        } else {
            Vec::new()
        };

        // We hold `_prompt_guard` here, so call the lock-free inner helper —
        // re-entering `send_prompt` would try to acquire the same mutex and
        // deadlock. The helper reserves channel capacity FIRST and only then
        // sets the turn-in-flight gate, with no await before the infallible
        // `permit.send`; so a failure (channel closed / process exited) happens
        // at the reserve step, BEFORE the gate is set — there is nothing to roll
        // back. On that failure we still flip the row to `Cancelled` so the UI
        // doesn't strand on `in_progress`: no `TurnComplete` will ever arrive
        // for a prompt that never reached the agent, so without this the
        // lifecycle subscriber's PendingReview write also never fires and the
        // row would be stuck until a follow-up `send_prompt_linked` re-flipped it.
        let agent_blocks = prepared_agent_blocks.unwrap_or(blocks);
        if let Some(monitor) = upload_monitor.as_ref() {
            monitor
                .require_current()
                .await
                .map_err(AcpError::from_capability_error)?;
        }
        match self
            .send_prompt_inner(
                conn_id,
                agent_blocks,
                user_messages,
                image_context,
                accepted,
                upload_monitor.as_ref(),
            )
            .await
        {
            Ok(()) => {
                // The prompt reached the agent: surface it to the chat-channel
                // "user message" event feed. Notification-only — never gates the
                // send result.
                if let Some(text_preview) = user_prompt_preview {
                    emit_with_state(
                        &state_arc,
                        &emitter,
                        AcpEvent::UserPromptSent { text_preview },
                    )
                    .await;
                }
                Ok(conversation_id_for_status)
            }
            Err(send_err) => {
                if let Some(cid) = conversation_id_for_status {
                    match conversation_service::update_status(
                        &db.conn,
                        cid,
                        ConversationStatus::Cancelled,
                    )
                    .await
                    {
                        Ok(_) => {
                            emit_with_state(
                                &state_arc,
                                &emitter,
                                AcpEvent::ConversationStatusChanged {
                                    conversation_id: cid,
                                    status: ConversationStatus::Cancelled,
                                },
                            )
                            .await;
                        }
                        Err(rollback_err) => {
                            // Best-effort: original send error is the load-bearing
                            // signal; rollback failure is logged but not surfaced.
                            tracing::error!(
                                "[ACP][ERROR] failed to mark conversation {cid} cancelled \
                                 after send failure (original={send_err}): {rollback_err}"
                            );
                        }
                    }
                }
                Err(send_err)
            }
        }
    }

    pub(crate) async fn send_force_batch_prompt(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        items: &[crate::acp::AgentInputItem],
        folder_id: i32,
        conversation_id: i32,
    ) -> Result<(), AcpError> {
        if items.is_empty() {
            return Err(AcpError::protocol("force batch is empty"));
        }
        let blocks = items
            .iter()
            .flat_map(|item| item.payload.blocks.clone())
            .collect::<Vec<_>>();
        let user_messages = items
            .iter()
            .map(|item| {
                (
                    item.id.clone(),
                    crate::acp::user_blocks_from_prompt(&item.payload.blocks),
                )
            })
            .collect();
        let (accepted, acceptance) = tokio::sync::oneshot::channel();
        self.send_prompt_linked_with_user_messages(
            db,
            conn_id,
            blocks,
            Some(folder_id),
            Some(conversation_id),
            None,
            None,
            Some(user_messages),
            Some(accepted),
        )
        .await?;
        acceptance.await.map_err(|_| AcpError::ProcessExited)?;
        Ok(())
    }

    pub async fn set_mode(&self, conn_id: &str, mode_id: String) -> Result<(), AcpError> {
        let cmd_tx = {
            let connections = self.connections.lock().await;
            let conn = connections
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
            conn.cmd_tx.clone()
        };
        cmd_tx
            .send(ConnectionCommand::SetMode { mode_id })
            .await
            .map_err(|_| AcpError::ProcessExited)
    }

    pub async fn set_config_option(
        &self,
        conn_id: &str,
        config_id: String,
        value_id: String,
    ) -> Result<(), AcpError> {
        let cmd_tx = {
            let connections = self.connections.lock().await;
            let conn = connections
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
            conn.cmd_tx.clone()
        };
        cmd_tx
            .send(ConnectionCommand::SetConfigOption {
                config_id,
                value_id,
            })
            .await
            .map_err(|_| AcpError::ProcessExited)
    }

    pub async fn cancel(&self, db: &DatabaseConnection, conn_id: &str) -> Result<(), AcpError> {
        let (cmd_tx, state_arc, emitter) = {
            let connections = self.connections.lock().await;
            let conn = connections
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
            (
                conn.cmd_tx.clone(),
                conn.state.clone(),
                conn.emitter.clone(),
            )
        };
        cmd_tx
            .send(ConnectionCommand::Cancel)
            .await
            .map_err(|_| AcpError::ProcessExited)?;

        // Eagerly flip the row to `Cancelled` so the sidebar/tabs leave the
        // "running" state immediately. The agent typically replies with
        // `TurnComplete{cancelled}` which the lifecycle subscriber ignores,
        // and stays connected (so `handle_terminal_event` doesn't fire either)
        // — without this write the row would strand on `InProgress`.
        // CAS-guarded so we don't overwrite a `PendingReview`/`Completed`
        // status if the turn happened to end just before the user clicked.
        let conversation_id = state_arc.read().await.conversation_id;
        if let Some(cid) = conversation_id {
            match conversation_service::update_status_if(
                db,
                cid,
                ConversationStatus::InProgress,
                ConversationStatus::Cancelled,
            )
            .await
            {
                Ok(true) => {
                    emit_with_state(
                        &state_arc,
                        &emitter,
                        AcpEvent::ConversationStatusChanged {
                            conversation_id: cid,
                            status: ConversationStatus::Cancelled,
                        },
                    )
                    .await;
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::error!(
                        "[ACP][ERROR] failed to mark conversation {cid} cancelled \
                         on user cancel (conn={conn_id}): {e}"
                    );
                }
            }
        }

        Ok(())
    }

    pub async fn respond_permission(
        &self,
        conn_id: &str,
        request_id: &str,
        option_id: &str,
    ) -> Result<(), AcpError> {
        let cmd_tx = {
            let connections = self.connections.lock().await;
            let conn = connections
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
            conn.cmd_tx.clone()
        };
        cmd_tx
            .send(ConnectionCommand::RespondPermission {
                request_id: request_id.into(),
                option_id: option_id.into(),
            })
            .await
            .map_err(|_| AcpError::ProcessExited)
    }

    /// Fork the agent's session and persist the resulting two-row layout in
    /// one backend call: the current row gets re-pointed at S2 (the forked
    /// session) with a `[Fork]` title prefix, and a freshly-created sibling
    /// row preserves the pre-fork (S1) history at `PendingReview`. Frontend
    /// no longer touches `external_id` or fork-related row creation —
    /// the wire `ForkResultInfo` carries `sibling_conversation_id` for tab/UI
    /// reconciliation.
    pub async fn fork_session(
        &self,
        db: &AppDatabase,
        chat_channel_manager: &ChatChannelManager,
        conn_id: &str,
        link_conversation_id: Option<i32>,
        link_folder_id: Option<i32>,
    ) -> Result<ForkResultInfo, AcpError> {
        let (state_arc, cmd_tx, emitter) = {
            let connections = self.connections.lock().await;
            let conn = connections
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
            (
                conn.state.clone(),
                conn.cmd_tx.clone(),
                conn.emitter.clone(),
            )
        };

        // Serialize the fork against concurrent prompts on this connection via
        // the same per-connection `prompt_lock` that `send_prompt`/
        // `send_prompt_linked` hold. A fork re-points the live session, so a
        // prompt must never start a turn underneath it. The lock is held for the
        // WHOLE operation (gate check → enqueue → protocol round-trip →
        // persistence); because the LOCK (not a flag) provides the exclusion,
        // the fork never SETS `turn_in_flight`, so there is no flag a dropped
        // future could strand and no window where a prompt's side effects (row
        // create / InProgress) commit only to lose the gate to a fork and roll
        // back to `Cancelled`.
        let prompt_lock = self.clone_prompt_lock(conn_id).await?;
        let prompt_guard = prompt_lock.lock_owned().await;

        // A resumed historical session is row-linked only when its first prompt
        // is sent. Fork-send happens before that prompt, so adopt the caller's
        // existing row while holding the same lock used by prompt linkage.
        if state_arc.read().await.conversation_id.is_none() {
            if let (Some(conversation_id), Some(folder_id)) = (link_conversation_id, link_folder_id)
            {
                emit_with_state(
                    &state_arc,
                    &emitter,
                    AcpEvent::ConversationLinked {
                        conversation_id,
                        folder_id,
                        parent_conversation_id: None,
                        parent_tool_use_id: None,
                    },
                )
                .await;
            }
        }

        // Fork requires a linked conversation row — the sibling we're about
        // to create exists to preserve THIS row's pre-fork history. Without
        // a current row, fork would either orphan S1 or violate the
        // no-pre-prompt-row invariant.
        let conversation_id = state_arc.read().await.conversation_id.ok_or_else(|| {
            AcpError::protocol("fork_session requires a linked conversation row".to_string())
        })?;

        // Reject if a turn is already in flight. `prompt_lock` is FREE between a
        // prompt's enqueue and its `TurnComplete` (it is released the moment the
        // command is queued), so the lock alone can't catch a turn the loop is
        // mid-processing — only the gate can. We CHECK the gate (bouncing with
        // `TurnInProgress` so the caller re-queues) under the prompt lock, where
        // the loop is the only writer and the value can't flip to true
        // underneath us, but we never SET it: not setting the gate is precisely
        // why a dropped fork can't wedge the connection.
        if state_arc.read().await.turn_in_flight {
            return Err(AcpError::TurnInProgress);
        }

        // CANCELLATION SHIELD. Up to here the fork is side-effect-free: if THIS
        // future is dropped now (e.g. an HTTP client disconnecting mid-fork), the
        // `prompt_guard` drops and nothing happened. But the instant we enqueue
        // `ConnectionCommand::Fork`, the connection loop executes the agent
        // `session/fork` and re-points the live session to S2 REGARDLESS of
        // whether this caller survives — `handle_fork_or_exit` ignores a dead
        // reply channel and still attaches + emits `SessionStarted{S2}`. So the
        // DB persistence (sibling row preserving S1 + `[Fork]` title) must NOT be
        // tied to this future; otherwise a dropped caller would strand the live
        // session on S2 with the pre-fork S1 history orphaned and no sibling row.
        // We run enqueue → reply → persist → emit in a DETACHED task that OWNS
        // the `prompt_guard`: dropping this future no longer aborts the
        // persistence — it runs to completion and only then releases the lock.
        // We await the task's handle purely to hand the result back to a live
        // caller; the result is harmlessly discarded if the caller is gone.
        let db_conn = db.conn.clone();
        let chat_channel_manager = chat_channel_manager.clone_ref();
        let conn_id_for_task = conn_id.to_string();
        let handle = tokio::spawn(async move {
            // Holding the owned guard for the whole task is what shields the
            // persistence from caller cancellation.
            let _prompt_guard = prompt_guard;
            let outcome: Result<ForkResultInfo, AcpError> = async {
                // Protocol-only round trip — no DB writes inside the loop.
                let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                cmd_tx
                    .send(ConnectionCommand::Fork { reply: reply_tx })
                    .await
                    .map_err(|_| AcpError::ProcessExited)?;
                let protocol_result = reply_rx
                    .await
                    .map_err(|_| AcpError::protocol("Fork reply channel closed".to_string()))??;

                let forked_session_id = protocol_result.forked_session_id;
                let original_session_id = protocol_result.original_session_id;

                let sibling_id = Self::persist_fork_outcome(
                    &db_conn,
                    conversation_id,
                    forked_session_id.clone(),
                    original_session_id.clone(),
                )
                .await?;

                // Fork mutates the sidebar in two ways the rest of the system
                // never sees otherwise: the current row's title (`[Fork] …`) and
                // external_id (→ S2) changed, and a brand-new sibling row now
                // exists (external_id S1, PendingReview). The current row goes
                // through the shared title coordinator so its sidebar and chat
                // topic side effects cannot finish out of order with Agent,
                // parser, or manual title changes.
                let title_context = ConversationTitleContext {
                    conn: &db_conn,
                    emitter: &emitter,
                    chat_channel_manager: &chat_channel_manager,
                };
                conversation_title::notify_current(&title_context, conversation_id).await;
                crate::commands::conversations::emit_conversation_upsert(
                    &emitter, &db_conn, sibling_id,
                )
                .await;

                Ok(ForkResultInfo {
                    forked_session_id,
                    original_session_id,
                    sibling_conversation_id: sibling_id,
                })
            }
            .await;
            // Surface failures even when the caller is gone (the detached task's
            // Result would otherwise be dropped silently).
            if let Err(ref e) = outcome {
                tracing::error!(
                    "[ACP][ERROR] fork persistence failed (conn={conn_id_for_task}): {e}"
                );
            }
            outcome
        });

        match handle.await {
            Ok(result) => result,
            Err(join_err) => {
                tracing::error!(
                    "[ACP][ERROR] fork persistence task did not complete (conn={conn_id}): \
                     {join_err}"
                );
                Err(AcpError::protocol(format!(
                    "fork persistence task did not complete: {join_err}"
                )))
            }
        }
    }

    /// Persist the two-row fork layout in one transaction.
    ///
    /// SQLite transactions start deferred, so the opening statement must be a
    /// write. Claiming the live row first avoids promoting a stale read snapshot
    /// to a writer (`SQLITE_BUSY_SNAPSHOT`) and prevents deleted-row resurrection.
    async fn persist_fork_outcome(
        db_conn: &DatabaseConnection,
        conversation_id: i32,
        forked_session_id: String,
        original_session_id: String,
    ) -> Result<i32, AcpError> {
        use sea_orm::sea_query::Expr;
        use sea_orm::{ColumnTrait, QueryFilter};

        db_conn
            .transaction::<_, i32, sea_orm::DbErr>(|txn| {
                Box::pin(async move {
                    let now = chrono::Utc::now();
                    let claimed = conversation::Entity::update_many()
                        .col_expr(conversation::Column::UpdatedAt, Expr::value(now))
                        .filter(conversation::Column::Id.eq(conversation_id))
                        .filter(conversation::Column::DeletedAt.is_null())
                        .exec(txn)
                        .await?;
                    if claimed.rows_affected == 0 {
                        return Err(sea_orm::DbErr::Custom(format!(
                            "conversation {conversation_id} not found or already deleted"
                        )));
                    }

                    let current = conversation::Entity::find_by_id(conversation_id)
                        .one(txn)
                        .await?
                        .ok_or_else(|| {
                            sea_orm::DbErr::Custom(format!(
                                "conversation {conversation_id} not found"
                            ))
                        })?;

                    // Strip any `[Fork]` prefix tolerantly (matches the prior
                    // frontend regex `/^\[Fork]\s*/g` behaviour for both
                    // spaced and no-space variants). None title stays None.
                    let clean_title: Option<String> = current.title.as_ref().map(|t| {
                        t.strip_prefix("[Fork]")
                            .map(str::trim_start)
                            .unwrap_or(t.as_str())
                            .to_string()
                    });

                    let folder_id = current.folder_id;
                    let agent_type_str = current.agent_type.clone();
                    let git_branch = current.git_branch.clone();
                    let sibling_title_source = match &current.title_source {
                        conversation::ConversationTitleSource::Manual => {
                            conversation::ConversationTitleSource::UserFallback
                        }
                        source => source.clone(),
                    };
                    // The sibling keeps the original's sidebar routing (a forked
                    // chat conversation must stay in the Chat group). `Delegate`
                    // is unreachable here — children are never forked from the
                    // UI — but the invariant `delegate ⟺ parent_id set` wins
                    // over inheritance, so it degrades to `Regular`.
                    let sibling_kind = match current.kind {
                        ConversationKind::Delegate => ConversationKind::Regular,
                        ref kind => kind.clone(),
                    };
                    // UPDATE current row → S2. Writing external_id explicitly
                    // here closes the race against `refreshConversations()`
                    // after this fn returns; the lifecycle subscriber's later
                    // SessionStarted{S2} write is an idempotent no-op.
                    let mut active: conversation::ActiveModel = current.into();
                    if let Some(ref clean) = clean_title {
                        active.title = Set(Some(format!("[Fork] {clean}")));
                    }
                    active.external_id = Set(Some(forked_session_id));
                    active.updated_at = Set(now);
                    active.update(txn).await?;

                    // INSERT sibling row preserving pre-fork (S1) history.
                    // PendingReview because no live agent is attached to S1.
                    let sibling = conversation::ActiveModel {
                        id: NotSet,
                        folder_id: Set(folder_id),
                        title: Set(clean_title),
                        title_locked: Set(false),
                        title_source: Set(sibling_title_source),
                        title_summary_attempted: Set(true),
                        agent_type: Set(agent_type_str),
                        status: Set(ConversationStatus::PendingReview),
                        kind: Set(sibling_kind),
                        model: Set(None),
                        git_branch: Set(git_branch),
                        external_id: Set(Some(original_session_id)),
                        parent_id: Set(None),
                        parent_tool_use_id: Set(None),
                        delegation_call_id: Set(None),
                        message_count: Set(0),
                        created_at: Set(now),
                        updated_at: Set(now),
                        deleted_at: Set(None),
                        pinned_at: Set(None),
                    };
                    let inserted = sibling.insert(txn).await?;
                    Ok(inserted.id)
                })
            })
            .await
            .map_err(|e| AcpError::protocol(e.to_string()))
    }

    pub async fn disconnect(&self, conn_id: &str) -> Result<(), AcpError> {
        let connection = {
            let mut connections = self.connections.lock().await;
            connections.remove(conn_id)
        };
        if let Some(connection) = connection {
            tracing::info!(
                connection_id = conn_id,
                source = "explicit_request",
                "[ACP] disconnect requested"
            );
            connection.request_disconnect();
            Ok(())
        } else {
            tracing::warn!(
                connection_id = conn_id,
                "[ACP] disconnect ignored because connection was not found"
            );
            Err(AcpError::ConnectionNotFound(conn_id.into()))
        }
    }

    pub async fn disconnect_for_replacement(&self, conn_id: &str) -> Result<bool, AcpError> {
        let prompt_lock = {
            let connections = self.connections.lock().await;
            let Some(connection) = connections.get(conn_id) else {
                return Ok(true);
            };
            Arc::clone(&connection.prompt_lock)
        };
        let Ok(_prompt_guard) = prompt_lock.try_lock_owned() else {
            tracing::info!(
                connection_id = conn_id,
                "[ACP] target replacement blocked by busy prompt path"
            );
            return Ok(false);
        };
        let connection = {
            let mut connections = self.connections.lock().await;
            let Some(state) = connections
                .get(conn_id)
                .map(|connection| Arc::clone(&connection.state))
            else {
                return Ok(true);
            };
            let Ok(state) = state.try_read() else {
                return Ok(false);
            };
            if active_operation_reason(&state, chrono::Utc::now()).is_some() {
                tracing::info!(
                    connection_id = conn_id,
                    "[ACP] target replacement blocked by active operation"
                );
                return Ok(false);
            }
            connections.remove(conn_id)
        };
        if let Some(connection) = connection {
            tracing::info!(
                connection_id = conn_id,
                source = "target_replacement",
                "[ACP] disconnect requested"
            );
            connection.request_disconnect();
        }
        Ok(true)
    }

    pub(crate) async fn disconnect_if_reclaimable(
        &self,
        conn_id: &str,
        grace: Duration,
        allow_recent_activity: bool,
        source: &'static str,
    ) -> Result<bool, AcpError> {
        let prompt_lock = {
            let connections = self.connections.lock().await;
            let Some(connection) = connections.get(conn_id) else {
                return Ok(false);
            };
            Arc::clone(&connection.prompt_lock)
        };
        let Ok(_prompt_guard) = prompt_lock.try_lock_owned() else {
            tracing::info!(
                connection_id = conn_id,
                source,
                "[ACP] reclaim skipped: prompt path busy"
            );
            return Ok(false);
        };
        let cmd_tx = {
            let mut connections = self.connections.lock().await;
            let Some(state) = connections.get(conn_id).map(|conn| Arc::clone(&conn.state)) else {
                return Ok(false);
            };
            let Ok(state) = state.try_read() else {
                tracing::info!(
                    connection_id = conn_id,
                    source,
                    "[ACP] reclaim skipped: state busy"
                );
                return Ok(false);
            };
            if let Some(reason) = crate::acp::resource_governor::reclaim_block_reason(
                &state,
                chrono::Utc::now(),
                grace,
                allow_recent_activity,
            ) {
                tracing::info!(
                    connection_id = conn_id,
                    source,
                    reason,
                    "[ACP] reclaim skipped: connection protected"
                );
                return Ok(false);
            }
            connections.remove(conn_id).map(|conn| conn.cmd_tx)
        };
        let Some(cmd_tx) = cmd_tx else {
            return Ok(false);
        };
        tracing::info!(
            connection_id = conn_id,
            source,
            "[ACP] reclaiming idle connection"
        );
        let _ = cmd_tx.send(ConnectionCommand::Disconnect).await;
        Ok(true)
    }

    /// Probe an agent for the modes / config_options it advertises on a fresh
    /// session, then immediately disconnect. The probe runs with
    /// `EventEmitter::Noop` so no event reaches the desktop webview, the
    /// global `WebEventBroadcaster`, or the `InternalEventBus` — the events
    /// land only in this probe connection's own (unsubscribed) per-connection
    /// stream and in its `SessionState` (which is the read source here).
    ///
    /// Used by the delegation-settings UI to enumerate the options the user
    /// can override, with the guarantee that what the UI shows is exactly
    /// what the built-in MCP delegation path passes to `session/set_config_option`
    /// when a delegation actually fires.
    ///
    /// Returns `Ok(snapshot)` even when the agent advertises no options
    /// (empty `config_options`, `None` modes) — that's a valid outcome the
    /// UI can render as "this agent has nothing to configure."
    pub async fn probe_agent_options(
        &self,
        agent_type: AgentType,
        working_dir: Option<String>,
        runtime_env: BTreeMap<String, String>,
    ) -> Result<AgentOptionsSnapshot, AcpError> {
        // Owner window label is informational only (used for
        // disconnect_by_owner_window), but worth being explicit so a probe
        // connection that somehow leaks past the disconnect below is easy to
        // identify in logs / debug snapshots.
        let owner_window = "delegation-probe".to_string();
        // Serialize concurrent probes for the same agent_type. Rapid tab
        // switching in the settings UI would otherwise fan out one real
        // CLI process per click — each one running up to 60s. The mutex
        // bounds this to one in-flight probe per agent type; different
        // agent_types still probe in parallel.
        //
        // The outer `probe_locks` guard MUST be dropped BEFORE the
        // `.lock_owned().await` on the per-agent mutex. If we held it
        // across the await, a probe queued behind another for the SAME
        // agent_type would keep the outer map locked, blocking probes
        // for every OTHER agent_type too — silently turning the
        // per-agent serialization into a global one.
        let per_agent_lock: Arc<tokio::sync::Mutex<()>> = {
            let mut locks = self.probe_locks.lock().await;
            locks
                .entry(agent_type)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _probe_guard = per_agent_lock.lock_owned().await;
        let conn_id = self
            .spawn_agent_with_origin(
                agent_type,
                working_dir,
                None, // brand-new session — no resume
                runtime_env,
                owner_window,
                EventEmitter::Noop,
                None,
                BTreeMap::new(),
                crate::user_memory::UserMemoryOrigin::Probe,
            )
            .await?;

        // Hold an `Arc<RwLock<SessionState>>` alongside the manager's own
        // entry so the state survives even if the connection task cleans
        // up its map slot mid-handshake. Without this, an agent that
        // errors during Initialize would trigger cleanup before the
        // probe's poll loop sees the `AcpEvent::Error` payload, and
        // `wait_for_session_options` would surface the unhelpful
        // `ConnectionNotFound` instead of the agent's own error text.
        let state_arc = self.get_state(&conn_id).await;

        // Generous timeout because some agents (Gemini in particular) take
        // 8-10s just to answer Initialize before session/new can even start;
        // a tight cap here would consistently return an empty snapshot and
        // make the settings UI claim those agents have nothing to configure.
        // Matches the per-step Initialize timeout in `connection.rs`.
        let probe_timeout = Duration::from_secs(60);
        let raw_snapshot = self.wait_for_session_options(&conn_id, probe_timeout).await;

        // If the wait errored, prefer the agent's own captured error
        // message over the generic ProbeTimedOut / ConnectionNotFound —
        // an agent that died on Initialize already explained why.
        let snapshot = match raw_snapshot {
            Ok(s) => Ok(s),
            Err(wait_err) => {
                let captured = if let Some(state) = state_arc.as_ref() {
                    state.read().await.last_error.clone()
                } else {
                    None
                };
                Err(match captured {
                    Some(err) => AcpError::protocol(err.message),
                    None => wait_err,
                })
            }
        };

        // Always disconnect — including on Err — so a failed probe doesn't
        // leak an agent process. Ignore disconnect errors (best-effort
        // cleanup; the agent will exit when its stdio is dropped anyway).
        let _ = self.disconnect(&conn_id).await;
        snapshot
    }

    /// Poll a connection's `SessionState` until the agent signals it has
    /// finished publishing its initial selectors (`SelectorsReady`), then
    /// give a small grace window for any tightly-following follow-up updates
    /// before snapshotting. Waiting on `selectors_ready` — not just
    /// `config_options.is_some()` — matters because some agents emit an
    /// empty `SessionConfigOptions` first and then push the real options
    /// in a subsequent update; returning on the first `Some(vec![])` would
    /// race ahead of those updates and report the agent as having nothing
    /// to configure.
    ///
    /// The `SessionConfigOptions` / `SelectorsReady` ACP events populate
    /// `SessionState` via `apply_event` regardless of which `EventEmitter`
    /// variant the connection uses — that's why the probe can rely on
    /// `Noop` and still observe the values here.
    ///
    /// Returns `AcpError::ProbeTimedOut` when the timeout elapses without
    /// `selectors_ready` ever flipping to `true`. Distinguishing that case
    /// from a clean "ready with no options" snapshot lets the UI tell the
    /// user "the agent never published its options — retry" instead of
    /// silently claiming the agent has nothing to configure.
    async fn wait_for_session_options(
        &self,
        conn_id: &str,
        timeout: Duration,
    ) -> Result<AgentOptionsSnapshot, AcpError> {
        let start = std::time::Instant::now();
        // Grace window between `selectors_ready` flipping true and the
        // snapshot we return. Lets a straggling `ConfigOptionUpdate` that
        // an agent emits in the same tick land before we read.
        let grace_period = Duration::from_millis(500);

        // Step 1: get the state Arc (briefly) so we can use the per-state
        // RwLock for the Notify pattern — without holding the connections map
        // lock across any await.
        let state_arc = {
            let conns = self.connections.lock().await;
            let conn = conns
                .get(conn_id)
                .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
            conn.state.clone()
        };

        // Step 2: wait until `selectors_ready` using the proper Notify pattern.
        // We create `notified()` while holding the state READ lock so no wakeup
        // is lost (mirrors `wait_for_launch_finalization`): `notify_waiters()`
        // is only called from `emit_with_state` which holds the WRITE lock, so
        // the window between our condition check and our `notified()` creation
        // is closed by the read lock.
        loop {
            let notified = {
                let snap = state_arc.read().await;
                if snap.selectors_ready {
                    break; // already ready — proceed to grace period
                }
                snap.selectors_ready_notify.clone().notified_owned()
            };
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(AcpError::ProbeTimedOut);
            }
            match tokio::time::timeout(remaining, notified).await {
                Ok(()) => { /* loop to re-check selectors_ready */ }
                Err(_elapsed) => return Err(AcpError::ProbeTimedOut),
            }
        }

        // Step 3: Grace window — lets any tightly-following ConfigOptionUpdate
        // land before we snapshot. Apply remaining timeout so the overall
        // deadline is still honoured.
        let grace_wait = grace_period.min(timeout.saturating_sub(start.elapsed()));
        tokio::time::sleep(grace_wait).await;

        // Step 4: Final snapshot. Re-verify the connection is still alive
        // (the probe calls disconnect() after us, but another path could have
        // already removed it).
        let snap = state_arc.read().await;
        Ok(AgentOptionsSnapshot {
            modes: snap.modes.clone(),
            config_options: snap.config_options.clone().unwrap_or_default(),
            available_commands: snap.available_commands.clone(),
        })
    }

    pub async fn disconnect_by_owner_window(&self, owner_window_label: &str) -> usize {
        let removed = {
            let mut connections = self.connections.lock().await;
            let ids: Vec<String> = connections
                .iter()
                .filter_map(|(id, conn)| {
                    if conn.owner_window_label == owner_window_label {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let mut removed = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(conn) = connections.remove(&id) {
                    removed.push(conn);
                }
            }
            removed
        };

        let disconnected = removed.len();
        for connection in removed {
            connection.request_disconnect();
        }
        tracing::info!(
            owner_window = owner_window_label,
            disconnected,
            "[ACP] disconnected connections by owner window"
        );
        disconnected
    }

    pub async fn disconnect_all(&self) -> usize {
        self.disconnect_all_checked().await.disconnected
    }

    pub async fn disconnect_all_checked(&self) -> ConnectionShutdownReport {
        let _shutdown_gate = self.connection_tasks.begin_shutdown().await;
        // Keep entries in the manager until their background task's cleanup
        // guard runs. If an outer shutdown budget cancels this future, a later
        // forced pass can still see and cancel the unfinished connections.
        let pending: Vec<(String, Arc<AtomicBool>)> = {
            let connections = self.connections.lock().await;
            for connection in connections.values() {
                connection.request_disconnect();
            }
            connections
                .values()
                .map(|connection| {
                    (
                        connection.id.clone(),
                        Arc::clone(&connection.cleanup_completed),
                    )
                })
                .collect()
        };
        let disconnected = pending.len();
        let task_ids = self.connection_tasks.ids().await;
        {
            let mut cleanup_pending = self.shutdown_cleanup_pending.lock().await;
            cleanup_pending.extend(task_ids);
            cleanup_pending.extend(pending.iter().map(|(id, _)| id.clone()));
        }
        let cleanup_waits = pending
            .iter()
            .cloned()
            .map(|(connection_id, cleanup_completed)| {
                wait_for_connection_cleanup(connection_id, cleanup_completed)
            })
            .collect::<Vec<_>>();
        join_all(cleanup_waits).await;
        let tasks_completed = self.connection_tasks.abort_and_reap_all().await;
        let residual_connections = {
            let mut connections = self.connections.lock().await;
            let count = connections.len();
            connections.clear();
            count
        };
        let cleanup_completed = pending
            .iter()
            .filter(|(_, completed)| completed.load(Ordering::Acquire))
            .count();
        let authority_ids = self
            .shutdown_cleanup_pending
            .lock()
            .await
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let builtin_mcp = self.builtin_mcp_snapshot();
        let delegation = self.delegation_snapshot();
        let authority_results = join_all(authority_ids.into_iter().map(|connection_id| {
            cleanup_connection_authorities(connection_id, builtin_mcp.clone(), delegation.clone())
        }))
        .await;
        {
            let mut cleanup_pending = self.shutdown_cleanup_pending.lock().await;
            for (connection_id, authority_completed) in authority_results {
                if authority_completed {
                    cleanup_pending.remove(&connection_id);
                }
            }
        }
        let authority_pending = self.shutdown_cleanup_pending.lock().await.len();
        let host_shutdown = self.runtime_hosts.shutdown_all().await;
        let completed = tasks_completed
            && cleanup_completed == disconnected
            && authority_pending == 0
            && host_shutdown.completed;
        tracing::info!(
            disconnected,
            stopped_hosts = host_shutdown.stopped_hosts,
            startup_tasks_reaped = host_shutdown.startup_tasks_reaped,
            host_shutdown_completed = host_shutdown.completed,
            cleanup_completed,
            cleanup_timed_out = disconnected.saturating_sub(cleanup_completed),
            residual_connections,
            tasks_completed,
            authority_pending,
            completed,
            "[ACP] disconnected all connections"
        );
        ConnectionShutdownReport {
            disconnected,
            completed,
        }
    }

    pub async fn list_connections(&self) -> Vec<ConnectionInfo> {
        let connections = self.connections.lock().await;
        connections.values().map(|c| c.info()).collect()
    }

    pub(crate) async fn runtime_session_snapshots(
        &self,
    ) -> Vec<crate::acp::resource_governor::RuntimeSessionSnapshot> {
        let candidates = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .map(|(id, connection)| (id.clone(), Arc::clone(&connection.state)))
                .collect::<Vec<_>>()
        };
        let now = chrono::Utc::now();
        let mut snapshots = Vec::with_capacity(candidates.len());
        for (connection_id, state) in candidates {
            let Ok(state) = state.try_read() else {
                continue;
            };
            let protection_reason = crate::acp::resource_governor::reclaim_block_reason(
                &state,
                now,
                crate::acp::resource_governor::completion_grace(),
                false,
            );
            snapshots.push(crate::acp::resource_governor::RuntimeSessionSnapshot {
                connection_id,
                conversation_id: state.conversation_id,
                agent_type: state.agent_type,
                status: state.status.clone(),
                launcher_pid: state.agent_pid,
                last_activity_at: state.last_activity_at.max(state.last_agent_event_at),
                recoverable: state.external_id.is_some()
                    && state.recoverable_session
                    && !state.recovery_failed,
                protection_reason,
            });
        }
        snapshots
    }

    pub(crate) async fn image_analysis_state_for_connection(
        &self,
        connection_id: &str,
    ) -> Option<(Option<String>, AgentType, bool)> {
        let state = {
            let connections = self.connections.lock().await;
            connections.get(connection_id)?.state.clone()
        };
        let session = state.read().await;
        let model = session.current_model.clone();
        let accepts_images = session
            .prompt_capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.image);
        Some((model, session.agent_type, accepts_images))
    }

    /// 是否存在会话存活的 Agent 连接（未断开且非错误状态）。
    ///
    /// IR-005：受管组件版本切换/激活前用它判断是否应延迟——会话存活时不
    /// 切换 active pointer，而是写入 pending activations，由下一次新建
    /// Agent 连接在 spawn 前消费激活。判定口径与旧 `install_tool_core` 的
    /// "Disconnect active Agents" 门禁一致（任何未断开连接都视为活跃）。
    pub async fn has_live_agent_sessions(&self) -> bool {
        self.list_connections().await.iter().any(|item| {
            !matches!(
                item.status,
                ConnectionStatus::Disconnected | ConnectionStatus::Error
            )
        })
    }

    pub async fn has_active_agent_operations(&self) -> bool {
        let states = {
            let connections = self.connections.lock().await;
            connections
                .values()
                .map(|connection| Arc::clone(&connection.state))
                .collect::<Vec<_>>()
        };
        let now = chrono::Utc::now();
        for state in states {
            let Ok(state) = state.try_read() else {
                return true;
            };
            if active_operation_reason(&state, now).is_some() {
                return true;
            }
        }
        false
    }

    pub async fn has_live_agent_session(&self, agent_type: AgentType) -> bool {
        self.list_connections().await.iter().any(|item| {
            item.agent_type == agent_type
                && !matches!(
                    item.status,
                    ConnectionStatus::Disconnected | ConnectionStatus::Error
                )
        })
    }

    /// Count live connections whose launch-time effective user context differs
    /// from current settings/files. Existing sessions deliberately keep their
    /// snapshot; the settings UI uses this count to request a new conversation.
    pub async fn count_stale_user_memory(
        &self,
        service: &crate::user_memory::UserMemoryService,
    ) -> usize {
        let health = self.builtin_mcp_health_snapshot();
        self.count_stale_user_memory_with_health(service, health)
            .await
    }

    pub async fn count_stale_user_memory_with_health(
        &self,
        service: &crate::user_memory::UserMemoryService,
        health: crate::user_memory::CompanionHealthSnapshot,
    ) -> usize {
        let states = {
            let connections = self.connections.lock().await;
            connections
                .values()
                .map(|connection| connection.state.clone())
                .collect::<Vec<_>>()
        };
        let mut stale = 0;
        for state in states {
            let launch = user_memory_staleness_snapshot(&state).await;
            if !launch.eligible() {
                continue;
            }
            if !launch.context_known {
                stale += 1;
                continue;
            }
            if let Ok(mut current) = service
                .launch_context_for(launch.agent_type, launch.origin)
                .await
            {
                current.finalize_runtime(crate::user_memory::UserMemoryRuntimeEnvironment {
                    companion_health: health.clone(),
                    host_bridge_available: self.user_memory_host_bridge_available(),
                });
                if current.effective_fingerprint != launch.fingerprint {
                    stale += 1;
                }
            }
        }
        stale
    }

    /// Raw per-connection rows for the pet panel's active-session list.
    /// "Active" = the connection is currently `Prompting`, awaiting a
    /// permission, or in an `Error` state — the sessions a user would want to
    /// see or act on from the floating pet. Idle `Connected` sessions are
    /// excluded to keep the list focused (mirrors the Codex pet "signal"
    /// model).
    ///
    /// `title` is left empty here: this layer has no DB handle. The command
    /// layer (`pet_list_active_sessions_core`) fills it from the conversation
    /// row. Connections without both a bound `conversation_id` and `folder_id`
    /// are skipped — the panel needs both to render a row and to navigate to
    /// it. Lock discipline mirrors `find_connection_by_conversation_id`: hold
    /// the connections mutex while taking each per-session read lock (the
    /// reads are microseconds and released each iteration).
    pub async fn list_active_sessions(&self) -> Vec<crate::models::pet::PetSessionEntry> {
        // Collect (id, agent_type, state_arc) without holding connections lock
        // across state reads.
        let candidates: Vec<(
            String,
            AgentType,
            Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
        )> = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .map(|(id, conn)| (id.clone(), conn.agent_type, conn.state.clone()))
                .collect()
        };
        let mut out = Vec::new();
        for (id, agent_type, state_arc) in candidates {
            let state = state_arc.read().await;
            let (Some(conversation_id), Some(folder_id)) = (state.conversation_id, state.folder_id)
            else {
                continue;
            };
            let pending = state
                .pending_permission
                .as_ref()
                .map(crate::models::pet::PetPermissionSummary::from);
            let is_active = pending.is_some()
                || matches!(
                    state.status,
                    ConnectionStatus::Prompting | ConnectionStatus::Error
                );
            if !is_active {
                continue;
            }
            out.push(crate::models::pet::PetSessionEntry {
                connection_id: id,
                conversation_id,
                folder_id,
                agent_type,
                title: String::new(),
                status: state.status.clone(),
                pending,
            });
        }
        out
    }

    /// Clone the `Arc<RwLock<SessionState>>` for a given connection id so the
    /// caller can read/write state without holding the connections mutex.
    /// Returns `None` if no such connection is registered.
    pub async fn get_state(
        &self,
        conn_id: &str,
    ) -> Option<std::sync::Arc<tokio::sync::RwLock<crate::acp::SessionState>>> {
        let connections = self.connections.lock().await;
        connections.get(conn_id).map(|conn| conn.state.clone())
    }

    /// Like `get_state`, but also clones the connection's `EventEmitter`.
    /// Used by the lifecycle subscriber when it needs to both update the
    /// per-session state and re-broadcast a derived event (e.g. emitting
    /// `ConversationStatusChanged` after writing the row's status).
    /// One short lock on the connections map; both pieces are cheap to clone.
    pub async fn get_state_and_emitter(
        &self,
        conn_id: &str,
    ) -> Option<(
        std::sync::Arc<tokio::sync::RwLock<crate::acp::SessionState>>,
        EventEmitter,
    )> {
        let connections = self.connections.lock().await;
        connections
            .get(conn_id)
            .map(|conn| (conn.state.clone(), conn.emitter.clone()))
    }

    /// Append a live-feedback note to a connection's session and broadcast it.
    ///
    /// Validation: the text is trimmed and rejected when empty
    /// ([`AcpError::InvalidFeedback`]) or longer than [`MAX_FEEDBACK_CHARS`] —
    /// the full text rides in the broadcast event, the snapshot, and the MCP
    /// response, so a sanity bound keeps one pathological note from bloating
    /// them. (There is deliberately no per-turn COUNT cap: the set is cleared
    /// every turn, so its size scales with human typing, not unboundedly.)
    ///
    /// Rejected with [`AcpError::NoActiveTurn`] unless a turn is in flight —
    /// feedback is mid-turn steering, pulled by the agent via the
    /// `check_user_feedback` MCP tool; with no active turn there is nothing to
    /// steer and the note would strand (the frontend falls back to an ordinary
    /// prompt). The append rides `emit_with_state` so `SessionState.feedback`,
    /// the ring buffer, and every attached client stay in lockstep.
    pub async fn submit_feedback(
        &self,
        conn_id: &str,
        text: String,
    ) -> Result<FeedbackItem, AcpError> {
        self.submit_feedback_with_id(conn_id, uuid::Uuid::new_v4().to_string(), text)
            .await
    }

    pub async fn submit_feedback_with_id(
        &self,
        conn_id: &str,
        id: String,
        text: String,
    ) -> Result<FeedbackItem, AcpError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(AcpError::InvalidFeedback("empty note".into()));
        }
        if trimmed.chars().count() > MAX_FEEDBACK_CHARS {
            return Err(AcpError::InvalidFeedback(format!(
                "note exceeds {MAX_FEEDBACK_CHARS} characters"
            )));
        }
        let text = trimmed.to_string();
        let (state, emitter) = self
            .get_state_and_emitter(conn_id)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.to_string()))?;
        // Per-connection capability gate: reject if THIS agent never got the
        // `check_user_feedback` tool (e.g. its session started before the feature
        // was enabled) — the note could never be read. `feedback_tool_available`
        // is fixed at launch, so a plain read is race-free.
        if !state.read().await.feedback_tool_available {
            return Err(AcpError::FeedbackDisabled);
        }
        let item = FeedbackItem::new_pending(id, text, chrono::Utc::now());
        // Gate on `turn_in_flight` and append in ONE critical section (via the
        // gated emit): a `TurnComplete` (flips the flag) or `UserMessage`
        // (clears `feedback`) can't slip between the gate and the append+seq, so
        // a note is never stranded on a finished turn nor re-added to a new one.
        let applied = emit_with_state_gated(
            &state,
            &emitter,
            AcpEvent::FeedbackSubmitted { item: item.clone() },
            |s| s.turn_in_flight,
        )
        .await;
        if !applied {
            return Err(AcpError::NoActiveTurn);
        }
        Ok(item)
    }

    /// Read the pending feedback for a connection WITHOUT marking it delivered.
    /// Returns an immediate snapshot. Read-only — backs the READ half of the
    /// `check_user_feedback` round-trip so the listener can commit delivery only
    /// after the response is actually written (a dropped / failed write leaves
    /// the notes pending for the agent's next check).
    pub async fn read_pending_feedback(&self, conn_id: &str) -> Vec<PendingFeedback> {
        let Some(state) = self.get_state(conn_id).await else {
            return Vec::new();
        };
        let pending: Vec<PendingFeedback> = {
            let s = state.read().await;
            s.feedback
                .iter()
                .filter(|f| f.status == FeedbackStatus::Pending)
                .map(|f| PendingFeedback {
                    id: f.id.clone(),
                    text: f.text.clone(),
                    created_at: f.created_at,
                })
                .collect()
        };
        bounded_feedback_batch(pending, MAX_FEEDBACK_RESPONSE_BYTES)
    }

    /// Mark the named notes `Delivered` and broadcast the consumption. Called by
    /// the listener ONLY after the `check_user_feedback` result was relayed in
    /// the authenticated MCP response, so a dropped response leaves notes pending and
    /// the agent's next check re-delivers them (at-least-once).
    ///
    /// Delivery is committed only after the HTTP response body has been relayed.
    /// A mid-wait cancellation leaves the note pending; a later retry is safe.
    ///
    /// The mark happens under a single write lock; only notes still `Pending`
    /// flip (idempotent — a repeated commit, or a note already consumed by a
    /// racing call, is skipped) and only the ids actually flipped are emitted,
    /// so a double-commit can't double-broadcast.
    pub async fn commit_feedback_delivered(&self, conn_id: &str, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        let Some((state, emitter)) = self.get_state_and_emitter(conn_id).await else {
            return;
        };
        let ids = crate::acp::agent_input_lifecycle::filter_feedback_commit_ids(
            self, &state, &emitter, ids,
        )
        .await;
        if ids.is_empty() {
            return;
        }
        let id_set: std::collections::HashSet<&String> = ids.iter().collect();
        let delivered_at = chrono::Utc::now();
        let marked: Vec<String> = {
            let mut s = state.write().await;
            let mut marked = Vec::new();
            for f in s.feedback.iter_mut() {
                if f.status == FeedbackStatus::Pending && id_set.contains(&f.id) {
                    f.status = FeedbackStatus::Delivered;
                    f.delivered_at = Some(delivered_at);
                    marked.push(f.id.clone());
                }
            }
            marked
        };
        if !marked.is_empty() {
            emit_with_state(
                &state,
                &emitter,
                AcpEvent::FeedbackConsumed {
                    ids: marked,
                    delivered_at,
                },
            )
            .await;
        }
    }

    /// Register a blocking `ask_user_question` on a connection: park a one-shot
    /// in `pending_questions` keyed by a fresh `question_id`, broadcast the
    /// `QuestionRequest` (so every attached client renders the interactive card
    /// and a mid-turn attach recovers it from the snapshot), and hand the
    /// receiver back to the listener to await. `None` when the connection is
    /// gone (nothing to ask) OR when this connection already has a pending ask
    /// — see below.
    ///
    /// One pending ask per connection: `SessionState.pending_question` and the
    /// frontend card are single slots, so a second concurrent ask would
    /// overwrite the first's card/snapshot and orphan the first (still-parked)
    /// tool call with no way to answer it. A single agent is blocked in its
    /// `ask_user_question` call and cannot issue a second, so this only guards a
    /// parallel / misbehaving MCP client; the refused second call resolves as
    /// `declined` (the listener's None path) so its agent proceeds with its own
    /// judgment instead of hanging. The check + insert are atomic under the
    /// registry lock.
    pub async fn register_question(
        &self,
        conn_id: &str,
        questions: Vec<QuestionSpec>,
    ) -> Option<RegisteredQuestion> {
        // Defense-in-depth: the MCP gateway validates first, but the broker also
        // refuses to broadcast malformed or oversized specs
        // (None → the listener declines the ask, as for any other None path).
        if crate::acp::question::validate_specs(&questions).is_err() {
            return None;
        }
        let (state, emitter) = self.get_state_and_emitter(conn_id).await?;
        let question_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut reg = self.pending_questions.lock().await;
            if reg.values().any(|e| e.parent_connection_id == conn_id) {
                return None;
            }
            reg.insert(
                question_id.clone(),
                PendingQuestionEntry {
                    parent_connection_id: conn_id.to_string(),
                    questions: questions.clone(),
                    sender: tx,
                },
            );
        }
        // Ungated emit: the agent is blocked in the tool call, so the card must
        // show regardless of any turn-flag timing.
        emit_with_state(
            &state,
            &emitter,
            AcpEvent::QuestionRequest {
                question_id: question_id.clone(),
                questions,
            },
        )
        .await;
        // Teardown event-ordering race: `cancel_questions_by_parent` may have
        // drained this entry between the insert above and the emit just now. The
        // QuestionRequest we broadcast would then have no waiter, and the sweep's
        // QuestionResolved may have raced ahead of it — leaving a card up with no
        // live backend waiter. Emit a compensating QuestionResolved (ordered after
        // our QuestionRequest) and decline. (The listener's post-register token
        // re-check covers the complementary case: a register that lands entirely
        // after the sweep, which this presence check would not catch.)
        if self
            .compensate_if_question_drained(&question_id, &state, &emitter)
            .await
        {
            return None;
        }
        Some(RegisteredQuestion {
            question_id,
            answer_rx: rx,
        })
    }

    /// Returns `true` — after emitting a clearing `QuestionResolved` — when
    /// `question_id` is no longer pending, i.e. a teardown sweep drained it in the
    /// window after its `QuestionRequest` was broadcast. The compensating event is
    /// ordered after the request so no client keeps a card with no live backend
    /// waiter. Returns `false` (no emit) while the entry is still parked.
    async fn compensate_if_question_drained(
        &self,
        question_id: &str,
        state: &std::sync::Arc<tokio::sync::RwLock<crate::acp::SessionState>>,
        emitter: &EventEmitter,
    ) -> bool {
        if self
            .pending_questions
            .lock()
            .await
            .contains_key(question_id)
        {
            return false;
        }
        emit_with_state(
            state,
            emitter,
            AcpEvent::QuestionResolved {
                question_id: question_id.to_string(),
            },
        )
        .await;
        true
    }

    /// Resolve a pending `ask_user_question` with the user's submission (from any
    /// client). Removes the one-shot atomically (first answer wins; a duplicate /
    /// already-resolved id is an idempotent no-op), sends the self-describing
    /// outcome to the blocked listener, and broadcasts `QuestionResolved` so the
    /// card clears on every client. Routing uses the entry's stored parent
    /// connection (the `question_id` is the authoritative key), so a stale
    /// `conn_id` from the caller can't misroute.
    pub async fn answer_question(
        &self,
        conn_id: &str,
        question_id: &str,
        answer: QuestionAnswer,
    ) -> Result<(), AcpError> {
        let _ = conn_id;
        let entry = self.pending_questions.lock().await.remove(question_id);
        let Some(entry) = entry else {
            // Already answered / canceled / gone elsewhere — idempotent success.
            return Ok(());
        };
        let outcome = build_outcome(&entry.questions, &answer);
        // Ignore a dropped receiver: the listener may have abandoned the wait
        // (peer-close) at the same instant; the resolved-event below still clears
        // the card.
        let _ = entry.sender.send(outcome);
        if let Some((state, emitter)) = self
            .get_state_and_emitter(&entry.parent_connection_id)
            .await
        {
            emit_with_state(
                &state,
                &emitter,
                AcpEvent::QuestionResolved {
                    question_id: question_id.to_string(),
                },
            )
            .await;
        }
        Ok(())
    }

    /// Cancel a pending `ask_user_question` because its MCP call was
    /// canceled (peer-close) or the connection is tearing down. Removes the
    /// one-shot (dropping the sender unblocks the listener with a declined
    /// outcome) and broadcasts `QuestionResolved` so the card clears. No-op if
    /// the question was already answered / gone.
    pub async fn cancel_question(&self, conn_id: &str, question_id: &str) {
        let _ = conn_id;
        let removed = self.pending_questions.lock().await.remove(question_id);
        let Some(entry) = removed else {
            return;
        };
        if let Some((state, emitter)) = self
            .get_state_and_emitter(&entry.parent_connection_id)
            .await
        {
            emit_with_state(
                &state,
                &emitter,
                AcpEvent::QuestionResolved {
                    question_id: question_id.to_string(),
                },
            )
            .await;
        }
    }

    /// Cancel every pending `ask_user_question` parked on a connection that is
    /// tearing down. The `run_connection` cleanup guard calls this (alongside
    /// the delegation `DelegationBroker::cancel_by_parent` cascade) so question
    /// entries — and the listener tasks parked on them — are reclaimed
    /// synchronously on disconnect instead of lingering until transport cleanup.
    /// Dropping each entry's sender unblocks its
    /// listener with a declined outcome; the `QuestionResolved` broadcast clears
    /// the card on every client. No-op when nothing is pending for this parent.
    pub async fn cancel_questions_by_parent(&self, conn_id: &str) {
        // Remove every entry for this parent under the lock (dropping their
        // senders unblocks the parked listeners), then emit outside the lock —
        // the registry mutex is never held across an await.
        let drained: Vec<String> = {
            let mut reg = self.pending_questions.lock().await;
            let ids: Vec<String> = reg
                .iter()
                .filter(|(_, e)| e.parent_connection_id == conn_id)
                .map(|(id, _)| id.clone())
                .collect();
            for id in &ids {
                reg.remove(id);
            }
            ids
        };
        if drained.is_empty() {
            return;
        }
        // Best-effort card clear: depending on the teardown path the connection
        // may already be out of the map (`disconnect` removes it before the
        // run_connection cleanup guard fires this sweep), so tolerate `None` — the
        // core removal above already ran and the frontend clears on disconnect.
        if let Some((state, emitter)) = self.get_state_and_emitter(conn_id).await {
            for question_id in drained {
                emit_with_state(&state, &emitter, AcpEvent::QuestionResolved { question_id }).await;
            }
        }
    }

    /// Resolve a conversation_id to its currently-active connection id, if any.
    /// Used by the by-conversation snapshot endpoint and the LifecycleSubscriber.
    pub async fn find_connection_by_conversation_id(&self, conversation_id: i32) -> Option<String> {
        // Collect state arcs without holding the connections lock across awaits.
        let candidates: Vec<(
            String,
            Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
        )> = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .map(|(id, conn)| (id.clone(), conn.state.clone()))
                .collect()
        };
        for (id, state_arc) in candidates {
            let state = state_arc.read().await;
            if state.conversation_id == Some(conversation_id) {
                return Some(id);
            }
        }
        None
    }

    /// The in-flight user prompt for `conversation_id` and the instant its turn
    /// started, if a turn is currently running on its live connection. `Some`
    /// exactly between `UserMessage` and `TurnComplete` (see
    /// `SessionState.pending_user_message` / `pending_user_message_started_at`);
    /// `None` when no connection is bound to the conversation or no turn is in
    /// flight.
    ///
    /// Used by the detail endpoint to stamp the persisted in-flight user turn
    /// with the broadcast `message_id`, so a cross-client viewer's synthesized
    /// turn (keyed by that same id) dedups against it instead of rendering a
    /// second copy. The start instant lets the matcher tell the in-flight prompt
    /// apart from a prior identical one. One lock pass over the connections map,
    /// mirroring `find_connection_by_conversation_id`.
    pub async fn pending_user_message_for_conversation(
        &self,
        conversation_id: i32,
    ) -> Option<(
        crate::acp::session_state::PendingUserMessage,
        Option<chrono::DateTime<chrono::Utc>>,
    )> {
        let candidates: Vec<Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>> = {
            let connections = self.connections.lock().await;
            connections.values().map(|c| c.state.clone()).collect()
        };
        for state_arc in candidates {
            let state = state_arc.read().await;
            if state.conversation_id == Some(conversation_id) {
                return state
                    .pending_user_message
                    .clone()
                    .map(|pending| (pending, state.pending_user_message_started_at));
            }
        }
        None
    }

    /// Resolve an `(external_id, agent_type)` (agent session) to its
    /// currently-active connection id, if any. Sibling to
    /// `find_connection_by_conversation_id`, used as the discovery fallback for
    /// the cross-client viewer attach: a connection binds its `conversation_id`
    /// only on the first prompt, but its `external_id` is set as soon as the
    /// session starts — so for a historical conversation opened by a second
    /// client *before* anyone has sent a prompt, the by-conversation lookup
    /// misses while this one still finds the live owner, letting the second
    /// client attach as a viewer instead of reusing the connection as a
    /// (mis-tagged) owner and later tearing it down.
    ///
    /// `agent_type` is part of the match because `external_id` is unique only
    /// per agent (`UNIQUE(external_id, agent_type)`), not globally — without it,
    /// a session id shared across two agents could attach a viewer to the wrong
    /// agent's connection.
    pub async fn find_connection_by_external_id(
        &self,
        external_id: &str,
        agent_type: AgentType,
    ) -> Option<String> {
        // Collect matching candidates without holding connections lock across awaits.
        let candidates: Vec<(
            String,
            Arc<tokio::sync::RwLock<crate::acp::session_state::SessionState>>,
        )> = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .filter(|(_, conn)| conn.agent_type == agent_type)
                .map(|(id, conn)| (id.clone(), conn.state.clone()))
                .collect()
        };
        for (id, state_arc) in candidates {
            let state = state_arc.read().await;
            if state.external_id.as_deref() == Some(external_id) {
                return Some(id);
            }
        }
        None
    }
}

fn log_prompt_image_summary(
    connection_id: &str,
    client_message_id: Option<&str>,
    blocks: &[PromptInputBlock],
) {
    let images: Vec<(&str, usize, bool, bool)> = blocks
        .iter()
        .filter_map(|block| match block {
            PromptInputBlock::Image {
                data,
                mime_type,
                uri,
                local_path,
            } => Some((
                mime_type.as_str(),
                data.len(),
                uri.is_some(),
                local_path
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
            )),
            _ => None,
        })
        .collect();
    if images.is_empty() {
        return;
    }
    let mime_types = images
        .iter()
        .map(|(mime, _, _, _)| *mime)
        .collect::<Vec<_>>()
        .join(",");
    let inline_lengths = images
        .iter()
        .map(|(_, length, _, _)| length.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let url_images = images.iter().filter(|(_, _, has_uri, _)| *has_uri).count();
    let local_images = images
        .iter()
        .filter(|(_, _, _, has_local_path)| *has_local_path)
        .count();
    tracing::info!(
        target: "acp.image",
        connection_id,
        client_message_id = client_message_id.unwrap_or(""),
        image_count = images.len(),
        mime_types,
        inline_lengths,
        url_images,
        local_images,
        "ACP image prompt accepted"
    );
}

/// Production impl of `ConnectionSpawner` used by `DelegationBroker`.
///
/// Bundles `Arc<ConnectionManager>` with `Arc<AppDatabase>` because
/// `cancel` writes the cancelled status onto the conversation row, which
/// happens inside `ConnectionManager::cancel`. The wrapper exists so the
/// broker can depend on a small `dyn`-able interface instead of pulling
/// in the full `AppState` graph.
///
/// `data_dir` is required so `spawn` can build a runtime env that
/// includes the git credential helper — without it, delegated subagents
/// fail any git command that depends on the iyw-claw-injected helper.
#[derive(Clone)]
pub struct ConnectionManagerSpawner {
    pub manager: Arc<ConnectionManager>,
    pub db: Arc<AppDatabase>,
    pub data_dir: Arc<PathBuf>,
}

#[async_trait::async_trait]
impl crate::acp::delegation::spawner::ConnectionSpawner for ConnectionManagerSpawner {
    async fn spawn(
        &self,
        parent_connection_id: &str,
        agent_type: AgentType,
        working_dir: Option<String>,
        preferred_mode_id: Option<String>,
        preferred_config_values: BTreeMap<String, String>,
    ) -> Result<String, crate::acp::delegation::spawner::SpawnerError> {
        use crate::acp::delegation::spawner::SpawnerError;
        // Resolve the parent connection so we can inherit its emitter and
        // owner_window. Falling back is not safe: a child whose emitter is
        // wired to a different broadcaster would emit events the frontend
        // never sees.
        let (emitter, owner_window, parent_working_dir) = {
            let conns = self.manager.connections.lock().await;
            let parent = conns.get(parent_connection_id).ok_or_else(|| {
                SpawnerError::Spawn(format!(
                    "parent connection {parent_connection_id} not found"
                ))
            })?;
            let pwd = {
                let s = parent.state.read().await;
                s.working_dir
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
            };
            (
                parent.emitter.clone(),
                parent.owner_window_label.clone(),
                pwd,
            )
        };
        let effective_working_dir = working_dir.or(parent_working_dir);

        // Build the same runtime env `acp_connect` would build for a
        // user-initiated session — disabled check, settings overrides,
        // model provider creds, git helper. Without this, delegated
        // subagents would skip the user's configuration entirely.
        let runtime_env = crate::commands::acp::build_session_runtime_env(
            &self.db,
            agent_type,
            None,
            self.data_dir.as_path(),
        )
        .await
        .map_err(|e| SpawnerError::Spawn(e.to_string()))?;
        crate::commands::acp::verify_agent_installed(agent_type, &runtime_env)
            .map_err(|e| SpawnerError::Spawn(e.to_string()))?;
        crate::acp::account_credentials::sync_agent_credentials_for_acp(&self.db.conn, agent_type)
            .await
            .map_err(|e| SpawnerError::Spawn(e.to_string()))?;

        self.manager
            .spawn_agent_with_origin(
                agent_type,
                effective_working_dir,
                None,
                runtime_env,
                owner_window,
                emitter,
                preferred_mode_id,
                preferred_config_values,
                crate::user_memory::UserMemoryOrigin::Delegation,
            )
            .await
            .map_err(|e| SpawnerError::Spawn(e.to_string()))
    }

    async fn send_prompt_linked_for_delegation(
        &self,
        conn_id: &str,
        task: String,
        link: crate::acp::delegation::spawner::DelegationLink,
    ) -> Result<i32, crate::acp::delegation::spawner::SpawnerError> {
        use crate::acp::delegation::spawner::SpawnerError;
        // The child has no caller-supplied conversation_id (it's brand new).
        // folder_id must be None too — the manager's create-new-row branch
        // requires folder_id, which we resolve from the child's working_dir
        // via folder_service. Do that lookup here so the trait stays small.
        let working_dir_pathbuf = {
            let conns = self.manager.connections.lock().await;
            let conn = conns
                .get(conn_id)
                .ok_or_else(|| SpawnerError::Send(format!("child {conn_id} not found")))?;
            let s = conn.state.read().await;
            s.working_dir.clone()
        };
        let folder_path = working_dir_pathbuf
            .ok_or_else(|| {
                SpawnerError::Send(
                    "child connection has no working_dir; cannot derive folder_id".into(),
                )
            })?
            .to_string_lossy()
            .to_string();
        let folder = crate::db::service::folder_service::add_folder(&self.db.conn, &folder_path)
            .await
            .map_err(|e| SpawnerError::Send(format!("add_folder: {e}")))?;

        let result = self
            .manager
            .send_prompt_linked(
                &self.db,
                conn_id,
                vec![PromptInputBlock::Text { text: task }],
                Some(folder.id),
                None,
                Some(link),
            )
            .await
            .map_err(|e| SpawnerError::Send(e.to_string()))?;
        result.ok_or_else(|| {
            SpawnerError::Send(
                "send_prompt_linked succeeded but no conversation_id was bound".into(),
            )
        })
    }

    async fn bind_parent_tool_call(
        &self,
        child_conversation_id: i32,
        parent_conversation_id: i32,
        delegation_call_id: &str,
        parent_tool_use_id: &str,
    ) -> Result<(), crate::acp::delegation::spawner::SpawnerError> {
        use crate::acp::delegation::spawner::SpawnerError;
        let bound = conversation_service::bind_delegation_parent_tool_call(
            &self.db.conn,
            child_conversation_id,
            parent_conversation_id,
            delegation_call_id,
            parent_tool_use_id,
        )
        .await
        .map_err(|error| SpawnerError::Bind(error.to_string()))?;
        if !bound {
            return Err(SpawnerError::Bind(format!(
                "child conversation {child_conversation_id} no longer matches delegation {delegation_call_id}"
            )));
        }
        Ok(())
    }

    async fn cancel(
        &self,
        conn_id: &str,
    ) -> Result<(), crate::acp::delegation::spawner::SpawnerError> {
        self.manager
            .cancel(&self.db.conn, conn_id)
            .await
            .map_err(|e| crate::acp::delegation::spawner::SpawnerError::Cancel(e.to_string()))
    }

    async fn disconnect(
        &self,
        conn_id: &str,
    ) -> Result<(), crate::acp::delegation::spawner::SpawnerError> {
        self.manager
            .disconnect(conn_id)
            .await
            .map_err(|e| crate::acp::delegation::spawner::SpawnerError::Disconnect(e.to_string()))
    }
}

/// Production impl of `ParentSessionLookup` for the delegation listener.
/// Resolves the parent's current `conversation_id` by reading its
/// `SessionState`. Bundled with `ConnectionManagerSpawner` here so the
/// concrete wiring lives next to the manager it depends on.
#[derive(Clone)]
pub struct ConnectionManagerParentLookup {
    pub manager: Arc<ConnectionManager>,
}

#[async_trait::async_trait]
impl crate::acp::delegation::listener::ParentSessionLookup for ConnectionManagerParentLookup {
    async fn current_conversation_id(&self, parent_connection_id: &str) -> Option<i32> {
        let state = self.manager.get_state(parent_connection_id).await?;
        let snapshot = state.read().await;
        snapshot.conversation_id
    }

    async fn current_turn_generation(&self, parent_connection_id: &str) -> Option<i64> {
        let state = self.manager.get_state(parent_connection_id).await?;
        let snapshot = state.read().await;
        Some(snapshot.turn_generation)
    }
}

/// Production impl of `SessionFeedbackAccess` for the delegation listener's
/// `check_user_feedback` arm. Resolves the parent connection's pending feedback
/// by delegating to `ConnectionManager::read_pending_feedback` /
/// `commit_feedback_delivered`. Mirrors
/// `ConnectionManagerParentLookup` so the listener stays unit-testable with an
/// in-memory stub.
#[derive(Clone)]
pub struct ConnectionManagerFeedbackLookup {
    pub manager: Arc<ConnectionManager>,
}

#[async_trait::async_trait]
impl SessionFeedbackAccess for ConnectionManagerFeedbackLookup {
    async fn read_pending_feedback(&self, parent_connection_id: &str) -> Vec<PendingFeedback> {
        self.manager
            .read_pending_feedback(parent_connection_id)
            .await
    }

    async fn commit_feedback_delivered(&self, parent_connection_id: &str, ids: Vec<String>) {
        self.manager
            .commit_feedback_delivered(parent_connection_id, ids)
            .await
    }
}

/// Production impl of `SessionQuestionAccess` for the delegation listener's
/// `ask_user_question` arm. Registers / cancels the parent connection's pending
/// question by delegating to `ConnectionManager`. Mirrors
/// `ConnectionManagerFeedbackLookup` so the listener stays unit-testable with an
/// in-memory stub.
#[derive(Clone)]
pub struct ConnectionManagerQuestionLookup {
    pub manager: Arc<ConnectionManager>,
}

#[async_trait::async_trait]
impl SessionQuestionAccess for ConnectionManagerQuestionLookup {
    async fn register_question(
        &self,
        parent_connection_id: &str,
        questions: Vec<QuestionSpec>,
    ) -> Option<RegisteredQuestion> {
        self.manager
            .register_question(parent_connection_id, questions)
            .await
    }

    async fn cancel_question(&self, parent_connection_id: &str, question_id: &str) {
        self.manager
            .cancel_question(parent_connection_id, question_id)
            .await
    }

    async fn cancel_questions_by_parent(&self, parent_connection_id: &str) {
        self.manager
            .cancel_questions_by_parent(parent_connection_id)
            .await
    }
}
