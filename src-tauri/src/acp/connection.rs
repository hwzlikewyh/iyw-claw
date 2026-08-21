use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::future::Future;
use std::panic::{resume_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures::FutureExt;
use sacp::schema::{
    BlobResourceContents, CancelNotification, ContentBlock, ContentChunk, EmbeddedResource,
    EmbeddedResourceResource, ImageContent, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PermissionOptionKind, Plan, PlanEntryPriority,
    PlanEntryStatus, PromptRequest, RequestPermissionRequest, RequestPermissionResponse,
    ResourceLink, ResumeSessionRequest, ResumeSessionResponse, SessionConfigKind,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectGroup,
    SessionConfigSelectOption, SessionConfigSelectOptions, SessionId, SessionModeState,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, SetSessionModeRequest, StopReason, TerminalExitStatus,
    TextContent, TextResourceContents, ToolCallContent,
};
use sacp::schema::{HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio};
use sacp::util::MatchDispatch;
use sacp::{Agent, ConnectionTo, Dispatch, Responder, SessionMessage, UntypedMessage};
use sacp_tokio::AcpAgent;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

use crate::acp::agent_input_capabilities::NativeSteerOutcome;
use crate::acp::agent_storage::AgentStoragePaths;
use crate::acp::automatic_mode::automatic_mode_id;
use crate::acp::background_watch;
use crate::acp::capability_policy::{runtime_enforcer, Capability, CapabilityRevocationMonitor};
use crate::acp::error::AcpError;
use crate::acp::file_system_runtime::FileSystemRuntime;
use crate::acp::npm_runtime;
use crate::acp::permission_queue::{PermissionQueue, QueuedPermission};
use crate::acp::permission_runtime::{PermissionRequestMeta, PermissionRuntime};
use crate::acp::registry::{self, AgentDistribution};
use crate::acp::session_config_compat::resolve_preferred_session_config;
use crate::acp::session_recovery::{
    RecoveryBudget, RecoveryFailure, RecoveryProgress, RecoveryStage,
};
use crate::acp::session_state::{SessionState, ToolCallStatus};
use crate::acp::stderr_tail::StderrTail;
use crate::acp::terminal_runtime::{TerminalRuntime, TerminalRuntimeError};
use crate::acp::turn_output::{
    finish_turn_reason, DropLogThrottle, DropSite, EmptyTurnReport, TurnOutputProbe,
};
use crate::acp::types::{
    AcpEvent, AvailableCommandInfo, ConnectionInfo, ConnectionStatus, PermissionOptionInfo,
    PlanEntryInfo, PromptCapabilitiesInfo, PromptInputBlock, SessionConfigKindInfo,
    SessionConfigOptionInfo, SessionConfigSelectGroupInfo, SessionConfigSelectInfo,
    SessionConfigSelectOptionInfo, SessionModeInfo, SessionModeStateInfo, ToolCallImageInfo,
    UserMessageBlock,
};
use crate::models::agent::AgentType;
use crate::network::proxy;
use crate::web::event_bridge::{emit_with_state, EventEmitter};

const DEFAULT_COMMAND_COLOR_ENV: [(&str, &str); 1] = [("CLICOLOR_FORCE", "1")];
const IYW_CLAW_MCP_SERVER_PREFIX: &str = "iyw-claw-builtin-";
const IYW_CLAW_MCP_SERVER_MAX_CHARS: usize = 32;
const LOCAL_FILE_PROMPT_PREFIX: &str =
    "Local file path (use filesystem tools, not MCP resources): ";
const MAX_INLINE_RESOURCE_WIRE_BYTES: usize = 256 * 1024;
const MAX_PROMPT_RESOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESOURCE_URI_DISPLAY_BYTES: usize = 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSteerRequest {
    session_id: SessionId,
    prompt: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionSteerResponse {
    outcome: String,
}

async fn send_native_steer(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    agent_type: AgentType,
    blocks: Vec<PromptInputBlock>,
) -> NativeSteerOutcome {
    let prompt = match map_prompt_blocks(agent_type, blocks) {
        Ok(prompt) => prompt,
        Err(error) => return NativeSteerOutcome::Failed(error),
    };
    let request = match UntypedMessage::new(
        "_session/steering",
        SessionSteerRequest {
            session_id: session_id.clone(),
            prompt,
        },
    ) {
        Ok(request) => request,
        Err(error) => return NativeSteerOutcome::Failed(error.to_string()),
    };
    let response = match cx.send_request_to(Agent, request).block_task().await {
        Ok(response) => response,
        Err(error) => return NativeSteerOutcome::Failed(error.to_string()),
    };
    match serde_json::from_value::<SessionSteerResponse>(response) {
        Ok(response) => match response.outcome.as_str() {
            "injected" => NativeSteerOutcome::Injected,
            "startedNewTurn" => NativeSteerOutcome::StartedNewTurn,
            "failed" => NativeSteerOutcome::Failed("agent rejected native steering".into()),
            "unsupported" => NativeSteerOutcome::Unsupported,
            other => NativeSteerOutcome::Failed(format!("unknown native steer outcome: {other}")),
        },
        Err(error) => NativeSteerOutcome::Failed(format!("invalid native steer response: {error}")),
    }
}

fn log_stdio_debug_line(agent_name: &str, direction: &str, line: &str) {
    let (method, id_kind) = match serde_json::from_str::<serde_json::Value>(line) {
        Ok(serde_json::Value::Object(object)) => {
            let method = object
                .get("method")
                .and_then(serde_json::Value::as_str)
                .filter(|value| value.len() <= 128)
                .unwrap_or("<none>")
                .to_string();
            let id_kind = match object.get("id") {
                Some(serde_json::Value::String(_)) => "string",
                Some(serde_json::Value::Number(_)) => "number",
                Some(serde_json::Value::Null) => "null",
                Some(_) => "other",
                None => "absent",
            };
            (method, id_kind.to_string())
        }
        _ => ("<non-json>".to_string(), "unknown".to_string()),
    };
    tracing::debug!(
        target: "acp.stdio",
        agent = agent_name,
        direction,
        bytes = line.len(),
        method,
        id_kind,
        "ACP JSON-RPC debug frame"
    );
}

fn capture_agent_stderr(stderr_tail: &StderrTail, line: &str, builtin_prompt: &str) {
    let line = redact_builtin_prompt(line, builtin_prompt);
    stderr_tail.push(&line);
}

fn redact_builtin_prompt(value: &str, builtin_prompt: &str) -> String {
    const REDACTED: &str = "<built-in prompt redacted>";
    let mut redacted = value.to_string();
    for variant in builtin_prompt_redaction_variants(builtin_prompt) {
        redacted = redacted.replace(&variant, REDACTED);
    }
    if builtin_prompt_fragment_present(&redacted, builtin_prompt) {
        return REDACTED.to_string();
    }
    redacted
}

fn builtin_prompt_redaction_variants(prompt: &str) -> Vec<String> {
    let mut variants = vec![
        prompt.to_string(),
        prompt.replace('\r', "\\r").replace('\n', "\\n"),
    ];
    push_encoded_redaction(&mut variants, prompt);
    for line in prompt.lines().map(str::trim).filter(|line| line.len() >= 8) {
        variants.push(line.to_string());
        push_encoded_redaction(&mut variants, line);
    }
    variants.sort_by_key(|value| std::cmp::Reverse(value.len()));
    variants.dedup();
    variants
}

fn push_encoded_redaction(variants: &mut Vec<String>, value: &str) {
    if let Ok(encoded) = serde_json::to_string(value) {
        if let Some(inner) = encoded.get(1..encoded.len().saturating_sub(1)) {
            variants.push(inner.to_string());
        }
    }
    let debug = format!("{value:?}");
    if let Some(inner) = debug.get(1..debug.len().saturating_sub(1)) {
        variants.push(inner.to_string());
    }
}

fn builtin_prompt_fragment_present(value: &str, prompt: &str) -> bool {
    const FRAGMENT_CHARS: usize = 24;
    const STEP_CHARS: usize = 12;
    prompt.lines().any(|line| {
        let chars = line.chars().collect::<Vec<_>>();
        chars
            .windows(FRAGMENT_CHARS)
            .step_by(STEP_CHARS)
            .any(|window| value.contains(&window.iter().collect::<String>()))
    })
}

fn merge_agent_env(
    env: &[(&'static str, &'static str)],
    runtime_env: &BTreeMap<String, String>,
) -> Vec<(String, String)> {
    // Env var order is not semantically meaningful; use map overwrite semantics
    // to keep precedence while avoiding repeated O(n) scans.
    let mut merged = BTreeMap::<String, String>::new();

    for (key, value) in DEFAULT_COMMAND_COLOR_ENV {
        merged.insert(key.to_string(), value.to_string());
    }

    for (key, value) in env {
        merged.insert((*key).to_string(), (*value).to_string());
    }

    for (key, value) in runtime_env {
        if key == crate::commands::acp::MANAGED_AGENT_VERSION_ENV {
            continue;
        }
        merged.insert(key.clone(), value.clone());
    }

    for (key, value) in proxy::current_proxy_env_vars() {
        merged.insert(key, value);
    }
    ensure_loopback_proxy_bypass(&mut merged);

    // Ensure agent-invoked `officecli …` (from an enabled office skill) resolves
    // even when iyw-claw installed the binary outside the user's shell PATH — the
    // Windows self-managed dir, or `~/.local/bin` under a GUI launch.
    prepend_officecli_path(&mut merged);
    prepend_internet_tools_path(&mut merged);
    crate::wecom_ai::reapply_runtime_path(&mut merged);

    merged.into_iter().collect()
}

fn ensure_loopback_proxy_bypass(env: &mut BTreeMap<String, String>) {
    const REQUIRED: [&str; 4] = ["127.0.0.1", "localhost", "::1", "[::1]"];
    let matching_keys = env
        .keys()
        .filter(|key| key.eq_ignore_ascii_case("NO_PROXY"))
        .cloned()
        .collect::<Vec<_>>();
    let mut entries = Vec::<String>::new();
    for key in matching_keys {
        if let Some(value) = env.remove(&key) {
            append_proxy_bypass_entries(&mut entries, &value);
        }
    }
    for (key, value) in std::env::vars() {
        if key.eq_ignore_ascii_case("NO_PROXY") {
            append_proxy_bypass_entries(&mut entries, &value);
        }
    }
    for required in REQUIRED {
        if !entries
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(required))
        {
            entries.push(required.to_string());
        }
    }
    let value = entries.join(",");
    env.insert("NO_PROXY".to_string(), value.clone());
    env.insert("no_proxy".to_string(), value);
}

fn append_proxy_bypass_entries(entries: &mut Vec<String>, value: &str) {
    for candidate in value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if !entries
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(candidate))
        {
            entries.push(candidate.to_string());
        }
    }
}

/// Prepend `dir` to the PATH entry of `env`, seeding from `fallback_path` when
/// `env` has no PATH key of its own. Removes any pre-existing PATH key first
/// (case-insensitively when `windows`, since Windows env keys are
/// case-insensitive) so the result has exactly one PATH entry — otherwise a
/// differently-cased duplicate (e.g. an inherited `Path` plus an inserted
/// `PATH`) could clobber the injected value when the child `Command` applies
/// them. Pure (no env/fs access) so it is unit-tested for both platforms.
fn prepend_dir_to_path_env(
    env: &mut BTreeMap<String, String>,
    dir: &str,
    fallback_path: &str,
    windows: bool,
) {
    let sep = if windows { ';' } else { ':' };
    // Collect every PATH-ish key. `BTreeMap` iterates sorted, so when several
    // differently-cased keys exist (e.g. both `Path` and `PATH`), the last is
    // the one the child `Command` applies last — i.e. the effective value under
    // Windows' case-insensitive env. Remove all of them so exactly one PATH
    // entry remains; a stale duplicate could otherwise overwrite the injected
    // value when the child applies them in order.
    let matching: Vec<String> = env
        .keys()
        .filter(|k| {
            if windows {
                k.eq_ignore_ascii_case("PATH")
            } else {
                k.as_str() == "PATH"
            }
        })
        .cloned()
        .collect();
    let mut existing_val: Option<String> = None;
    for k in &matching {
        existing_val = env.remove(k);
    }
    let existing_val = existing_val.unwrap_or_else(|| fallback_path.to_string());
    let new_path = if existing_val.is_empty() {
        dir.to_string()
    } else {
        format!("{dir}{sep}{existing_val}")
    };
    // Reuse the effective (last-sorted) key's casing when present; otherwise
    // default to the platform-conventional name (`Path` on Windows, `PATH` on Unix).
    let key = matching
        .into_iter()
        .next_back()
        .unwrap_or_else(|| if windows { "Path" } else { "PATH" }.to_string());
    env.insert(key, new_path);
}

/// Prepend iyw-claw's known OfficeCLI install dir to `env`'s PATH when officecli is
/// installed there but not yet on the live PATH (see
/// `office_tools::officecli_agent_path_dir`). Applied to both the agent process
/// env (`merge_agent_env`) and the ACP terminal runtime's base env, so an
/// agent-invoked `officecli` resolves whether the agent execs it directly or
/// runs it through the client `terminal/create` tool. PATH-only: never forwards
/// model/API secrets.
fn prepend_officecli_path(env: &mut BTreeMap<String, String>) {
    if let Some(dir) = crate::commands::office_tools::officecli_agent_path_dir() {
        let fallback = std::env::var("PATH").unwrap_or_default();
        prepend_dir_to_path_env(env, &dir.to_string_lossy(), &fallback, cfg!(windows));
    }
}

fn prepend_internet_tools_path(env: &mut BTreeMap<String, String>) {
    let fallback = std::env::var("PATH").unwrap_or_default();
    for dir in crate::commands::internet_tools::private_tool_bin_dirs() {
        prepend_dir_to_path_env(env, &dir.to_string_lossy(), &fallback, cfg!(windows));
    }
    for (key, value) in crate::commands::internet_tools::private_tool_environment() {
        env.insert(key.to_string(), value.to_string_lossy().to_string());
    }
}

/// Commands sent from Tauri command handlers to the ACP connection loop.
pub enum ConnectionCommand {
    Prompt {
        blocks: Vec<PromptInputBlock>,
        /// Private launch context sent only on the ACP wire. Kept separate from
        /// `blocks` so user events, prompt ledgers, previews, and titles retain
        /// the exact original input.
        user_context: Option<Arc<str>>,
        /// Pre-projected cross-client user-message broadcast (`message_id` +
        /// user blocks), computed by the manager under the prompt lock. The
        /// loop emits it as `AcpEvent::UserMessage` right before issuing the
        /// agent request, so its seq strictly precedes the turn's assistant /
        /// status events (viewers apply in seq order) and it only fires for a
        /// prompt actually being processed. `None` for delegation children,
        /// empty prompts, unbound conversations, and non-linked senders.
        user_messages: Vec<(String, Vec<UserMessageBlock>)>,
        /// Internal acknowledgement that the connection loop accepted and
        /// queued the ACP request. Durable force batches settle only after this
        /// boundary so a command-channel enqueue is never mistaken for delivery.
        accepted: Option<tokio::sync::oneshot::Sender<()>>,
    },
    SetMode {
        mode_id: String,
    },
    SetConfigOption {
        config_id: String,
        value_id: String,
    },
    Cancel,
    /// Cancel only at a scheduler-proven safe boundary. Unlike manual Cancel,
    /// this preserves terminal runtimes and waits for the real prompt response
    /// or stop reason before the connection returns to idle.
    SafeCancel {
        expected_turn_generation: i64,
    },
    NativeSteer {
        message_id: String,
        blocks: Vec<PromptInputBlock>,
        codex_image_validation: Option<(PathBuf, crate::acp::agent_image_input::CodexImageScope)>,
        expected_turn_generation: i64,
        reply: tokio::sync::oneshot::Sender<NativeSteerOutcome>,
        settled: tokio::sync::oneshot::Receiver<()>,
    },
    RespondPermission {
        request_id: String,
        option_id: String,
    },
    Fork {
        reply:
            tokio::sync::oneshot::Sender<Result<crate::acp::types::ForkProtocolResult, AcpError>>,
    },
    Disconnect,
}

/// Sentinel string embedded in a `sacp::Error` when the Initialize
/// handshake times out. Converted back to `AcpError::InitializeTimeout`
/// by the outer `.map_err(...)` in `run_connection`.

/// RAII guard that removes the `AgentConnection` entry from the manager
/// map when dropped. Runs on both normal task exit AND task panic, so a
/// panic inside `run_connection` can't leak a stale map entry.
///
/// The `Mutex` is async, so we take two paths:
/// - If the lock is immediately available (`try_lock` succeeds), remove
///   the entry synchronously in the current context.
/// - Otherwise, spawn a short-lived cleanup task to acquire the lock
///   and remove the entry asynchronously. The guard must hold owned
///   `Arc<Mutex<_>>` and `String` so the spawned task has `'static`
///   captures.
struct ConnectionCleanupGuard {
    connections: Arc<tokio::sync::Mutex<HashMap<String, AgentConnection>>>,
    connection_id: String,
}

impl Drop for ConnectionCleanupGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.connections.try_lock() {
            guard.remove(&self.connection_id);
            return;
        }
        let connections = self.connections.clone();
        let connection_id = std::mem::take(&mut self.connection_id);
        tokio::spawn(async move {
            connections.lock().await.remove(&connection_id);
        });
    }
}

struct ConnectionTaskCompletion {
    completed: Arc<AtomicBool>,
}

impl Drop for ConnectionTaskCompletion {
    fn drop(&mut self) {
        self.completed.store(true, Ordering::Release);
    }
}

async fn run_with_async_cleanup<T>(
    run: impl Future<Output = T>,
    cleanup: impl Future<Output = ()>,
) -> T {
    let outcome = AssertUnwindSafe(run).catch_unwind().await;
    cleanup.await;
    match outcome {
        Ok(output) => output,
        Err(payload) => resume_unwind(payload),
    }
}

struct AbruptRecoveryContext<'a> {
    progress: &'a RecoveryProgress,
    session_id: Option<&'a str>,
    connection_id: &'a str,
    agent_type: AgentType,
    state: &'a Arc<RwLock<SessionState>>,
    emitter: &'a EventEmitter,
}

async fn emit_abrupt_recovery_failure(
    context: AbruptRecoveryContext<'_>,
    error: &AcpError,
) -> bool {
    let Some(snapshot) = context.progress.take() else {
        return false;
    };
    let Some(session_id) = context.session_id else {
        return false;
    };
    context.state.write().await.mark_recovery_failed();
    tracing::warn!(
        connection_id = context.connection_id,
        agent_type = %context.agent_type,
        session_id,
        stage = snapshot.stage.as_str(),
        elapsed_ms = snapshot.elapsed_ms,
        remaining_ms = snapshot.remaining_ms,
        category = "transport",
        code = "session_recovery_transport",
        error = %error,
        "[ACP] session recovery transport ended unexpectedly"
    );
    emit_with_state(
        context.state,
        context.emitter,
        AcpEvent::SessionLoadFailed {
            session_id: session_id.to_string(),
            message: error.to_string(),
            code: "session_recovery_transport".to_string(),
        },
    )
    .await;
    emit_with_state(
        context.state,
        context.emitter,
        AcpEvent::StatusChanged {
            status: ConnectionStatus::Error,
        },
    )
    .await;
    true
}

/// Revoke every per-launch authority owned by the connection before it leaves,
/// including panic paths where `run_connection` never returns normally.
async fn cleanup_delegation_resources(
    injection: Option<&DelegationInjection>,
    connection_id: &str,
) {
    let Some(injection) = injection else {
        return;
    };
    injection.tokens.revoke_by_parent(connection_id).await;
    injection.broker.cancel_by_parent(connection_id).await;
    injection
        .questions
        .cancel_questions_by_parent(connection_id)
        .await;
    injection
        .confirmations
        .cancel_channel_confirmations_by_parent(connection_id)
        .await;
}

async fn cleanup_connection_resources(
    injection: Option<&DelegationInjection>,
    builtin_mcp: Option<&crate::acp::builtin_mcp::BuiltinMcpClient>,
    http_lease_issued: &AtomicBool,
    connection_id: &str,
    mut prompt_bridges: crate::acp::builtin_prompt_bridge::PreparedPromptBridges,
) {
    if let Some(client) = builtin_mcp {
        client.revoke_parent(connection_id).await;
        http_lease_issued.store(false, Ordering::Release);
    } else if http_lease_issued.load(Ordering::Acquire) {
        tracing::error!(
            connection_id,
            transport = "http",
            "[ACP] issued HTTP MCP lease lost its process client during cleanup"
        );
    }
    cleanup_delegation_resources(injection, connection_id).await;
    if let Err(error) = prompt_bridges.release() {
        tracing::warn!(
            connection_id,
            code = ?error.code(),
            error = %error,
            "[ACP] built-in prompt bridge cleanup failed"
        );
    }
}

/// Represents a single active ACP agent connection.
pub struct AgentConnection {
    pub id: String,
    pub agent_type: AgentType,
    pub status: ConnectionStatus,
    pub owner_window_label: String,
    pub cmd_tx: mpsc::Sender<ConnectionCommand>,
    /// Cancels process launch and session establishment before the command loop
    /// can consume `ConnectionCommand::Disconnect`.
    pub cancellation: tokio_util::sync::CancellationToken,
    /// Becomes true once the session command loop can consume Disconnect.
    pub disconnect_command_ready: Arc<AtomicBool>,
    /// Shared completion flag used by shutdown retries. Unlike a one-shot
    /// completion channel, it can be observed more than once when an outer
    /// timeout cancels `disconnect_all` before the task finishes.
    pub(crate) cleanup_completed: Arc<AtomicBool>,
    /// 后端权威的会话状态。所有 `emit_with_state` 写入此状态并自增 seq。
    /// 使用 `Arc<RwLock<_>>` 让 spawn 出的连接 task 与外部 snapshot 读取共享。
    pub state: Arc<RwLock<SessionState>>,
    /// 出口侧的事件发射器；管理器层（如 `send_prompt_linked`）需要直接发射
    /// `ConversationLinked` 等带 SessionState 写入的事件。
    pub emitter: EventEmitter,
    /// Serializes prompt sends per connection. Held across the
    /// link-check + DB write + emit + cmd_tx.send sequence so two
    /// concurrent prompts (multiple browser tabs of the same conversation,
    /// chat-channel + UI overlap) can't interleave and produce duplicate
    /// conversation rows or a confused agent that received two prompts
    /// in the same turn.
    pub prompt_lock: Arc<tokio::sync::Mutex<()>>,

    /// Canonical fingerprint of the agent's effective config (env vars + model
    /// provider creds + native config file content) captured at spawn. The
    /// running process is locked to THIS config; comparing it against a freshly
    /// recomputed fingerprint after a settings save tells us whether the session
    /// has drifted onto stale config. Immutable for the connection's lifetime.
    pub config_fingerprint: String,
    /// The most recent fingerprint seen by `refresh_connection_staleness`.
    /// Tracks "did anything change since we last looked" so a second settings
    /// save re-emits `SessionConfigStale` (re-showing a dismissed banner) while a
    /// no-op save (identical values) stays silent. Starts equal to
    /// `config_fingerprint`.
    pub last_observed_fingerprint: String,
    /// Domain-separated identity of the effective Hermes home. Immutable and
    /// content-free; used only to detect unsafe shared-home concurrency.
    pub(crate) hermes_home_hash: Option<String>,
}

impl AgentConnection {
    pub fn info(&self) -> ConnectionInfo {
        ConnectionInfo {
            id: self.id.clone(),
            agent_type: self.agent_type,
            status: self.status.clone(),
        }
    }

    pub fn request_disconnect(&self) {
        let command_queued = match self.cmd_tx.try_send(ConnectionCommand::Disconnect) {
            Ok(()) => true,
            Err(error) => {
                if matches!(error, tokio::sync::mpsc::error::TrySendError::Full(_)) {
                    tracing::info!(
                        connection_id = self.id,
                        "[ACP] disconnect command queue busy; using cancellation fallback"
                    );
                }
                false
            }
        };
        if !command_queued || !self.disconnect_command_ready.load(Ordering::Acquire) {
            self.cancellation.cancel();
        }
    }
}

/// Build an AcpAgent from registry metadata.
/// Directory handed to codex-acp via `APP_SERVER_LOGS` so its adapter-side
/// (ACP ↔ Codex app-server translation) logs land on disk for support.
///
/// Roots under the same `<cache>/app.iywclaw` tree as
/// [`binary_cache::cache_dir`] for consistency. Returns `None` — and the
/// caller injects nothing — when the system cache dir is unknown or the
/// directory can't be created: diagnostics must never block a connection.
fn codex_app_server_log_dir() -> Option<String> {
    let dir = crate::paths::codex_acp_logs_root()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.to_string_lossy().into_owned())
}

/// Pi runs through pi-acp, which spawns the actual private `pi` binary at
/// runtime. The command is injected as an absolute `PI_ACP_PI_COMMAND`; missing
/// or damaged private runtimes fail before the adapter starts.
///
/// The message contains the literal substring "is not installed", which the
/// frontend matches to show the localized SDK-missing prompt with an "Open Agent
/// Settings" action (see `src/contexts/acp-connections-context.tsx`). Do not
/// change that substring.
fn pi_launch_preflight(runtime_env: &BTreeMap<String, String>) -> Option<String> {
    let command = runtime_env
        .get("PI_ACP_PI_COMMAND")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if command.is_some_and(|path| crate::commands::acp::resolve_pi_command_path(path).is_some()) {
        return None;
    }
    Some(format!(
        "Pi is not installed: private command \"{}\" was not found. Reinstall Pi from Agent Settings.",
        command.unwrap_or("<private pi command missing>")
    ))
}

fn inherited_env_keys_to_remove(agent_type: AgentType) -> Vec<OsString> {
    if agent_type != AgentType::CodeBuddy {
        return Vec::new();
    }

    let mut keys = crate::acp::provider_overlay::CODEBUDDY_CONFLICTING_ENV_KEYS
        .iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    keys.extend(std::env::vars_os().filter_map(|(key, _)| {
        key.to_str()
            .is_some_and(crate::acp::provider_overlay::is_codebuddy_conflicting_env_key)
            .then_some(key)
    }));
    keys.sort();
    keys.dedup();
    keys
}

struct AgentLaunchSpec<'a> {
    agent_type: AgentType,
    runtime_env: &'a BTreeMap<String, String>,
    cwd: &'a Path,
    builtin_prompt: &'a str,
    stderr_tail: &'a Arc<StderrTail>,
}

struct AgentRebuildSpec {
    agent_type: AgentType,
    runtime_env: BTreeMap<String, String>,
    process_cwd: PathBuf,
    builtin_prompt: Arc<str>,
    additional_file_system_roots: Vec<PathBuf>,
}

async fn rebuild_agent(
    spec: &AgentRebuildSpec,
    stderr_tail: &Arc<StderrTail>,
) -> Result<AcpAgent, AcpError> {
    build_agent(AgentLaunchSpec {
        agent_type: spec.agent_type,
        runtime_env: &spec.runtime_env,
        cwd: &spec.process_cwd,
        builtin_prompt: &spec.builtin_prompt,
        stderr_tail,
    })
    .await
}

fn shared_runtime_host_enabled(agent_type: AgentType) -> bool {
    if agent_type != AgentType::Codex {
        return false;
    }
    !std::env::var("IYW_CLAW_SHARED_ACP_HOST")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off"
            )
        })
}

fn stderr_tail_for_runtime_host(agent_type: AgentType) -> Arc<StderrTail> {
    if shared_runtime_host_enabled(agent_type) {
        Arc::new(StderrTail::disabled())
    } else {
        Arc::new(StderrTail::new())
    }
}

fn runtime_host_process_cwd<'a>(
    agent_type: AgentType,
    storage: &'a AgentStoragePaths,
    session_cwd: &'a Path,
) -> &'a Path {
    if shared_runtime_host_enabled(agent_type) {
        storage.root().as_path()
    } else {
        session_cwd
    }
}

async fn build_agent(spec: AgentLaunchSpec<'_>) -> Result<AcpAgent, AcpError> {
    let AgentLaunchSpec {
        agent_type,
        runtime_env,
        cwd,
        builtin_prompt,
        stderr_tail,
    } = spec;
    let meta = registry::try_get_agent_meta(agent_type)
        .ok_or_else(|| AcpError::protocol("Agent identity is not trusted for execution"))?;
    debug_assert_eq!(meta.agent_type, agent_type);

    let agent = match meta.distribution {
        AgentDistribution::Npx { cmd, args, env, .. } => {
            let storage = AgentStoragePaths::active().ok_or_else(|| {
                AcpError::SdkNotInstalled(
                    "Agent storage is not initialized. Choose a private storage directory in Agent Settings."
                        .to_string(),
                )
            })?;
            let version = runtime_env
                .get(crate::commands::acp::MANAGED_AGENT_VERSION_ENV)
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AcpError::SdkNotInstalled(format!(
                        "{} is not installed. Please install it in Agent Settings.",
                        meta.name
                    ))
                })?;
            let command =
                npm_runtime::resolve_private_npm_command(&storage, agent_type, version, cmd)
                    .ok_or_else(|| {
                        AcpError::SdkNotInstalled(format!(
                            "{} is not installed. Please install it in Agent Settings.",
                            meta.name
                        ))
                    })?;
            // pi-acp spawns the real `pi` binary; fail fast with a clear,
            // install-prompt-routable error if it (or a BYO-pi override) isn't
            // resolvable, rather than letting pi-acp die mid-connection on a raw
            // ENOENT that surfaces as an opaque protocol error.
            if agent_type == AgentType::Pi {
                if let Some(message) = pi_launch_preflight(runtime_env) {
                    return Err(AcpError::SdkNotInstalled(message));
                }
                // Trust the workspace iyw-claw is launching pi into (default on, via
                // the PI_ACP_TRUST_WORKSPACE env_json key) so pi loads the
                // project's local config/skills without a redundant prompt. Gates
                // config loading only, never execution; scoped, additive, and
                // best-effort (never blocks the connect).
                crate::commands::acp::seed_pi_workspace_trust(cwd, runtime_env);
            }
            let mut merged_env = merge_agent_env(env, runtime_env)
                .into_iter()
                .collect::<BTreeMap<_, _>>();
            let prefix = npm_runtime::private_npm_prefix(&storage, agent_type, version)?;
            let bin_dir = npm_runtime::npm_prefix_bin_dir(&prefix);
            let inherited_path = std::env::var("PATH").unwrap_or_default();
            prepend_dir_to_path_env(
                &mut merged_env,
                &bin_dir.to_string_lossy(),
                &inherited_path,
                cfg!(windows),
            );
            crate::wecom_ai::reapply_runtime_path(&mut merged_env);
            if agent_type == AgentType::Pi {
                let private_pi =
                    npm_runtime::resolve_private_npm_command(&storage, agent_type, version, "pi")
                        .ok_or_else(|| {
                        AcpError::SdkNotInstalled(
                            "Pi is not installed. Reinstall it from Agent Settings.".to_string(),
                        )
                    })?;
                merged_env.insert(
                    "PI_ACP_PI_COMMAND".to_string(),
                    private_pi.to_string_lossy().into_owned(),
                );
            }
            // codex-acp 1.0.0 honors APP_SERVER_LOGS as a directory for its
            // adapter-side logs. Surface it only under IYW_CLAW_ACP_DEBUG so
            // default runs are unchanged; a directory-creation failure silently
            // skips injection (diagnostics must never block a connect).
            let want_codex_logs = agent_type == AgentType::Codex
                && std::env::var("IYW_CLAW_ACP_DEBUG")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
            if want_codex_logs {
                if let Some(dir) = codex_app_server_log_dir() {
                    merged_env.insert("APP_SERVER_LOGS".to_string(), dir);
                }
            }
            let mut parts: Vec<String> = Vec::new();
            for (k, v) in &merged_env {
                parts.push(format!("{k}={v}"));
            }
            parts.push(command.to_string_lossy().into_owned());
            if agent_type == AgentType::Grok {
                parts.extend(crate::acp::grok::launch_args(args, Some(builtin_prompt)));
            } else {
                parts.extend(args.iter().map(|arg| (*arg).to_string()));
                crate::acp::builtin_agent_prompt::append_launch_args(
                    agent_type,
                    &mut parts,
                    builtin_prompt,
                );
            }
            // Translate OpenClaw-specific env vars to CLI flags
            if agent_type == AgentType::OpenClaw {
                if let Some(url) = runtime_env
                    .get("OPENCLAW_GATEWAY_URL")
                    .filter(|v| !v.is_empty())
                {
                    parts.push("--url".into());
                    parts.push(url.clone());
                }
                if let Some(key) = runtime_env
                    .get("OPENCLAW_SESSION_KEY")
                    .filter(|v| !v.is_empty())
                {
                    parts.push("--session".into());
                    parts.push(key.clone());
                }
                // When creating a new conversation (no session_id to resume),
                // pass --reset-session so OpenClaw mints a fresh transcript
                // instead of appending to the previous one.
                if runtime_env
                    .get("OPENCLAW_RESET_SESSION")
                    .is_some_and(|v| v == "1")
                {
                    parts.push("--reset-session".into());
                }
            }
            let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
            let prompt_for_log = builtin_prompt.to_string();
            let tail = Arc::clone(stderr_tail);
            AcpAgent::from_args(&refs)
                .map(|a| {
                    a.with_debug(move |line, dir| {
                        if dir == sacp_tokio::LineDirection::Stderr {
                            capture_agent_stderr(&tail, line, &prompt_for_log);
                        }
                    })
                })
                .map_err(|e| {
                    AcpError::SpawnFailed(redact_builtin_prompt(&e.to_string(), builtin_prompt))
                })
        }
        AgentDistribution::Binary {
            version: _,
            cmd,
            args,
            env,
            platforms,
        } => {
            let storage = AgentStoragePaths::active().ok_or_else(|| {
                AcpError::SdkNotInstalled(
                    "Agent storage is not initialized. Choose a private storage directory in Agent Settings."
                        .to_string(),
                )
            })?;
            let platform = registry::current_platform();
            if !registry::binary_platform_supported(agent_type, platforms) {
                return Err(AcpError::PlatformNotSupported(format!(
                    "{} is not available on {platform}",
                    meta.name
                )));
            }
            if platforms.is_empty() {
                tracing::debug!(
                    agent = meta.name,
                    agent_type = %agent_type,
                    "trusted ManagedBinary platform delegated to Fusion artifact"
                );
            }

            // Session-page connect must never trigger a download. Resolve only
            // the explicitly active version so pin/switch/rollback decisions
            // cannot be bypassed by a newer directory left in the cache.
            //
            // INVARIANT: the substring "is not installed" is matched
            // verbatim by the frontend catch block in
            // `src/contexts/acp-connections-context.tsx` to surface a
            // localized install prompt. Do not change the wording.
            let active_version = runtime_env
                .get(crate::commands::acp::MANAGED_AGENT_VERSION_ENV)
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AcpError::SdkNotInstalled(format!(
                        "{} is not installed. Please install it in Agent Settings.",
                        meta.name
                    ))
                })?;
            let binary_path = crate::acp::binary_cache::find_cached_binary_for_agent(
                &storage,
                agent_type,
                active_version,
                cmd,
            )?
            .ok_or_else(|| {
                AcpError::SdkNotInstalled(format!(
                    "{} is not installed. Please install it in Agent Settings.",
                    meta.name
                ))
            })?;
            tracing::info!("[ACP][{}] Using active binary {active_version}", meta.name);

            let binary_str = binary_path.to_string_lossy().to_string();
            let binary_size = std::fs::metadata(&binary_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let mut server = McpServerStdio::new(meta.name, &binary_str);
            let cmd_args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
            if !cmd_args.is_empty() {
                server = server.args(cmd_args);
            }
            let merged_env = merge_agent_env(env, runtime_env);
            let env_key_list: Vec<&str> = merged_env.iter().map(|(k, _)| k.as_str()).collect();
            if !merged_env.is_empty() {
                let env_vars: Vec<sacp::schema::EnvVariable> = merged_env
                    .iter()
                    .map(|(k, v)| sacp::schema::EnvVariable::new(k, v))
                    .collect();
                server = server.env(env_vars);
            }
            // Log only bounded launch metadata. Full paths, arguments, and
            // environment values can contain user or credential material.
            tracing::info!(
                agent = meta.name,
                binary_name = binary_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("<unknown>"),
                binary_size,
                platform = registry::current_platform(),
                arg_count = args.len(),
                env_key_count = env_key_list.len(),
                "[ACP] managed binary resolved"
            );

            // Stdio logging policy:
            // - stderr is captured in a redacted, bounded in-memory ring and
            //   is never written as raw INFO output.
            // - stdin / stdout carry JSON-RPC traffic that includes
            //   prompt text, tool-call arguments, file read/write
            //   contents, and permission-response payloads — all of
            //   which may contain API keys pasted by users or file
            //   contents the agent is editing. They are gated behind
            //   the `IYW_CLAW_ACP_DEBUG=1` env var so production builds
            //   don't persist user content into OS-level log files
            //   (Console.app on macOS, journald on Linux).
            // - Debug logging extracts only the JSON-RPC envelope metadata;
            //   it never writes the complete frame.
            let stdio_debug_enabled = std::env::var("IYW_CLAW_ACP_DEBUG")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let agent_name = meta.name.to_string();
            let prompt_for_log = builtin_prompt.to_string();
            let tail = Arc::clone(stderr_tail);
            Ok(
                AcpAgent::new(sacp::schema::McpServer::Stdio(server)).with_debug(
                    move |line, dir| {
                        match dir {
                            sacp_tokio::LineDirection::Stderr => {
                                capture_agent_stderr(&tail, line, &prompt_for_log);
                            }
                            sacp_tokio::LineDirection::Stdout if stdio_debug_enabled => {
                                log_stdio_debug_line(&agent_name, "stdout", line);
                            }
                            sacp_tokio::LineDirection::Stdin if stdio_debug_enabled => {
                                log_stdio_debug_line(&agent_name, "stdin", line);
                            }
                            _ => {}
                        }
                    },
                ),
            )
        }
        AgentDistribution::Uvx {
            package,
            cmd,
            args,
            env,
            python,
            system_cmd: _,
            ..
        } => {
            let storage = AgentStoragePaths::active().ok_or_else(|| {
                AcpError::SdkNotInstalled(
                    "Agent storage is not initialized. Choose a private storage directory in Agent Settings."
                        .to_string(),
                )
            })?;
            let mut merged_env = merge_agent_env(env, runtime_env)
                .into_iter()
                .collect::<BTreeMap<_, _>>();
            let active_version = runtime_env
                .get(crate::commands::acp::MANAGED_AGENT_VERSION_ENV)
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AcpError::SdkNotInstalled(format!(
                        "{} is not installed. Please install it in Agent Settings.",
                        meta.name
                    ))
                })?;
            let uv_env = crate::acp::version_center::uvx_bundle_env(
                &storage,
                agent_type,
                active_version,
            )
            .unwrap_or_else(|| crate::acp::binary_cache::uv_runtime_env(&storage));
            for (key, value) in uv_env {
                merged_env.insert(key.to_string(), value.to_string_lossy().into_owned());
            }
            let mut parts: Vec<String> = Vec::new();
            for (k, v) in &merged_env {
                parts.push(format!("{k}={v}"));
            }
            if let Some(uvx_path) = crate::acp::binary_cache::find_cached_uv_tool(&storage, "uvx") {
                // Primary: `uvx [--python <ver>] --from <pinned package> <entry
                // script>`. uvx fetches + caches the pinned package on first use;
                // the `--python` pin keeps it on an interpreter the agent
                // supports (see the registry `python` field).
                parts.push(uvx_path.to_string_lossy().to_string());
                parts.extend(crate::commands::acp::uvx_python_args(python));
                parts.push("--offline".into());
                parts.push("--from".into());
                if !crate::acp::binary_cache::is_uvx_agent_version_prepared(
                    &storage,
                    agent_type,
                    active_version,
                ) {
                    return Err(AcpError::SdkNotInstalled(format!(
                        "{} is not installed. Please install it in Agent Settings.",
                        meta.name
                    )));
                }
                parts.push(crate::commands::acp::uvx_package_spec_for_version(
                    package,
                    active_version,
                )?);
                parts.push(cmd.to_string());
                for a in args {
                    parts.push((*a).into());
                }
            } else {
                // INVARIANT: the substring "is not installed" is matched
                // verbatim by the frontend catch block in
                // `src/contexts/acp-connections-context.tsx` to surface a
                // localized install prompt. Do not change the wording.
                return Err(AcpError::SdkNotInstalled(format!(
                    "{} is not installed. Please install it in Agent Settings.",
                    meta.name
                )));
            }
            let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
            let prompt_for_log = builtin_prompt.to_string();
            let tail = Arc::clone(stderr_tail);
            AcpAgent::from_args(&refs)
                .map(|a| {
                    a.with_debug(move |line, dir| {
                        if dir == sacp_tokio::LineDirection::Stderr {
                            capture_agent_stderr(&tail, line, &prompt_for_log);
                        }
                    })
                })
                .map_err(|e| {
                    AcpError::SpawnFailed(redact_builtin_prompt(&e.to_string(), builtin_prompt))
                })
        }
    }?
    .with_removed_envs(inherited_env_keys_to_remove(agent_type));

    // Run the agent subprocess in the session's working directory rather than
    // iyw-claw's own process cwd (a desktop app launched from the Dock often
    // inherits "/"). A coding agent belongs in its project root. This is
    // required for Hermes, whose local terminal backend force-exports
    // TERMINAL_CWD = os.getcwd() at import (clobbering any inherited value)
    // and reports that as the agent's "Current working directory" in its
    // system prompt — without pinning it would believe it lives in "/". For
    // agents that already use the ACP session/new cwd this is a harmless
    // alignment (process cwd == session cwd). Guard on an existing directory
    // so a not-yet-created working_dir (e.g. a worktree path) can't make the
    // spawn fail.
    Ok(if cwd.is_dir() {
        agent.with_current_dir(cwd)
    } else {
        agent
    })
}

/// Spawn an ACP agent process and run the connection loop in a background task.
///
/// On success, the newly created `AgentConnection` is inserted into
/// `connections` before this function returns. The background task
/// automatically removes the entry from `connections` once `run_connection`
/// exits (timeout, error, or clean disconnect), so the manager never
/// leaks stale entries after a connection tears down.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn_agent_connection(
    connection_id: String,
    agent_type: AgentType,
    working_dir: Option<String>,
    session_id: Option<String>,
    runtime_env: BTreeMap<String, String>,
    owner_window_label: String,
    emitter: EventEmitter,
    connections: Arc<tokio::sync::Mutex<HashMap<String, AgentConnection>>>,
    connection_tasks: Arc<crate::acp::connection_tasks::ConnectionTaskRegistry>,
    runtime_hosts: Arc<crate::acp::runtime_host::RuntimeHostRegistry>,
    startup_trace: crate::acp::startup_trace::StartupTrace,
    preferred_mode_id: Option<String>,
    preferred_config_values: BTreeMap<String, String>,
    user_memory_context: crate::user_memory::UserMemoryContextSnapshot,
    delegation_injection: Option<DelegationInjection>,
    builtin_mcp: Option<crate::acp::builtin_mcp::BuiltinMcpClient>,
    version_center_db: Option<sea_orm::DatabaseConnection>,
) -> Result<tokio::sync::oneshot::Receiver<()>, AcpError> {
    let spawn_gate = connection_tasks.begin_spawn().await?;
    // 恢复会话（session_id 为 Some）走 reconcile_resumed_session：保持策略
    // 代际，只刷新允许热更新的安全字段；新建会话走完整 reconcile。两种路径
    // 都先经过 provider overlay 门，失败都会阻止 spawn。
    let overlay_result = if session_id.is_some() {
        crate::acp::provider_overlay::enforce_resumed_active_provider_overlay(agent_type)
    } else {
        crate::acp::provider_overlay::enforce_active_provider_overlay(agent_type)
    };
    // This gate runs before `session_state` exists, so there is no channel to
    // emit `AcpEvent::Error` on — the frontend only sees `spawn_agent` reject.
    // Log at ERROR with the resolved storage root: when the root is
    // unresolvable or read-only, every agent fails here identically and the
    // only other trace is a `spawning connection` line with no handshake
    // after it.
    overlay_result.map_err(|error| {
        tracing::error!(
            "[ACP] provider overlay gate rejected spawn connection_id={} agent={:?} \
             resumed={} storage_root={:?} error={error}",
            connection_id,
            agent_type,
            session_id.is_some(),
            crate::acp::agent_storage::AgentStoragePaths::active().map(|s| s.root().clone()),
        );
        AcpError::protocol(format!(
            "Failed to enforce private provider configuration: {error}"
        ))
    })?;

    let hermes_memory = crate::context_governor::diagnose_hermes_memory(agent_type, &runtime_env);

    // Create the authoritative session state up front. Subsequent emit_with_state
    // calls write through this state and increment its seq counter so the first
    // event the frontend sees has seq=1, not the placeholder 0 from Phase 0.
    let mut initial_state = SessionState::new(
        connection_id.clone(),
        agent_type,
        working_dir.clone().map(PathBuf::from),
        owner_window_label.clone(),
        None, // folder_id 由后续 prompt handler 在首次 send 时绑定 (Phase 2)
    );
    initial_state.requested_external_id = session_id.clone();
    initial_state.current_model = preferred_config_values
        .get("model")
        .map(|model| model.trim())
        .filter(|model| !model.is_empty())
        .map(str::to_string);
    initial_state.managed_agent_version = runtime_env
        .get(crate::commands::acp::MANAGED_AGENT_VERSION_ENV)
        .map(String::as_str)
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string);
    initial_state.startup_trace = Some(startup_trace.clone());
    initial_state.hermes_memory = hermes_memory.native_memory;
    initial_state.user_memory_context = user_memory_context;
    if session_id.is_some() {
        // The external session already retains the user-memory envelope in its
        // own history. Re-injecting here would mix policy generations. Memory setting
        // changes therefore take full effect only in a fresh conversation.
        initial_state.mark_user_context_already_present();
    }

    // Install the SessionStarted dedup signal BEFORE wrapping into Arc so the
    // first event (StatusChanged{Connecting} below) doesn't race with the
    // installer. The receiver is returned to `spawn_agent`, which holds the
    // per-session dedup lock until this rx fires (or times out / aborts).
    let session_started_rx = initial_state.install_session_started_signal();

    let session_state = Arc::new(RwLock::new(initial_state));

    emit_with_state(
        &session_state,
        &emitter,
        AcpEvent::StatusChanged {
            status: ConnectionStatus::Connecting,
        },
    )
    .await;

    // Align ~/.hermes/.env's base-URL var with config.yaml's model.base_url so
    // Hermes' auxiliary tasks (title generation, compression, …) resolve the
    // same endpoint as the main conversation. Best-effort; never blocks launch.
    if agent_type == AgentType::Hermes {
        crate::commands::acp::reconcile_hermes_runtime_env(&runtime_env);
    }

    // Resolve the launch cwd from the same `working_dir` (via the same helper)
    // that run_connection uses for the session/new request, so the process
    // cwd, the ACP session cwd, and any os.getcwd()-derived agent state all
    // agree. Computed here because `working_dir` is moved into run_connection
    // below.
    let launch_cwd = resolve_working_dir(working_dir.as_deref());
    let config_fingerprint = crate::commands::acp::fingerprint_config(agent_type, &runtime_env);
    // `build_agent` resolves agent storage, the managed version env key, and the
    // private npm command. Every one of those failures returns before the
    // `tokio::spawn` block below, which is the *only* site that emits
    // `AcpEvent::Error`. Without this arm the frontend is left on the
    // `Connecting` status emitted above and falls back to a generic toast, and
    // nothing at all reaches the log. Emit + log here so the failure is
    // attributable to a concrete path.
    let process_prepare_stage = startup_trace.stage("process_prepare");
    let stderr_tail = stderr_tail_for_runtime_host(agent_type);
    let launch = async {
        let storage = AgentStoragePaths::active().ok_or_else(|| {
            AcpError::SdkNotInstalled(
                "Agent storage is not initialized. Choose a private storage directory in Agent Settings."
                    .to_string(),
            )
        })?;
        let prepared = crate::acp::builtin_prompt_injection::prepare(
            crate::acp::builtin_prompt_injection::PrepareRequest {
                agent_type,
                connection_id: &connection_id,
                session_id: session_id.as_deref(),
                environment: &runtime_env,
                storage: &storage,
            },
        )
        .await?;
        let process_fingerprint =
            crate::commands::acp::fingerprint_config(agent_type, &prepared.environment);
        let process_cwd =
            runtime_host_process_cwd(agent_type, &storage, &launch_cwd).to_path_buf();
        let agent = build_agent(AgentLaunchSpec {
            agent_type,
            runtime_env: &prepared.environment,
            cwd: &process_cwd,
            builtin_prompt: &prepared.prompt.text,
            stderr_tail: &stderr_tail,
        })
        .await?;
        Ok::<_, AcpError>((agent, prepared, process_fingerprint, process_cwd))
    }
    .await;
    let (agent, prepared_prompt, process_fingerprint, process_cwd) = match launch {
        Ok(launch) => {
            process_prepare_stage.finish("ok");
            launch
        }
        Err(error) => {
            process_prepare_stage.finish("error");
            cleanup_delegation_resources(delegation_injection.as_ref(), &connection_id).await;
            tracing::error!(
                "[ACP] agent launch preparation failed connection_id={} agent={:?} cwd={} \
                 code={:?} storage_root={:?} error={error}",
                connection_id,
                agent_type,
                launch_cwd.display(),
                error.code(),
                crate::acp::agent_storage::AgentStoragePaths::active().map(|s| s.root().clone()),
            );
            let code = error.code().map(String::from);
            emit_with_state(
                &session_state,
                &emitter,
                AcpEvent::Error {
                    message: error.to_string(),
                    agent_type: agent_type.to_string(),
                    code,
                    details: None,
                    // Terminal: the connection was never inserted into the map
                    // and no `run_connection` task will follow this.
                    terminal: true,
                },
            )
            .await;
            emit_with_state(
                &session_state,
                &emitter,
                AcpEvent::StatusChanged {
                    status: ConnectionStatus::Error,
                },
            )
            .await;
            return Err(error);
        }
    };
    let crate::acp::builtin_prompt_injection::PreparedBuiltinPrompt {
        environment: launch_environment,
        prompt: builtin_prompt,
        bridges: prompt_bridges,
        openclaw,
    } = prepared_prompt;
    let agent_rebuild = AgentRebuildSpec {
        agent_type,
        runtime_env: launch_environment,
        process_cwd,
        builtin_prompt: Arc::clone(&builtin_prompt.text),
        additional_file_system_roots: crate::acp::deepseek_config::managed_file_system_roots(
            agent_type,
        ),
    };

    // Forward only the iyw-claw git credential helper keys into the terminal
    // runtime — not the agent's API tokens or model provider credentials.
    // This makes `git fetch`/`git push` issued through the ACP
    // `terminal/create` tool authenticate via the same helper path the
    // agent process uses, while keeping unrelated secrets scoped to the
    // agent and out of arbitrary shell commands it runs.
    let mut terminal_base_env: BTreeMap<String, String> = runtime_env
        .iter()
        .filter(|(k, _)| k.starts_with("GIT_CONFIG_"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    // Also surface a iyw-claw-installed OfficeCLI on the terminal's PATH: agents run
    // office skills' `officecli …` through this `terminal/create` tool, not as a
    // child of the agent process, so the agent-env injection alone wouldn't reach
    // them right after install (before install.ps1's User-PATH change lands).
    prepend_officecli_path(&mut terminal_base_env);
    prepend_internet_tools_path(&mut terminal_base_env);
    crate::acp::runtime_context::prepend_tool_dirs(
        crate::acp::agent_storage::AgentStoragePaths::active().as_ref(),
        &mut terminal_base_env,
    );
    crate::wecom_ai::inherit_terminal_environment(&runtime_env, &mut terminal_base_env);

    let (cmd_tx, cmd_rx) = mpsc::channel::<ConnectionCommand>(32);
    let cancellation = tokio_util::sync::CancellationToken::new();
    let disconnect_command_ready = Arc::new(AtomicBool::new(false));
    let http_lease_issued = Arc::new(AtomicBool::new(false));
    let cleanup_completed = Arc::new(AtomicBool::new(false));
    let conn_id = connection_id.clone();
    let emitter_clone = emitter.clone();
    let cleanup_connections = connections.clone();
    let cleanup_connection_id = connection_id.clone();
    let state_clone = Arc::clone(&session_state);
    let recovery_in_progress = Arc::new(RecoveryProgress::new());
    let recovery_in_progress_for_run = Arc::clone(&recovery_in_progress);
    let recovery_session_id = session_id.clone();

    // Insert the entry BEFORE spawning the background task so that a
    // fast-failing `run_connection` can never remove it before it was
    // inserted (would otherwise leak the entry).
    let task_connection_id = connection_id.clone();
    connections.lock().await.insert(
        connection_id.clone(),
        AgentConnection {
            id: connection_id,
            agent_type,
            status: ConnectionStatus::Connecting,
            owner_window_label,
            cmd_tx,
            cancellation: cancellation.clone(),
            disconnect_command_ready: Arc::clone(&disconnect_command_ready),
            cleanup_completed: Arc::clone(&cleanup_completed),
            state: Arc::clone(&session_state),
            emitter: emitter.clone(),
            prompt_lock: Arc::new(tokio::sync::Mutex::new(())),
            last_observed_fingerprint: config_fingerprint.clone(),
            config_fingerprint,
            hermes_home_hash: hermes_memory.home_hash,
        },
    );

    let (task_registered_tx, task_registered_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _parent_cancellation_guard = cancellation.clone().drop_guard();
        let connection_started_at = std::time::Instant::now();
        let _completion = ConnectionTaskCompletion {
            completed: cleanup_completed,
        };
        // RAII guard: runs on normal exit AND on panic unwinding, so a
        // panic inside `run_connection` can't leak a stale map entry.
        let _cleanup = ConnectionCleanupGuard {
            connections: cleanup_connections,
            connection_id: cleanup_connection_id,
        };
        if task_registered_rx.await.is_err() {
            return;
        }

        let cleanup_injection = delegation_injection.clone();
        let cleanup_builtin_mcp = builtin_mcp.clone();
        let cleanup_http_lease_issued = Arc::clone(&http_lease_issued);
        let result = run_with_async_cleanup(
            run_connection(
                agent,
                agent_rebuild,
                conn_id.clone(),
                agent_type,
                working_dir,
                session_id,
                recovery_in_progress_for_run,
                cmd_rx,
                emitter_clone.clone(),
                Arc::clone(&state_clone),
                terminal_base_env,
                preferred_mode_id,
                preferred_config_values,
                delegation_injection,
                builtin_mcp,
                builtin_prompt,
                openclaw.session_key,
                process_fingerprint,
                runtime_hosts,
                version_center_db,
                stderr_tail,
                cancellation,
                disconnect_command_ready,
                http_lease_issued,
            ),
            cleanup_connection_resources(
                cleanup_injection.as_ref(),
                cleanup_builtin_mcp.as_ref(),
                cleanup_http_lease_issued.as_ref(),
                &conn_id,
                prompt_bridges,
            ),
        )
        .await;
        let connection_failed = result.is_err();

        if agent_type == AgentType::Codex {
            tokio::spawn(crate::acp::codex_rollout_migration::retry_deferred_migrations());
        }

        if let Err(e) = result {
            let recovery_handled = emit_abrupt_recovery_failure(
                AbruptRecoveryContext {
                    progress: &recovery_in_progress,
                    session_id: recovery_session_id.as_deref(),
                    connection_id: &conn_id,
                    agent_type,
                    state: &state_clone,
                    emitter: &emitter_clone,
                },
                &e,
            )
            .await;
            if !recovery_handled {
                let code = e.code().map(String::from);
                let detail = safe_error_detail(&e.to_string());
                // The frontend gets an `AcpEvent::Error` from here, but until this
                // log existed the backend recorded nothing: a failure inside
                // `connect_with` (the OS-level process spawn) left only the
                // `spawning connection` line with no `Sending Initialize` after
                // it, because the connect closure that logs Initialize never runs
                // when the spawn itself fails. Log the code and message so a
                // process that dies before the handshake is attributable.
                tracing::error!(
                    connection_id = conn_id,
                    agent = %agent_type,
                    error_code = code.as_deref().unwrap_or("acp_connection_failed"),
                    error = %detail,
                    "[ACP] connection terminated with error"
                );
                emit_with_state(
                    &state_clone,
                    &emitter_clone,
                    AcpEvent::Error {
                        message: e.to_string(),
                        agent_type: agent_type.to_string(),
                        code,
                        details: None,
                        // The only genuinely terminal emit site: `run_connection`
                        // is unwinding and the next event is `Disconnected`.
                        // The lifecycle worker uses this flag to decide whether
                        // to flip the conversation row to Cancelled and to
                        // buffer the detail for the broker's cancel reason.
                        terminal: true,
                    },
                )
                .await;
                // Drive the state machine through `Error` before `Disconnected`
                // so the frontend's error-handling effect (cancelled-on-error)
                // engages — without this hop the connection would jump straight
                // to Disconnected and look like a clean shutdown.
                emit_with_state(
                    &state_clone,
                    &emitter_clone,
                    AcpEvent::StatusChanged {
                        status: ConnectionStatus::Error,
                    },
                )
                .await;
            }
        }

        let (external_session_id, conversation_id) = {
            let snapshot = state_clone.read().await;
            (
                snapshot.external_id.clone().unwrap_or_default(),
                snapshot.conversation_id,
            )
        };
        tracing::info!(
            connection_id = conn_id,
            session_id = external_session_id,
            conversation_id = conversation_id.unwrap_or_default(),
            agent = %agent_type,
            outcome = if connection_failed { "error" } else { "closed" },
            elapsed_ms = connection_started_at.elapsed().as_millis(),
            "[ACP] connection lifecycle ended"
        );

        emit_with_state(
            &state_clone,
            &emitter_clone,
            AcpEvent::StatusChanged {
                status: ConnectionStatus::Disconnected,
            },
        )
        .await;
        // `_cleanup` is dropped here — removes the connection entry from
        // the manager map. Same drop semantics apply on panic unwinding.
    });
    connection_tasks.register(task_connection_id, task).await;
    let _ = task_registered_tx.send(());
    drop(spawn_gate);

    Ok(session_started_rx)
}

pub(crate) async fn prewarm_agent_runtime(
    agent_type: AgentType,
    runtime_env: BTreeMap<String, String>,
    runtime_hosts: Arc<crate::acp::runtime_host::RuntimeHostRegistry>,
) -> Result<bool, AcpError> {
    if !shared_runtime_host_enabled(agent_type) {
        return Ok(false);
    }
    crate::acp::provider_overlay::enforce_active_provider_overlay(agent_type)
        .map_err(AcpError::protocol)?;
    let storage = AgentStoragePaths::active().ok_or_else(|| {
        AcpError::SdkNotInstalled(
            "Agent storage is not initialized. Choose a private storage directory in Agent Settings."
                .to_string(),
        )
    })?;
    let prepared = crate::acp::builtin_prompt_injection::prepare(
        crate::acp::builtin_prompt_injection::PrepareRequest {
            agent_type,
            connection_id: "runtime-host-prewarm",
            session_id: None,
            environment: &runtime_env,
            storage: &storage,
        },
    )
    .await?;
    let fingerprint = crate::commands::acp::fingerprint_config(agent_type, &prepared.environment);
    let stderr_tail = stderr_tail_for_runtime_host(agent_type);
    let agent = build_agent(AgentLaunchSpec {
        agent_type,
        runtime_env: &prepared.environment,
        cwd: runtime_host_process_cwd(agent_type, &storage, storage.root()),
        builtin_prompt: &prepared.prompt.text,
        stderr_tail: &stderr_tail,
    })
    .await?;
    let host_key = runtime_host_key(agent_type, fingerprint).await?;
    let _reservation = runtime_hosts.acquire(host_key, agent, stderr_tail).await?;
    Ok(true)
}

async fn runtime_host_key(
    agent_type: AgentType,
    process_fingerprint: String,
) -> Result<crate::acp::runtime_host::RuntimeHostKey, AcpError> {
    let identity = crate::acp::trusted_agents::runtime_identity(agent_type).ok_or_else(|| {
        AcpError::protocol(format!(
            "ACP runtime Host identity is not trusted for {agent_type}"
        ))
    })?;
    let policy = crate::acp::runtime_host_policy::resolve(agent_type).await;
    Ok(crate::acp::runtime_host::RuntimeHostKey::new(
        agent_type,
        process_fingerprint,
        crate::acp::runtime_host::RuntimeHostIdentity {
            definition_fingerprint: identity.definition_fingerprint,
            runtime_version: identity.runtime_version,
            policy,
        },
    ))
}

/// Shared permission responders and their single visible FIFO queue.
pub(crate) type PendingPermissions = crate::acp::permission_runtime::PendingPermissions;

fn map_session_modes(mode_state: &SessionModeState) -> SessionModeStateInfo {
    SessionModeStateInfo {
        current_mode_id: mode_state.current_mode_id.to_string(),
        available_modes: mode_state
            .available_modes
            .iter()
            .map(|mode| SessionModeInfo {
                id: mode.id.to_string(),
                name: mode.name.clone(),
                description: mode.description.clone(),
            })
            .collect(),
    }
}

async fn emit_session_modes(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    modes: &Option<SessionModeState>,
) {
    if let Some(mode_state) = modes {
        emit_with_state(
            state,
            emitter,
            AcpEvent::SessionModes {
                modes: map_session_modes(mode_state),
            },
        )
        .await;
    }
}

fn map_session_config_category(category: &SessionConfigOptionCategory) -> String {
    match category {
        SessionConfigOptionCategory::Mode => "mode".to_string(),
        SessionConfigOptionCategory::Model => "model".to_string(),
        SessionConfigOptionCategory::ThoughtLevel => "thought_level".to_string(),
        SessionConfigOptionCategory::Other(value) => value.clone(),
        _ => "unknown".to_string(),
    }
}

fn map_session_config_select_option(
    option: &SessionConfigSelectOption,
) -> SessionConfigSelectOptionInfo {
    SessionConfigSelectOptionInfo {
        value: option.value.to_string(),
        name: option.name.clone(),
        description: option.description.clone(),
    }
}

fn map_session_config_select_group(
    group: &SessionConfigSelectGroup,
) -> SessionConfigSelectGroupInfo {
    SessionConfigSelectGroupInfo {
        group: group.group.to_string(),
        name: group.name.clone(),
        options: group
            .options
            .iter()
            .map(map_session_config_select_option)
            .collect(),
    }
}

fn map_session_config_option(option: &SessionConfigOption) -> Option<SessionConfigOptionInfo> {
    match &option.kind {
        SessionConfigKind::Select(select) => {
            let (flat_options, groups) = match &select.options {
                SessionConfigSelectOptions::Ungrouped(options) => (
                    options
                        .iter()
                        .map(map_session_config_select_option)
                        .collect::<Vec<_>>(),
                    Vec::new(),
                ),
                SessionConfigSelectOptions::Grouped(grouped) => (
                    grouped
                        .iter()
                        .flat_map(|group| {
                            group.options.iter().map(map_session_config_select_option)
                        })
                        .collect::<Vec<_>>(),
                    grouped
                        .iter()
                        .map(map_session_config_select_group)
                        .collect::<Vec<_>>(),
                ),
                _ => (Vec::new(), Vec::new()),
            };

            Some(SessionConfigOptionInfo {
                id: option.id.to_string(),
                name: option.name.clone(),
                description: option.description.clone(),
                category: option.category.as_ref().map(map_session_config_category),
                kind: SessionConfigKindInfo::Select(SessionConfigSelectInfo {
                    current_value: select.current_value.to_string(),
                    options: flat_options,
                    groups,
                }),
            })
        }
        _ => None,
    }
}

fn map_session_config_options(
    config_options: &[SessionConfigOption],
) -> Vec<SessionConfigOptionInfo> {
    config_options
        .iter()
        .filter_map(map_session_config_option)
        .collect()
}

/// Defensive fallback for Codex's approval-preset selector.
///
/// codex-acp 1.0.0 advertises its modes through *both* standard ACP
/// `SessionModes` and an `id = "mode"` config option (see `AgentMode.ts`'s
/// `toSessionModeState()` + `toConfigOption()`), so this synthesizer is
/// normally a no-op — the early return fires because the agent already
/// surfaced "mode". We keep it only as a safety net: if a future build ever
/// omits the "mode" config option (older 0.16.0 did this when the sandbox
/// policy didn't match a preset, e.g. after `writable_roots` injection), the
/// user would otherwise lose the preset picker entirely, because the composer
/// hides the standard mode selector whenever any config option exists. Codex's
/// `set_config_option` handler accepts `config_id = "mode"` regardless of
/// whether it was advertised.
///
/// The preset ids/names/descriptions below MUST match the live adapter
/// vocabulary (`read-only` / `agent` / `agent-full-access`, default `agent`);
/// the legacy 0.16.0 ids (`auto` / `full-access`) are no longer accepted.
fn ensure_codex_mode_option(options: &mut Vec<SessionConfigOptionInfo>) {
    if options.iter().any(|o| o.id == "mode") {
        return;
    }
    options.insert(
        0,
        SessionConfigOptionInfo {
            id: "mode".to_string(),
            name: "Approval Preset".to_string(),
            description: Some(
                "Choose an approval and sandboxing preset for your session".to_string(),
            ),
            category: Some("mode".to_string()),
            kind: SessionConfigKindInfo::Select(SessionConfigSelectInfo {
                current_value: "agent".to_string(),
                options: vec![
                    SessionConfigSelectOptionInfo {
                        value: "read-only".to_string(),
                        name: "Read-only".to_string(),
                        description: Some(
                            "Requires approval to edit files and run commands.".to_string(),
                        ),
                    },
                    SessionConfigSelectOptionInfo {
                        value: "agent".to_string(),
                        name: "Agent".to_string(),
                        description: Some("Read and edit files, and run commands.".to_string()),
                    },
                    SessionConfigSelectOptionInfo {
                        value: "agent-full-access".to_string(),
                        name: "Agent (full access)".to_string(),
                        description: Some(
                            "Codex can edit files outside this workspace and run commands with \
                             network access."
                                .to_string(),
                        ),
                    },
                ],
                groups: vec![],
            }),
        },
    );
}

async fn emit_session_config_options_values(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    agent_type: AgentType,
    config_options: Vec<SessionConfigOption>,
) {
    let mut mapped = map_session_config_options(&config_options);
    if agent_type == AgentType::Codex {
        ensure_codex_mode_option(&mut mapped);
    }
    emit_with_state(
        state,
        emitter,
        AcpEvent::SessionConfigOptions {
            config_options: mapped,
        },
    )
    .await;
}

async fn emit_session_config_options_info(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    config_options: Vec<SessionConfigOptionInfo>,
) {
    emit_with_state(
        state,
        emitter,
        AcpEvent::SessionConfigOptions { config_options },
    )
    .await;
}

async fn emit_selectors_ready(state: &Arc<RwLock<SessionState>>, emitter: &EventEmitter) {
    emit_with_state(state, emitter, AcpEvent::SelectorsReady).await;
}

async fn emit_prompt_capabilities(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    capabilities: &sacp::schema::PromptCapabilities,
) {
    emit_with_state(
        state,
        emitter,
        AcpEvent::PromptCapabilities {
            prompt_capabilities: PromptCapabilitiesInfo {
                image: capabilities.image,
                audio: capabilities.audio,
                embedded_context: capabilities.embedded_context,
            },
        },
    )
    .await;
}

fn resolve_working_dir(working_dir: Option<&str>) -> PathBuf {
    match working_dir {
        Some(dir) => {
            let path = PathBuf::from(dir);
            if path.is_absolute() {
                path
            } else {
                std::env::current_dir().unwrap_or_default().join(path)
            }
        }
        None => std::env::current_dir()
            .unwrap_or_else(|_| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))),
    }
}

#[derive(Clone, Copy)]
struct SessionRequestContext<'a> {
    agent_type: AgentType,
    cwd: &'a Path,
    mcp_servers: &'a [McpServer],
    builtin_prompt: &'a str,
    openclaw_session_key: Option<&'a str>,
}

impl SessionRequestContext<'_> {
    fn meta(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        crate::acp::builtin_agent_prompt::session_meta(
            self.agent_type,
            self.builtin_prompt,
            self.openclaw_session_key,
        )
    }
}

fn build_new_session_request(context: SessionRequestContext<'_>) -> NewSessionRequest {
    let mut req = NewSessionRequest::new(context.cwd.to_path_buf());
    if let Some(meta) = context.meta() {
        req = req.meta(meta);
    }
    if !context.mcp_servers.is_empty() {
        req = req.mcp_servers(context.mcp_servers.to_vec());
    }
    req
}

fn build_load_session_request(
    context: SessionRequestContext<'_>,
    session_id: SessionId,
) -> LoadSessionRequest {
    let mut req = LoadSessionRequest::new(session_id, context.cwd.to_path_buf());
    if let Some(meta) = context.meta() {
        req = req.meta(meta);
    }
    if !context.mcp_servers.is_empty() {
        req = req.mcp_servers(context.mcp_servers.to_vec());
    }
    req
}

/// Build a `session/resume` request. Mirrors `build_load_session_request`
/// (same fields + ClaudeCode raw-SDK meta + non-empty mcp_servers); the only
/// wire difference is that `ResumeSessionRequest.mcp_servers` is
/// `skip_serializing_if = Vec::is_empty`, so an empty list is omitted from the
/// payload rather than emitted as `[]`.
fn build_resume_session_request(
    context: SessionRequestContext<'_>,
    session_id: SessionId,
) -> ResumeSessionRequest {
    let mut req = ResumeSessionRequest::new(session_id, context.cwd.to_path_buf());
    if let Some(meta) = context.meta() {
        req = req.meta(meta);
    }
    if !context.mcp_servers.is_empty() {
        req = req.mcp_servers(context.mcp_servers.to_vec());
    }
    req
}

/// Wire-level half of `session/resume`: send the request and deserialize the
/// reply into `ResumeSessionResponse`.
///
/// `sacp` 11.0.0 ships no `JsonRpcRequest` impl for `ResumeSessionRequest`, and
/// the orphan rule blocks iyw-claw from adding one, so we send via `UntypedMessage`
/// — the same in-tree pattern `set_session_config_option_inner` already uses for
/// `session/set_config_option`. On a JSON-RPC error the agent returns,
/// `block_task()` yields `Err(sacp::Error)` with `.code` / `.to_string()`
/// intact, so the caller's error ladder reads identically to the
/// `session/load` arm.
async fn send_resume_session(
    cx: &ConnectionTo<Agent>,
    req: ResumeSessionRequest,
) -> Result<(ResumeSessionResponse, Option<serde_json::Value>), RecoveryFailure> {
    let untyped_req = UntypedMessage::new("session/resume", req).map_err(|e| {
        RecoveryFailure::InvalidResponse(format!("Failed to build resume request: {e}"))
    })?;

    let raw_response = cx
        .send_request_to(Agent, untyped_req)
        .block_task()
        .await
        .map_err(RecoveryFailure::from_request_error)?;
    let models = raw_response.get("models").cloned();
    let response = serde_json::from_value(raw_response).map_err(|e| {
        RecoveryFailure::InvalidResponse(format!("Failed to parse resume response: {e}"))
    })?;
    Ok((response, models))
}

async fn send_load_session(
    cx: &ConnectionTo<Agent>,
    req: LoadSessionRequest,
) -> Result<LoadSessionResponse, RecoveryFailure> {
    let untyped_req = UntypedMessage::new("session/load", req).map_err(|e| {
        RecoveryFailure::InvalidResponse(format!("Failed to build load request: {e}"))
    })?;
    let raw_response = cx
        .send_request_to(Agent, untyped_req)
        .block_task()
        .await
        .map_err(RecoveryFailure::from_request_error)?;
    serde_json::from_value(raw_response).map_err(|e| {
        RecoveryFailure::InvalidResponse(format!("Failed to parse load response: {e}"))
    })
}

struct SessionRecoveryFailureContext<'a> {
    connection_id: &'a str,
    session_id: &'a str,
    agent_type: AgentType,
    stage: RecoveryStage,
    supports_resume: bool,
    supports_load: bool,
    budget: &'a RecoveryBudget,
    progress: &'a RecoveryProgress,
    state: &'a Arc<RwLock<SessionState>>,
    emitter: &'a EventEmitter,
}

async fn emit_session_recovery_failure(
    context: SessionRecoveryFailureContext<'_>,
    failure: RecoveryFailure,
) {
    context.progress.finish();
    context.state.write().await.mark_recovery_failed();
    let code = failure.stable_code();
    let message = safe_error_detail(&failure.message());
    tracing::warn!(
        connection_id = context.connection_id,
        agent_type = %context.agent_type,
        session_id = context.session_id,
        stage = context.stage.as_str(),
        supports_resume = context.supports_resume,
        supports_load = context.supports_load,
        elapsed_ms = context.budget.elapsed_ms(),
        remaining_ms = context.budget.remaining_ms(),
        category = failure.category(),
        code,
        error = %message,
        "[ACP] session recovery failed"
    );
    emit_with_state(
        context.state,
        context.emitter,
        AcpEvent::SessionLoadFailed {
            session_id: context.session_id.to_string(),
            message,
            code: code.to_string(),
        },
    )
    .await;
    emit_with_state(
        context.state,
        context.emitter,
        AcpEvent::StatusChanged {
            status: ConnectionStatus::Error,
        },
    )
    .await;
}

async fn send_new_session_capturing_models(
    cx: &ConnectionTo<Agent>,
    agent_type: AgentType,
    req: NewSessionRequest,
) -> Result<(NewSessionResponse, Option<serde_json::Value>), sacp::Error> {
    if agent_type != AgentType::Grok {
        return Ok((cx.send_request_to(Agent, req).block_task().await?, None));
    }
    let request = UntypedMessage::new("session/new", req).map_err(|error| {
        sacp::util::internal_error(format!("Failed to build new_session request: {error}"))
    })?;
    let raw_response = cx.send_request_to(Agent, request).block_task().await?;
    let models = raw_response.get("models").cloned();
    let response = serde_json::from_value(raw_response).map_err(|error| {
        sacp::util::internal_error(format!("Failed to parse new_session response: {error}"))
    })?;
    Ok((response, models))
}

async fn send_new_session_with_routing(
    cx: &ConnectionTo<Agent>,
    context: SessionRequestContext<'_>,
) -> Result<(NewSessionResponse, Option<serde_json::Value>), sacp::Error> {
    let new_context = if context.agent_type == AgentType::OpenClaw {
        SessionRequestContext {
            openclaw_session_key: None,
            ..context
        }
    } else {
        context
    };
    let (new_response, models) = send_new_session_capturing_models(
        cx,
        context.agent_type,
        build_new_session_request(new_context),
    )
    .await?;
    if context.agent_type != AgentType::OpenClaw {
        return Ok((new_response, models));
    }
    let session_id = new_response.session_id.clone();
    let session_key = crate::acp::builtin_prompt_openclaw::session_key(session_id.0.as_ref());
    let routed_context = SessionRequestContext {
        openclaw_session_key: Some(&session_key),
        ..context
    };
    let load_request = build_load_session_request(routed_context, session_id.clone());
    let response = cx.send_request_to(Agent, load_request).block_task().await?;
    Ok((
        NewSessionResponse::new(session_id)
            .modes(response.modes)
            .config_options(response.config_options)
            .meta(response.meta),
        models,
    ))
}

/// Whether MCP servers forwarded over the ACP wire (`session/new.mcpServers`)
/// actually reach the agent's model. Almost all adapters deliver them; pi-acp
/// accepts the field but drops it before launching the inner `pi --mode rpc`
/// process, and Pi has no native MCP. Forwarding either user servers or the
/// built-in HTTP MCP endpoint would therefore expose phantom host tools.
/// `supports_mcp` stays `true` for Pi because its ACP schema accepts the field,
/// so this remains a separate delivery gate.
fn agent_delivers_wire_mcp(agent_type: AgentType) -> bool {
    !matches!(agent_type, AgentType::Pi)
}

pub(crate) fn agent_supports_builtin_mcp(agent_type: AgentType) -> bool {
    registry::get_agent_meta(agent_type).supports_mcp && agent_delivers_wire_mcp(agent_type)
}

const HERMES_HTTP_COMPAT_PACKAGE: &str = "hermes-agent[acp,mcp]==0.19.0";
const HERMES_HTTP_COMPAT_VERSION: &str = "0.19.0";
const HERMES_HTTP_COMPAT_AGENT_NAME: &str = "hermes-agent";

/// Hermes 0.19.0 accepts ACP HTTP MCP servers but omits `http` from its
/// initialize capability response. This exception is deliberately limited to
/// the reviewed built-in identity and applies only to our gateway route; user
/// MCP entries still use the runtime capability declaration below. The
/// managed version and initialize identity are checked as well: the registry
/// definition alone does not prove which version the active runtime launched.
fn builtin_http_capability_available(
    agent_type: AgentType,
    managed_version: Option<&str>,
    initialize: &sacp::schema::InitializeResponse,
) -> bool {
    if initialize.agent_capabilities.mcp_capabilities.http {
        return true;
    }
    let compatible = agent_type == AgentType::Hermes
        && managed_version == Some(HERMES_HTTP_COMPAT_VERSION)
        && initialize.agent_info.as_ref().is_some_and(|info| {
            info.name == HERMES_HTTP_COMPAT_AGENT_NAME && info.version == HERMES_HTTP_COMPAT_VERSION
        })
        && crate::acp::trusted_agents::definition_for_agent(agent_type).is_some_and(|definition| {
            definition.package_or_binary == HERMES_HTTP_COMPAT_PACKAGE
                && definition.version_floor.minimum_tool_version == HERMES_HTTP_COMPAT_VERSION
        });
    if compatible {
        tracing::warn!(
            agent = %agent_type,
            reviewed_version = HERMES_HTTP_COMPAT_VERSION,
            "applying built-in-only Hermes HTTP MCP capability compatibility"
        );
    } else if agent_type == AgentType::Hermes {
        tracing::warn!(
            managed_version = managed_version.unwrap_or("<missing>"),
            agent_name = initialize
                .agent_info
                .as_ref()
                .map(|info| info.name.as_str())
                .unwrap_or("<missing>"),
            agent_version = initialize
                .agent_info
                .as_ref()
                .map(|info| info.version.as_str())
                .unwrap_or("<missing>"),
            "Hermes HTTP compatibility exception rejected because runtime identity was not the reviewed version"
        );
    }
    compatible
}

fn intersect_trusted_session_capabilities(
    agent_type: AgentType,
    runtime_resume: bool,
    runtime_load: bool,
) -> (bool, bool) {
    let Some(definition) = crate::acp::trusted_agents::definition_for_agent(agent_type) else {
        return (runtime_resume, runtime_load);
    };
    let supports_resume = runtime_resume && definition.capabilities.resume;
    let supports_load = runtime_load && definition.capabilities.load;
    if runtime_resume != supports_resume || runtime_load != supports_load {
        tracing::warn!(
            agent_type = %agent_type,
            runtime_resume,
            runtime_load,
            trusted_resume = definition.capabilities.resume,
            trusted_load = definition.capabilities.load,
            "[ACP] ignored session recovery capabilities outside trusted definition"
        );
    }
    (supports_resume, supports_load)
}

fn agent_reads_native_mcp_config(agent_type: AgentType) -> bool {
    matches!(
        agent_type,
        AgentType::Codex | AgentType::Hermes | AgentType::KimiCode | AgentType::Grok
    )
}

/// Load MCP servers configured for `agent_type` and convert them into the
/// ACP wire format. Errors and unsupported entries are logged and skipped so
/// a single malformed entry never blocks a session from starting.
fn load_mcp_servers_for_agent(agent_type: AgentType) -> Vec<McpServer> {
    // Codex, Hermes, Kimi Code, and Grok read their own native MCP config at
    // launch. Codex reads `[mcp_servers.<name>]` from its layered config;
    // Hermes uses `~/.hermes/config.yaml` (`mcp_servers`, registered as
    // `mcp-<name>` toolsets), Kimi Code uses `~/.kimi-code/mcp.json`
    // (`mcpServers`), and Grok uses `config.toml` (`[mcp_servers.<name>]`).
    // Forwarding the same entries over ACP would register every tool twice.
    // The per-connection built-in HTTP MCP endpoint is injected separately.
    if agent_reads_native_mcp_config(agent_type) {
        return Vec::new();
    }
    let entries = match crate::commands::mcp::read_servers_for_agent_type(agent_type) {
        Ok(map) => map,
        Err(err) => {
            tracing::error!(
                "[ACP][{}] failed to read MCP servers from local config: {err}",
                agent_type
            );
            return Vec::new();
        }
    };

    let mut out = Vec::with_capacity(entries.len());
    for (name, spec) in entries {
        match canonical_spec_to_mcp_server(&name, &spec) {
            Ok(server) => out.push(server),
            Err(err) => {
                tracing::warn!(
                    "[ACP][{}] skip MCP server '{name}' (cannot map to ACP schema): {err}",
                    agent_type
                );
            }
        }
    }
    out
}

/// Context the connection layer needs to issue an in-process HTTP MCP
/// authority. Built once per `run_connection` from the live AppState pieces
/// (broker config, token registry, and service endpoint) and passed through.
///
/// Optional because some test paths spin up `run_connection` without a
/// full delegation stack — those just skip injection.
#[derive(Clone)]
pub struct DelegationInjection {
    pub broker: Arc<crate::acp::delegation::broker::DelegationBroker>,
    pub tokens: Arc<crate::acp::delegation::listener::TokenRegistry>,
    /// Hot-swappable "is live-feedback enabled?" flag. Read when issuing the
    /// HTTP authority alongside the broker's delegation flag so the service
    /// exposes the correct tool groups. Shares the same token registry.
    pub feedback: crate::acp::feedback::FeedbackRuntimeConfig,
    /// Hot-swappable "is ask-user-question enabled?" flag. Read at injection
    /// time alongside delegation + feedback so the HTTP service exposes
    /// `ask_user_question` when enabled.
    pub ask: crate::acp::question::QuestionRuntimeConfig,
    /// Hot-swappable "is get-session-info enabled?" flag. Read at injection time
    /// alongside the other two so the HTTP service exposes `get_session_info`
    /// when enabled. No teardown handle (the lookup is stateless).
    pub sessions: crate::acp::session_info::SessionInfoRuntimeConfig,
    /// Question registry handle for the teardown cascade. The `run_connection`
    /// cleanup guard calls `cancel_questions_by_parent` through this so a pending
    /// `ask_user_question` is reclaimed synchronously on disconnect, mirroring
    /// the delegation `broker.cancel_by_parent` cleanup. Shares the same backing
    /// `ConnectionManager` as the listener's question lookup.
    pub questions: Arc<dyn crate::acp::question::SessionQuestionAccess>,
    pub confirmations:
        Arc<dyn crate::acp::channel_tools::confirmation::SessionChannelConfirmationAccess>,
}

/// The feature snapshot for an in-process HTTP companion authority. Image
/// display/analysis and task artifact registration are always on; the
/// remaining tool groups follow their settings flags.
///
/// Pulled out as a pure function so the feature set is unit-testable without a
/// real binary on disk or a live broker.
fn companion_features_arg(
    delegation_enabled: bool,
    feedback_enabled: bool,
    ask_enabled: bool,
    sessions_enabled: bool,
    memory_enabled: bool,
) -> String {
    let mut features: Vec<&str> = vec!["images", "artifacts", "channels"];
    if delegation_enabled {
        features.push("delegation");
    }
    if feedback_enabled {
        features.push("feedback");
    }
    if ask_enabled {
        features.push("ask");
    }
    if sessions_enabled {
        features.push("sessions");
    }
    if memory_enabled {
        features.push("memory");
    }
    features.join(",")
}

/// Outcome of injecting the in-process HTTP companion, plus whether the
/// `check_user_feedback` tool was exposed to this agent.
struct CompanionInjection {
    feedback_available: bool,
    memory_tools_expected: bool,
}

struct PreparedCompanion {
    server: McpServer,
    injection: CompanionInjection,
}

struct CompanionLaunchPreparation {
    health: crate::user_memory::CompanionHealthSnapshot,
    companion: Option<PreparedCompanion>,
    policy_monitor: Option<CapabilityRevocationMonitor>,
}

struct MemoryLaunchAccess {
    confirmed_append: bool,
    candidate_proposal: bool,
    recall: bool,
    turn_tracker: Arc<crate::acp::memory_turn::MemoryTurnTracker>,
}

async fn user_memory_runtime_after_companion_ready(
    injection: Option<&DelegationInjection>,
    companion: Option<&CompanionInjection>,
    health: &crate::user_memory::CompanionHealthSnapshot,
) -> crate::user_memory::UserMemoryRuntimeEnvironment {
    let unavailable = || crate::user_memory::UserMemoryRuntimeEnvironment {
        companion_health: health.clone(),
        host_bridge_available: false,
    };
    let (Some(_injection), Some(companion)) = (injection, companion) else {
        return unavailable();
    };
    if !companion.memory_tools_expected {
        return crate::user_memory::UserMemoryRuntimeEnvironment {
            companion_health: health.clone(),
            host_bridge_available: true,
        };
    }
    crate::user_memory::UserMemoryRuntimeEnvironment {
        companion_health: health.clone(),
        host_bridge_available: true,
    }
}

struct UserMemoryLaunchFinalization<'a> {
    injection: Option<&'a DelegationInjection>,
    companion: Option<&'a CompanionInjection>,
    health: &'a crate::user_memory::CompanionHealthSnapshot,
    resumed: bool,
}

async fn finalize_user_memory_launch(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    launch: UserMemoryLaunchFinalization<'_>,
    version_center_db: Option<&sea_orm::DatabaseConnection>,
) {
    let runtime = user_memory_runtime_after_companion_ready(
        launch.injection,
        launch.companion,
        launch.health,
    )
    .await;
    {
        let mut session = state.write().await;
        if launch.resumed {
            session
                .user_memory_context
                .finalize_resumed_runtime(runtime.clone());
            // Force context re-injection on resume: the companion may have been
            // replaced (host restart / upgrade) and the model's in-context tool
            // guidance is stale. Resetting this flag causes the updated Memory
            // maintenance block to be re-sent in the next user turn.
            if session.user_memory_context.memory_write_enabled {
                session.user_context_injected = false;
            }
        } else {
            session
                .user_memory_context
                .finalize_runtime(runtime.clone());
            session.user_context_injected = false;
        }
        session.user_memory_capabilities = session.user_memory_context.capabilities.clone();
        session.mark_launch_finalized();
    }
    emit_with_state(
        state,
        emitter,
        AcpEvent::StatusChanged {
            status: ConnectionStatus::Connected,
        },
    )
    .await;
    if let Some(conn) = version_center_db {
        let agent_type = state.read().await.agent_type;
        if let Err(error) = crate::acp::version_center::promote_agent_lkg(conn, agent_type).await {
            tracing::warn!(
                agent_type = ?agent_type,
                error = %error,
                "[agent-version-center] failed to promote successful Agent version to LKG"
            );
        }
    }
}

/// Resolve the feature snapshot used for an in-process HTTP MCP authority.
///
/// The built-in companion is exposed only through the main process HTTP
/// service. There is deliberately no stdio/sidecar construction here. An
/// Agent route that should support the gateway fails closed when HTTP cannot
/// be injected; policy-excluded routes remain available without the gateway.
struct ResolvedCompanionFeatures {
    features: crate::acp::delegation::companion::CompanionFeatures,
    features_arg: String,
    feedback_available: bool,
    memory_tools_expected: bool,
}

async fn resolve_companion_features(
    injection: &DelegationInjection,
    capability_tools: &[String],
    memory_access: &MemoryLaunchAccess,
) -> ResolvedCompanionFeatures {
    let has_tool = |name: &str| capability_tools.iter().any(|tool| tool == name);
    let delegation_enabled = injection.broker.config_snapshot().await.enabled
        && [
            "delegate_to_agent",
            "get_delegation_status",
            "cancel_delegation",
        ]
        .into_iter()
        .all(has_tool);
    let feedback_available =
        injection.feedback.is_enabled().await && has_tool("check_user_feedback");
    let ask_enabled = injection.ask.is_enabled().await && has_tool("ask_user_question");
    let sessions_enabled = injection.sessions.is_enabled().await && has_tool("get_session_info");
    let mut features_arg = companion_features_arg(
        delegation_enabled,
        feedback_available,
        ask_enabled,
        sessions_enabled,
        memory_access.confirmed_append,
    );
    let browser_enabled = cfg!(all(
        feature = "tauri-runtime",
        target_os = "windows",
        target_arch = "x86_64"
    )) && crate::browser::BROWSER_AGENT_TOOL_NAMES
        .iter()
        .all(|tool| has_tool(tool));
    append_optional_companion_features(&mut features_arg, memory_access, browser_enabled);
    let features = crate::acp::delegation::companion::CompanionFeatures::parse(Some(&features_arg));
    ResolvedCompanionFeatures {
        features,
        features_arg,
        feedback_available,
        memory_tools_expected: memory_access.confirmed_append
            || memory_access.candidate_proposal
            || memory_access.recall,
    }
}

fn append_optional_companion_features(
    features_arg: &mut String,
    memory_access: &MemoryLaunchAccess,
    browser_enabled: bool,
) {
    if browser_enabled {
        features_arg.push_str(",browser");
    }
    if memory_access.candidate_proposal {
        features_arg.push_str(",memory-proposal");
    }
    if memory_access.recall {
        features_arg.push_str(",memory-recall");
    }
}

#[derive(Debug)]
enum ConnectionAttemptError {
    Protocol(sacp::Error),
    BuiltinMcp(AcpError),
}

impl From<sacp::Error> for ConnectionAttemptError {
    fn from(error: sacp::Error) -> Self {
        Self::Protocol(error)
    }
}

impl From<AcpError> for ConnectionAttemptError {
    fn from(error: AcpError) -> Self {
        Self::BuiltinMcp(error)
    }
}
fn in_process_companion_health(
    client: &crate::acp::builtin_mcp::BuiltinMcpClient,
) -> crate::user_memory::CompanionHealthSnapshot {
    crate::user_memory::CompanionHealthSnapshot {
        status: crate::user_memory::CompanionHealthStatus::Ready,
        reason: crate::user_memory::CompanionHealthReason::Ready,
        expected_version: env!("CARGO_PKG_VERSION").to_string(),
        detected_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        selected_path: None,
        advertised_tools: client.advertised_tools().to_vec(),
        detail: None,
    }
}

struct CompanionLaunchContext<'a> {
    injection: Option<&'a DelegationInjection>,
    builtin_mcp: Option<&'a crate::acp::builtin_mcp::BuiltinMcpClient>,
    http_lease_issued: &'a AtomicBool,
    connection_id: &'a str,
    working_dir: &'a Path,
    agent_type: AgentType,
    state: &'a Arc<RwLock<SessionState>>,
    agent_http_capable: bool,
    parent_cancellation: &'a tokio_util::sync::CancellationToken,
}

fn http_session_authority(
    context: &CompanionLaunchContext<'_>,
    resolved: &ResolvedCompanionFeatures,
    memory_access: &MemoryLaunchAccess,
    server_name: &str,
) -> crate::acp::builtin_mcp::SessionAuthority {
    crate::acp::builtin_mcp::SessionAuthority::new(
        crate::acp::builtin_mcp::SessionIdentity::new(
            context.connection_id.to_string(),
            context.working_dir.to_path_buf(),
            context.agent_type,
            server_name.to_string(),
        ),
        crate::acp::builtin_mcp::FeatureSnapshot::capture(resolved.features),
        crate::acp::builtin_mcp::MemoryPermissions::new(
            memory_access.confirmed_append,
            memory_access.candidate_proposal,
            memory_access.recall,
        ),
    )
    .with_parent_cancellation(context.parent_cancellation)
}

async fn prepare_http_companion(
    context: &CompanionLaunchContext<'_>,
    client: &crate::acp::builtin_mcp::BuiltinMcpClient,
    injection: &DelegationInjection,
) -> Result<CompanionLaunchPreparation, crate::acp::builtin_mcp::BuiltinMcpIssueError> {
    let health = in_process_companion_health(client);
    let memory_access = project_memory_launch_access(context.state, &health, true).await;
    let resolved =
        resolve_companion_features(injection, client.capability_tools(), &memory_access).await;
    let server_name = builtin_mcp_server_name();
    let authority = http_session_authority(context, &resolved, &memory_access, &server_name);
    let bearer = client
        .issue(authority, Arc::clone(&memory_access.turn_tracker))
        .await?;
    context.http_lease_issued.store(true, Ordering::Release);
    let server =
        McpServerHttp::new(server_name.clone(), client.endpoint()).headers(vec![HttpHeader::new(
            "Authorization",
            format!("Bearer {}", bearer.as_str()),
        )]);
    tracing::info!(
        connection_id = context.connection_id,
        agent = %context.agent_type,
        transport = "http",
        server_name,
        endpoint = %client.endpoint(),
        features = %resolved.features_arg,
        "[ACP] selected built-in MCP transport: main-process HTTP"
    );
    Ok(CompanionLaunchPreparation {
        health,
        companion: Some(PreparedCompanion {
            server: McpServer::Http(server),
            injection: CompanionInjection {
                feedback_available: resolved.feedback_available,
                memory_tools_expected: resolved.memory_tools_expected,
            },
        }),
        policy_monitor: None,
    })
}

fn unavailable_companion_launch() -> CompanionLaunchPreparation {
    CompanionLaunchPreparation {
        health: Default::default(),
        companion: None,
        policy_monitor: None,
    }
}

async fn companion_policy_monitor(
    context: &CompanionLaunchContext<'_>,
) -> Result<Option<CapabilityRevocationMonitor>, AcpError> {
    if !agent_supports_builtin_mcp(context.agent_type) {
        tracing::info!(
            connection_id = context.connection_id,
            agent = %context.agent_type,
            transport = "unavailable",
            reason = "agent_policy_or_adapter_does_not_forward_mcp",
            "[ACP] built-in MCP is disabled for this Agent; no stdio or HTTP entry will be injected"
        );
        return Ok(None);
    }
    let enforcer = match runtime_enforcer() {
        Ok(enforcer) => enforcer,
        Err(error) => {
            tracing::warn!(
                connection_id = context.connection_id,
                agent = %context.agent_type,
                transport = "unavailable",
                error = %error,
                "[capability-policy] MCP enforcer unavailable before issuing authority"
            );
            return Err(builtin_mcp_unavailable(
                context,
                "capability policy enforcer is unavailable",
            ));
        }
    };
    match enforcer
        .monitor_existing_agent(
            context.agent_type,
            Capability::Mcp,
            true,
            Some(context.parent_cancellation.clone()),
        )
        .await
    {
        Ok(monitor) => Ok(Some(monitor)),
        Err(error) => {
            tracing::warn!(
                connection_id = context.connection_id,
                agent = %context.agent_type,
                transport = "unavailable",
                error = %error,
                "[capability-policy] Built-in MCP denied before issuing authority"
            );
            Err(builtin_mcp_unavailable(
                context,
                "capability policy denied the built-in MCP route",
            ))
        }
    }
}

async fn try_prepare_http_companion(
    context: &CompanionLaunchContext<'_>,
) -> Result<CompanionLaunchPreparation, AcpError> {
    if !context.agent_http_capable {
        tracing::info!(
            connection_id = context.connection_id,
            agent = %context.agent_type,
            transport = "unavailable",
            reason = "agent_mcp_capabilities_http_false",
            "[ACP] built-in MCP unavailable: Agent did not advertise HTTP MCP"
        );
        return Err(builtin_mcp_unavailable(
            context,
            "Agent did not advertise mcpCapabilities.http",
        ));
    }
    let injection = context
        .injection
        .ok_or_else(|| builtin_mcp_unavailable(context, "delegation injection is unavailable"))?;
    let client = context.builtin_mcp.ok_or_else(|| {
        builtin_mcp_unavailable(context, "main-process HTTP service is unavailable")
    })?;
    if !client.is_ready() {
        return Err(builtin_mcp_unavailable(
            context,
            "main-process HTTP service is not ready",
        ));
    }
    match prepare_http_companion(context, client, injection).await {
        Ok(prepared) => Ok(prepared),
        Err(error) => {
            tracing::warn!(
                connection_id = context.connection_id,
                agent = %context.agent_type,
                transport = "http",
                error = %error,
                "[ACP] failed to issue built-in HTTP MCP authority"
            );
            Err(builtin_mcp_unavailable(
                context,
                "failed to issue HTTP authority",
            ))
        }
    }
}

fn builtin_mcp_unavailable(context: &CompanionLaunchContext<'_>, reason: &str) -> AcpError {
    tracing::warn!(
        connection_id = context.connection_id,
        agent = %context.agent_type,
        agent_http_capable = context.agent_http_capable,
        injection_installed = context.injection.is_some(),
        service_ready = context.builtin_mcp.is_some_and(|client| client.is_ready()),
        transport = "unavailable",
        reason,
        "[ACP] built-in MCP unavailable; HTTP-only policy is fail-closed"
    );
    AcpError::BuiltinMcpUnavailable(reason.to_string())
}

async fn prepare_companion_launch(
    context: CompanionLaunchContext<'_>,
) -> Result<CompanionLaunchPreparation, AcpError> {
    let Some(policy_monitor) = companion_policy_monitor(&context).await? else {
        return Ok(unavailable_companion_launch());
    };
    let mut prepared = try_prepare_http_companion(&context).await?;
    if prepared.companion.is_some() {
        prepared.policy_monitor = Some(policy_monitor);
    }
    Ok(prepared)
}

async fn project_memory_launch_access(
    state: &Arc<RwLock<SessionState>>,
    health: &crate::user_memory::CompanionHealthSnapshot,
    host_bridge_available: bool,
) -> MemoryLaunchAccess {
    let session = state.read().await;
    let mut projected = session.user_memory_context.clone();
    projected.finalize_runtime(crate::user_memory::UserMemoryRuntimeEnvironment {
        companion_health: health.clone(),
        host_bridge_available,
    });
    MemoryLaunchAccess {
        confirmed_append: projected.capabilities.confirmed_append.available,
        candidate_proposal: projected.capabilities.candidate_proposal.available,
        recall: projected.recall_tool_enabled && projected.capabilities.read_context.available,
        turn_tracker: session.memory_turn_tracker.clone(),
    }
}

/// Resolve an MCP server `command` to an absolute path.
///
/// The ACP spec requires `McpServerStdio.command` to be an absolute path.
/// Users typically configure bare names like `npx` / `node` / `bunx`; if we
/// forwarded those verbatim, agents would fail to spawn the server. We try
/// `which` first, fall back to the platform-normalized form (which adds
/// `.exe`/`.cmd` on Windows), and finally to the raw input as last resort.
fn resolve_mcp_command(command: &str) -> PathBuf {
    let path = Path::new(command);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    if let Ok(found) = which::which(command) {
        return found;
    }
    PathBuf::from(crate::process::normalized_program(command))
}

fn canonical_spec_to_mcp_server(name: &str, spec: &serde_json::Value) -> Result<McpServer, String> {
    let obj = spec
        .as_object()
        .ok_or_else(|| "spec must be a JSON object".to_string())?;
    let typ = obj
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("stdio");

    match typ {
        "stdio" => {
            let command = obj
                .get("command")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "stdio MCP entry missing 'command'".to_string())?;
            // ACP spec requires an absolute path. If users wrote a bare
            // command (e.g. "npx"), resolve it via PATH so the agent can
            // actually spawn the server. Fall back to the raw value when
            // resolution fails — the agent will surface a clearer error.
            let command_path = resolve_mcp_command(command);
            let mut server = McpServerStdio::new(name, command_path);
            if let Some(args) = obj.get("args").and_then(serde_json::Value::as_array) {
                let args: Vec<String> = args
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect();
                if !args.is_empty() {
                    server = server.args(args);
                }
            }
            if let Some(env_obj) = obj.get("env").and_then(serde_json::Value::as_object) {
                let env_vars: Vec<sacp::schema::EnvVariable> = env_obj
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| sacp::schema::EnvVariable::new(k, s)))
                    .collect();
                if !env_vars.is_empty() {
                    server = server.env(env_vars);
                }
            }
            Ok(McpServer::Stdio(server))
        }
        "http" | "sse" => {
            let url = obj
                .get("url")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "remote MCP entry missing 'url'".to_string())?;
            let headers: Vec<HttpHeader> = obj
                .get("headers")
                .and_then(serde_json::Value::as_object)
                .map(|map| {
                    map.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| HttpHeader::new(k, s)))
                        .collect()
                })
                .unwrap_or_default();
            if typ == "http" {
                let mut server = McpServerHttp::new(name, url);
                if !headers.is_empty() {
                    server = server.headers(headers);
                }
                Ok(McpServer::Http(server))
            } else {
                let mut server = McpServerSse::new(name, url);
                if !headers.is_empty() {
                    server = server.headers(headers);
                }
                Ok(McpServer::Sse(server))
            }
        }
        other => Err(format!("unsupported MCP transport type '{other}'")),
    }
}

fn builtin_mcp_server_name() -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let suffix_len = IYW_CLAW_MCP_SERVER_MAX_CHARS - IYW_CLAW_MCP_SERVER_PREFIX.len();
    debug_assert!(suffix_len > 0);
    format!(
        "{IYW_CLAW_MCP_SERVER_PREFIX}{}",
        &suffix[..suffix_len.min(suffix.len())]
    )
}

/// The main ACP connection loop.
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(
    name = "connection",
    skip_all,
    fields(
        connection_id = %connection_id,
        agent_type = ?agent_type,
        working_dir = ?working_dir,
        session_id = ?session_id,
    )
)]
async fn run_connection(
    agent: AcpAgent,
    agent_rebuild: AgentRebuildSpec,
    connection_id: String,
    agent_type: AgentType,
    working_dir: Option<String>,
    session_id: Option<String>,
    recovery_in_progress: Arc<RecoveryProgress>,
    mut cmd_rx: mpsc::Receiver<ConnectionCommand>,
    emitter: EventEmitter,
    state: Arc<RwLock<SessionState>>,
    terminal_base_env: BTreeMap<String, String>,
    preferred_mode_id: Option<String>,
    preferred_config_values: BTreeMap<String, String>,
    delegation_injection: Option<DelegationInjection>,
    builtin_mcp: Option<crate::acp::builtin_mcp::BuiltinMcpClient>,
    builtin_prompt: crate::acp::builtin_agent_prompt::RenderedBuiltinPrompt,
    openclaw_session_key: Option<String>,
    process_fingerprint: String,
    runtime_hosts: Arc<crate::acp::runtime_host::RuntimeHostRegistry>,
    version_center_db: Option<sea_orm::DatabaseConnection>,
    stderr_tail: Arc<StderrTail>,
    cancellation: tokio_util::sync::CancellationToken,
    disconnect_command_ready: Arc<AtomicBool>,
    http_lease_issued: Arc<AtomicBool>,
) -> Result<(), AcpError> {
    let mut attempt_agent = Some(agent);
    let attempt_stderr_tail = stderr_tail;
    loop {
        let agent = attempt_agent
            .take()
            .ok_or_else(|| AcpError::protocol("ACP connection attempt lost its Agent builder"))?;
        let stderr_tail = Arc::clone(&attempt_stderr_tail);
        let pending_perms: PendingPermissions =
            Arc::new(tokio::sync::Mutex::new(PermissionQueue::default()));
        // `terminal_base_env` already filtered to just the credential helper
        // keys upstream — see `spawn_agent_connection` for the rationale and
        // why we don't forward the full agent runtime_env here.
        let cwd = resolve_working_dir(working_dir.as_deref());
        // Default terminals to the session working directory so an agent that calls
        // `terminal/create` without a `cwd` (e.g. CodeBuddy) runs in the folder the
        // conversation runs in rather than iyw-claw's own process cwd.
        let terminal_activity = Arc::clone(&state.read().await.active_terminal_count);
        let terminal_runtime = Arc::new(
            TerminalRuntime::with_base_env(terminal_base_env.clone(), terminal_activity)
                .with_default_cwd(Some(cwd.clone())),
        );
        let cwd_string = cwd.to_string_lossy().to_string();
        let file_system_runtime = Arc::new(FileSystemRuntime::with_additional_roots(
            cwd.clone(),
            agent_rebuild.additional_file_system_roots.clone(),
        ));

        let conn_id = connection_id.clone();
        let emitter_clone = emitter.clone();
        let perms = pending_perms.clone();
        let state_outer = Arc::clone(&state);
        let prompt_ledger = background_watch::PromptLedger::shared();
        let _background_watch = background_watch::spawn_if_claude(
            &connection_id,
            agent_type,
            Arc::clone(&state),
            emitter.clone(),
            cwd_string.clone(),
            Arc::clone(&prompt_ledger),
        );

        let shared_host = shared_runtime_host_enabled(agent_type);
        let host_key = runtime_host_key(agent_type, process_fingerprint.clone()).await?;
        let host_started = std::time::Instant::now();
        let startup_trace = state.read().await.startup_trace.clone();
        let host_stage = startup_trace
            .as_ref()
            .map(|trace| trace.stage("host_lookup_wait_spawn"));
        let host_result = if shared_host {
            match startup_trace.clone() {
                Some(trace) => {
                    runtime_hosts
                        .acquire_traced(host_key, agent, Arc::clone(&stderr_tail), trace)
                        .await
                }
                None => {
                    runtime_hosts
                        .acquire(host_key, agent, Arc::clone(&stderr_tail))
                        .await
                }
            }
        } else {
            match startup_trace.clone() {
                Some(trace) => {
                    runtime_hosts
                        .start_owned_traced(host_key, agent, Arc::clone(&stderr_tail), trace)
                        .await
                }
                None => {
                    runtime_hosts
                        .start_owned(host_key, agent, Arc::clone(&stderr_tail))
                        .await
                }
            }
        };
        let mut host = match host_result {
            Ok(host) => {
                if let Some(stage) = host_stage {
                    stage.finish("ok");
                }
                host
            }
            Err(error) => {
                if let Some(stage) = host_stage {
                    stage.finish("error");
                }
                return Err(error);
            }
        };
        let stderr_tail = host.stderr_tail();
        if let Some(pid) = host.pid() {
            state.write().await.set_agent_pid(pid);
        }
        tracing::info!(
            connection_id,
            agent = %agent_type,
            shared_host,
            elapsed_ms = host_started.elapsed().as_millis(),
            "[ACP][startup] runtime Host ready"
        );
        let host_capabilities = host.capabilities();
        let runtime_verified = host.runtime_verified();
        let _route_lease = host.register_route(
            connection_id.clone(),
            session_id.clone(),
            crate::acp::runtime_host::RuntimeSessionRoute {
                state: Arc::clone(&state),
                emitter: emitter.clone(),
                permissions: pending_perms.clone(),
                cwd: cwd_string.clone(),
                file_system: Arc::clone(&file_system_runtime),
                terminal: Arc::clone(&terminal_runtime),
                elicitation: if agent_type == AgentType::DeepSeek {
                    delegation_injection.as_ref().map(|injection| {
                        crate::acp::deepseek_elicitation::ElicitationAccess::new(
                            Arc::clone(&injection.questions),
                            injection.ask.clone(),
                        )
                    })
                } else {
                    None
                },
                host_capabilities,
                runtime_verified,
            },
        )?;
        let route_binding = _route_lease.binding();
        let terminal_cleanup = Arc::clone(&terminal_runtime);
        let cx = host.connection();
        let init_resp = host.initialize_response();
        let session_id = session_id.clone();
        let preferred_mode_id = preferred_mode_id.clone();
        let preferred_config_values = preferred_config_values.clone();
        let delegation_injection = delegation_injection.clone();
        let builtin_mcp = builtin_mcp.clone();
        let recovery_in_progress = Arc::clone(&recovery_in_progress);
        let disconnect_command_ready = Arc::clone(&disconnect_command_ready);
        let http_lease_issued = Arc::clone(&http_lease_issued);
        let builtin_prompt = builtin_prompt.clone();
        let openclaw_session_key = openclaw_session_key.clone();
        let version_center_db = version_center_db.clone();
        let mut cmd_rx = &mut cmd_rx;
        let attempt_cancellation = cancellation.clone();
        let authority_parent_cancellation = attempt_cancellation.clone();
        let connection = async move {
            let state = state_outer;
            let managed_agent_version = state.read().await.managed_agent_version.clone();
            let native_steering_available = agent_type == AgentType::Codex
                && init_resp
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.get("steering"))
                    .and_then(serde_json::Value::as_object)
                    .and_then(|steering| steering.get("supported"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
            state.write().await.native_steering_available = native_steering_available;
            tracing::info!(
                agent_type = %agent_type,
                native_steering_available,
                "[agent-input] native steering capability negotiated"
            );
            emit_prompt_capabilities(
                &state,
                &emitter_clone,
                &init_resp.agent_capabilities.prompt_capabilities,
            )
            .await;

            let supports_fork = init_resp
                .agent_capabilities
                .session_capabilities
                .fork
                .is_some();
            let runtime_supports_resume = init_resp
                .agent_capabilities
                .session_capabilities
                .resume
                .is_some();
            let runtime_supports_load = init_resp.agent_capabilities.load_session;
            let (supports_resume, supports_load) = intersect_trusted_session_capabilities(
                agent_type,
                runtime_supports_resume,
                runtime_supports_load,
            );
            state.write().await.set_recovery_capability(
                agent_type != AgentType::Cline && (supports_load || supports_resume),
            );
            tracing::info!(
                runtime_load = runtime_supports_load,
                runtime_resume = runtime_supports_resume,
                "[ACP] Agent capabilities: load_session={}, fork={}, resume={}",
                supports_load,
                supports_fork,
                supports_resume
            );

            // OpenClaw rejects non-empty MCP lists and Pi's ACP adapter drops them,
            // so neither may receive user-configured or built-in wire MCP entries.
            // This chokepoint feeds new/load/resume and the load-to-new fallback.
            let agent_supports_mcp = registry::get_agent_meta(agent_type).supports_mcp
                && agent_delivers_wire_mcp(agent_type);

            // Load MCP servers configured for this agent and filter by the
            // capabilities the agent just declared. Stdio is mandatory per
            // ACP spec; HTTP/SSE are gated on `mcp_capabilities.{http,sse}`.
            let mcp_caps = &init_resp.agent_capabilities.mcp_capabilities;
            let mut mcp_servers: Vec<McpServer> = if agent_supports_mcp {
                load_mcp_servers_for_agent(agent_type)
                    .into_iter()
                    .filter(|s| match s {
                        McpServer::Stdio(_) => true,
                        McpServer::Http(server) => {
                            if mcp_caps.http {
                                true
                            } else {
                                tracing::warn!(
                                    "[ACP][{}] skip HTTP MCP server '{}': agent does not advertise mcpCapabilities.http",
                                    agent_type, server.name
                                );
                                false
                            }
                        }
                        McpServer::Sse(server) => {
                            if mcp_caps.sse {
                                true
                            } else {
                                tracing::warn!(
                                    "[ACP][{}] skip SSE MCP server '{}': agent does not advertise mcpCapabilities.sse",
                                    agent_type, server.name
                                );
                                false
                            }
                        }
                        _ => false,
                    })
                    .collect()
            } else {
                tracing::info!(
                    "[ACP][{}] supports_mcp=false: skipping all MCP wire forwarding (user servers + built-in HTTP MCP)",
                    agent_type
                );
                Vec::new()
            };

            // Issue the built-in HTTP MCP lease per logical session. Shared Agent
            // hosts must not inherit another session's token or cwd.
            let CompanionLaunchPreparation {
                health: companion_health,
                companion,
                policy_monitor: _companion_policy_monitor,
            } = prepare_companion_launch(CompanionLaunchContext {
                injection: delegation_injection.as_ref(),
                builtin_mcp: builtin_mcp.as_ref(),
                http_lease_issued: http_lease_issued.as_ref(),
                connection_id: &conn_id,
                working_dir: &cwd,
                agent_type,
                state: &state,
                agent_http_capable: builtin_http_capability_available(
                    agent_type,
                    managed_agent_version.as_deref(),
                    &init_resp,
                ),
                parent_cancellation: &authority_parent_cancellation,
            })
            .await
            .map_err(ConnectionAttemptError::from)?;
            let delegate_injection = companion.map(|prepared| {
                mcp_servers.push(prepared.server);
                prepared.injection
            });
            {
                let mut s = state.write().await;
                // The agent's actual feedback capability for this session — the
                // authoritative gate for submit + UI, fixed at launch.
                s.feedback_tool_available = delegate_injection
                    .as_ref()
                    .is_some_and(|injected| injected.feedback_available);
            }
            // Emit fork support capability
            emit_with_state(
                &state,
                &emitter_clone,
                AcpEvent::ForkSupported {
                    supported: supports_fork,
                },
            )
            .await;

            let session_request_context = SessionRequestContext {
                agent_type,
                cwd: &cwd,
                mcp_servers: &mcp_servers,
                builtin_prompt: &builtin_prompt.text,
                openclaw_session_key: openclaw_session_key.as_deref(),
            };

            if let Some(sid) = session_id {
                let recovery_budget = RecoveryBudget::start();
                let initial_stage = if supports_resume {
                    RecoveryStage::Resume
                } else {
                    RecoveryStage::Load
                };
                recovery_in_progress.begin(initial_stage);

                // session/resume restores context without replaying history.
                // Only a complete remote RPC error leaves this connection
                // trusted enough for a subsequent session/load request.
                if supports_resume {
                    let resume_stage = startup_trace
                        .as_ref()
                        .map(|trace| trace.stage("session_resume"));
                    let resume_req = build_resume_session_request(
                        session_request_context,
                        SessionId::new(sid.clone()),
                    );
                    let resume_result = match recovery_budget.timeout_for(RecoveryStage::Resume) {
                        Some(timeout) => {
                            tokio::time::timeout(timeout, send_resume_session(&cx, resume_req))
                                .await
                                .unwrap_or(Err(RecoveryFailure::Timeout))
                        }
                        None => Err(RecoveryFailure::Timeout),
                    };
                    if let Some(stage) = resume_stage {
                        stage.finish(if resume_result.is_ok() { "ok" } else { "error" });
                    }
                    match resume_result {
                        Ok((resume_resp, grok_models_raw)) => {
                            let initial_config_options = resume_resp.config_options.clone();
                            let new_resp = NewSessionResponse::new(SessionId::new(sid.clone()))
                                .modes(resume_resp.modes)
                                .config_options(resume_resp.config_options)
                                .meta(resume_resp.meta);
                            let grok_meta = (agent_type == AgentType::Grok)
                                .then(|| new_resp.meta.clone())
                                .flatten();
                            let grok_effort_specs = (agent_type == AgentType::Grok).then(|| {
                                crate::acp::grok::parse_effort_specs(grok_models_raw.as_ref())
                            });
                            let mut session = cx.attach_session(new_resp, Default::default())?;
                            recovery_in_progress.finish();
                            state.write().await.mark_recovery_succeeded();
                            tracing::info!(
                                connection_id = conn_id,
                                agent_type = %agent_type,
                                session_id = sid,
                                stage = "resume",
                                elapsed_ms = recovery_budget.elapsed_ms(),
                                remaining_ms = recovery_budget.remaining_ms(),
                                "[ACP] session recovery succeeded"
                            );
                            finalize_user_memory_launch(
                                &state,
                                &emitter_clone,
                                UserMemoryLaunchFinalization {
                                    injection: delegation_injection.as_ref(),
                                    companion: delegate_injection.as_ref(),
                                    health: &companion_health,
                                    resumed: true,
                                },
                                version_center_db.as_ref(),
                            )
                            .await;

                            // No drain: session/resume does not replay history,
                            // so there is nothing to discard. Any buffered
                            // notification (e.g. an early AvailableCommandsUpdate)
                            // is consumed and forwarded by run_conversation_loop.

                            emit_with_state(
                                &state,
                                &emitter_clone,
                                AcpEvent::SessionStarted {
                                    session_id: sid.clone(),
                                },
                            )
                            .await;
                            emit_session_modes(&state, &emitter_clone, session.modes()).await;
                            let selectors_stage = startup_trace
                                .as_ref()
                                .map(|trace| trace.stage("selectors_ready"));
                            apply_and_emit_session_config_options(
                                &cx,
                                &mut session,
                                &state,
                                &emitter_clone,
                                agent_type,
                                grok_meta.as_ref(),
                                grok_effort_specs.as_ref(),
                                preferred_mode_id.as_deref(),
                                &preferred_config_values,
                                initial_config_options.unwrap_or_default(),
                            )
                            .await;
                            emit_selectors_ready(&state, &emitter_clone).await;
                            if let Some(stage) = selectors_stage {
                                stage.finish("ok");
                            }

                            disconnect_command_ready.store(true, Ordering::Release);
                            let loop_result = run_conversation_loop(
                                &mut session,
                                &conn_id,
                                &emitter_clone,
                                &state,
                                agent_type,
                                &perms,
                                &mut cmd_rx,
                                terminal_runtime.clone(),
                                &cwd_string,
                                supports_fork,
                                &prompt_ledger,
                                delegation_injection.as_ref(),
                                &stderr_tail,
                            )
                            .await;
                            terminal_runtime.release_all_for_session(&sid).await;
                            drop(session);
                            // Explicit return: this arm is NOT in tail position
                            // (the session/load block follows it), so without
                            // `return` a successful resume would fall into
                            // session/load.
                            return handle_fork_or_exit(
                                loop_result,
                                &conn_id,
                                &emitter_clone,
                                &state,
                                agent_type,
                                &perms,
                                &mut cmd_rx,
                                terminal_runtime.clone(),
                                &cwd,
                                &cwd_string,
                                &prompt_ledger,
                                &route_binding,
                                delegation_injection.as_ref(),
                                &stderr_tail,
                            )
                            .await
                            .map_err(ConnectionAttemptError::from);
                        }
                        Err(e) => {
                            if e.allows_load_fallback() && supports_load {
                                tracing::warn!(
                                    connection_id = conn_id,
                                    agent_type = %agent_type,
                                    session_id = sid,
                                    stage = "resume",
                                    elapsed_ms = recovery_budget.elapsed_ms(),
                                    remaining_ms = recovery_budget.remaining_ms(),
                                    category = e.category(),
                                    error = %safe_error_detail(&e.message()),
                                    "[ACP] session/resume returned RPC error; trying session/load"
                                );
                            } else {
                                emit_session_recovery_failure(
                                    SessionRecoveryFailureContext {
                                        connection_id: &conn_id,
                                        session_id: &sid,
                                        agent_type,
                                        stage: RecoveryStage::Resume,
                                        supports_resume,
                                        supports_load,
                                        budget: &recovery_budget,
                                        progress: &recovery_in_progress,
                                        state: &state,
                                        emitter: &emitter_clone,
                                    },
                                    e,
                                )
                                .await;
                                return Ok(());
                            }
                        }
                    }
                }

                if !supports_load {
                    emit_session_recovery_failure(
                        SessionRecoveryFailureContext {
                            connection_id: &conn_id,
                            session_id: &sid,
                            agent_type,
                            stage: RecoveryStage::Load,
                            supports_resume,
                            supports_load,
                            budget: &recovery_budget,
                            progress: &recovery_in_progress,
                            state: &state,
                            emitter: &emitter_clone,
                        },
                        RecoveryFailure::Unavailable(
                            "Agent does not advertise session recovery support".to_string(),
                        ),
                    )
                    .await;
                    return Ok(());
                }

                recovery_in_progress.set_stage(RecoveryStage::Load);
                let load_req = build_load_session_request(
                    session_request_context,
                    SessionId::new(sid.clone()),
                );
                let load_stage = startup_trace
                    .as_ref()
                    .map(|trace| trace.stage("session_load"));
                let load_result = match recovery_budget.timeout_for(RecoveryStage::Load) {
                    Some(timeout) => {
                        tokio::time::timeout(timeout, send_load_session(&cx, load_req))
                            .await
                            .unwrap_or(Err(RecoveryFailure::Timeout))
                    }
                    None => Err(RecoveryFailure::Timeout),
                };
                if let Some(stage) = load_stage {
                    stage.finish(if load_result.is_ok() { "ok" } else { "error" });
                }

                match load_result {
                    Ok(load_resp) => {
                        let initial_config_options = load_resp.config_options.clone();
                        let new_resp = NewSessionResponse::new(SessionId::new(sid.clone()))
                            .modes(load_resp.modes)
                            .config_options(load_resp.config_options)
                            .meta(load_resp.meta);
                        let grok_meta = (agent_type == AgentType::Grok)
                            .then(|| new_resp.meta.clone())
                            .flatten();
                        let mut session = cx.attach_session(new_resp, Default::default())?;
                        finalize_user_memory_launch(
                            &state,
                            &emitter_clone,
                            UserMemoryLaunchFinalization {
                                injection: delegation_injection.as_ref(),
                                companion: delegate_injection.as_ref(),
                                health: &companion_health,
                                resumed: true,
                            },
                            version_center_db.as_ref(),
                        )
                        .await;

                        // Drain historical replay notifications from session/load,
                        // but forward AvailableCommandsUpdate to the frontend
                        let mut drained = 0u32;
                        let mut drain_failure = None;
                        loop {
                            let Some(remaining) = recovery_budget.timeout_for(RecoveryStage::Load)
                            else {
                                drain_failure = Some(RecoveryFailure::Timeout);
                                break;
                            };
                            let idle_wait = remaining.min(std::time::Duration::from_millis(100));
                            let msg = match tokio::time::timeout(idle_wait, session.read_update())
                                .await
                            {
                                Ok(Ok(msg)) => msg,
                                Ok(Err(error)) => {
                                    drain_failure =
                                        Some(RecoveryFailure::Transport(error.to_string()));
                                    break;
                                }
                                Err(_) if remaining <= std::time::Duration::from_millis(100) => {
                                    drain_failure = Some(RecoveryFailure::Timeout);
                                    break;
                                }
                                Err(_) => break,
                            };
                            drained += 1;
                            if let SessionMessage::SessionMessage(dispatch) = msg {
                                let h = emitter_clone.clone();
                                let st = Arc::clone(&state);
                                let dispatch = fix_usage_update_nulls(dispatch);
                                let _ = MatchDispatch::new(dispatch)
                                    .if_notification(async |notif: SessionNotification| {
                                        if matches!(
                                            notif.update,
                                            SessionUpdate::AvailableCommandsUpdate(_)
                                        ) {
                                            // Historical-replay path only
                                            // forwards AvailableCommandsUpdate,
                                            // which never carries tool output or
                                            // tool-call titles — throwaway state
                                            // is fine.
                                            let mut replay_cache = ToolCallOutputCache::default();
                                            let mut replay_cb_state = CodeBuddyLiveState::default();
                                            emit_conversation_update(
                                                &st,
                                                &h,
                                                agent_type,
                                                notif.update,
                                                None,
                                                &mut replay_cache,
                                                &mut replay_cb_state,
                                            )
                                            .await;
                                        }
                                        Ok(())
                                    })
                                    .await
                                    .otherwise(async |dispatch| {
                                        maybe_emit_claude_sdk_ext_notification(&st, &h, dispatch)
                                            .await;
                                        Ok(())
                                    })
                                    .await;
                            }
                        }
                        if let Some(failure) = drain_failure {
                            drop(session);
                            emit_session_recovery_failure(
                                SessionRecoveryFailureContext {
                                    connection_id: &conn_id,
                                    session_id: &sid,
                                    agent_type,
                                    stage: RecoveryStage::Load,
                                    supports_resume,
                                    supports_load,
                                    budget: &recovery_budget,
                                    progress: &recovery_in_progress,
                                    state: &state,
                                    emitter: &emitter_clone,
                                },
                                failure,
                            )
                            .await;
                            return Ok(());
                        }
                        if drained > 0 {
                            tracing::info!(
                                "[ACP] Drained {drained} historical replay notifications"
                            );
                        }

                        recovery_in_progress.finish();
                        state.write().await.mark_recovery_succeeded();
                        tracing::info!(
                            connection_id = conn_id,
                            agent_type = %agent_type,
                            session_id = sid,
                            stage = "load",
                            replay_notifications = drained,
                            elapsed_ms = recovery_budget.elapsed_ms(),
                            remaining_ms = recovery_budget.remaining_ms(),
                            "[ACP] session recovery succeeded"
                        );

                        emit_with_state(
                            &state,
                            &emitter_clone,
                            AcpEvent::SessionStarted {
                                session_id: sid.clone(),
                            },
                        )
                        .await;
                        emit_session_modes(&state, &emitter_clone, session.modes()).await;
                        let selectors_stage = startup_trace
                            .as_ref()
                            .map(|trace| trace.stage("selectors_ready"));
                        apply_and_emit_session_config_options(
                            &cx,
                            &mut session,
                            &state,
                            &emitter_clone,
                            agent_type,
                            grok_meta.as_ref(),
                            None,
                            preferred_mode_id.as_deref(),
                            &preferred_config_values,
                            initial_config_options.unwrap_or_default(),
                        )
                        .await;
                        emit_selectors_ready(&state, &emitter_clone).await;
                        if let Some(stage) = selectors_stage {
                            stage.finish("ok");
                        }

                        disconnect_command_ready.store(true, Ordering::Release);
                        let loop_result = run_conversation_loop(
                            &mut session,
                            &conn_id,
                            &emitter_clone,
                            &state,
                            agent_type,
                            &perms,
                            &mut cmd_rx,
                            terminal_runtime.clone(),
                            &cwd_string,
                            supports_fork,
                            &prompt_ledger,
                            delegation_injection.as_ref(),
                            &stderr_tail,
                        )
                        .await;
                        terminal_runtime.release_all_for_session(&sid).await;
                        drop(session);
                        handle_fork_or_exit(
                            loop_result,
                            &conn_id,
                            &emitter_clone,
                            &state,
                            agent_type,
                            &perms,
                            &mut cmd_rx,
                            terminal_runtime.clone(),
                            &cwd,
                            &cwd_string,
                            &prompt_ledger,
                            &route_binding,
                            delegation_injection.as_ref(),
                            &stderr_tail,
                        )
                        .await
                        .map_err(ConnectionAttemptError::from)
                    }
                    Err(e) => {
                        emit_session_recovery_failure(
                            SessionRecoveryFailureContext {
                                connection_id: &conn_id,
                                session_id: &sid,
                                agent_type,
                                stage: RecoveryStage::Load,
                                supports_resume,
                                supports_load,
                                budget: &recovery_budget,
                                progress: &recovery_in_progress,
                                state: &state,
                                emitter: &emitter_clone,
                            },
                            e,
                        )
                        .await;
                        Ok(())
                    }
                }
            } else {
                // Create new session
                let new_stage = startup_trace
                    .as_ref()
                    .map(|trace| trace.stage("session_new"));
                let (new_resp, grok_models_raw) =
                    match send_new_session_with_routing(&cx, session_request_context).await {
                        Ok(response) => {
                            if let Some(stage) = new_stage {
                                stage.finish("ok");
                            }
                            response
                        }
                        Err(error) => {
                            if let Some(stage) = new_stage {
                                stage.finish("error");
                            }
                            return Err(error.into());
                        }
                    };
                let sid = new_resp.session_id.0.to_string();
                if !route_binding.bind_session(sid.clone()) {
                    return Err(sacp::util::internal_error(
                        "ACP runtime route expired before new session binding",
                    )
                    .into());
                }
                let initial_config_options = new_resp.config_options.clone();
                let grok_meta = (agent_type == AgentType::Grok)
                    .then(|| new_resp.meta.clone())
                    .flatten();
                let grok_effort_specs = (agent_type == AgentType::Grok)
                    .then(|| crate::acp::grok::parse_effort_specs(grok_models_raw.as_ref()));
                let mut session = cx.attach_session(new_resp, Default::default())?;
                finalize_user_memory_launch(
                    &state,
                    &emitter_clone,
                    UserMemoryLaunchFinalization {
                        injection: delegation_injection.as_ref(),
                        companion: delegate_injection.as_ref(),
                        health: &companion_health,
                        resumed: false,
                    },
                    version_center_db.as_ref(),
                )
                .await;
                emit_with_state(
                    &state,
                    &emitter_clone,
                    AcpEvent::SessionStarted {
                        session_id: sid.clone(),
                    },
                )
                .await;
                emit_session_modes(&state, &emitter_clone, session.modes()).await;
                let selectors_stage = startup_trace
                    .as_ref()
                    .map(|trace| trace.stage("selectors_ready"));
                apply_and_emit_session_config_options(
                    &cx,
                    &mut session,
                    &state,
                    &emitter_clone,
                    agent_type,
                    grok_meta.as_ref(),
                    grok_effort_specs.as_ref(),
                    preferred_mode_id.as_deref(),
                    &preferred_config_values,
                    initial_config_options.unwrap_or_default(),
                )
                .await;
                emit_selectors_ready(&state, &emitter_clone).await;
                if let Some(stage) = selectors_stage {
                    stage.finish("ok");
                }

                disconnect_command_ready.store(true, Ordering::Release);
                let loop_result = run_conversation_loop(
                    &mut session,
                    &conn_id,
                    &emitter_clone,
                    &state,
                    agent_type,
                    &perms,
                    &mut cmd_rx,
                    terminal_runtime.clone(),
                    &cwd_string,
                    supports_fork,
                    &prompt_ledger,
                    delegation_injection.as_ref(),
                    &stderr_tail,
                )
                .await;
                terminal_runtime.release_all_for_session(&sid).await;
                drop(session);
                handle_fork_or_exit(
                    loop_result,
                    &conn_id,
                    &emitter_clone,
                    &state,
                    agent_type,
                    &perms,
                    &mut cmd_rx,
                    terminal_runtime.clone(),
                    &cwd,
                    &cwd_string,
                    &prompt_ledger,
                    &route_binding,
                    delegation_injection.as_ref(),
                    &stderr_tail,
                )
                .await
                .map_err(ConnectionAttemptError::from)
            }
        };
        let result = {
            tokio::pin!(connection);
            tokio::select! {
                biased;
                result = &mut connection => result,
                _ = attempt_cancellation.cancelled() => {
                    let snapshot = state.read().await;
                    tracing::info!(
                        connection_id,
                        agent = %agent_type,
                        launch_finalized = snapshot.launch_finalized,
                        session_id = snapshot.external_id.as_deref().unwrap_or(""),
                        "[ACP] connection establishment canceled"
                    );
                    Ok(())
                }
            }
        };
        PermissionRuntime::new(&state, &emitter, &pending_perms)
            .close_and_drain("connection_teardown")
            .await;
        for session_id in _route_lease.session_ids() {
            terminal_cleanup.release_all_for_session(&session_id).await;
        }
        drop(_route_lease);
        if !shared_host {
            host.shutdown().await;
        }
        match result {
            Ok(()) => return Ok(()),
            Err(ConnectionAttemptError::Protocol(error)) => {
                return Err(AcpError::protocol(safe_error_detail(&error.to_string())));
            }
            Err(ConnectionAttemptError::BuiltinMcp(error)) => return Err(error),
        }
    }
}

/// Store the permission responder and emit event to frontend.
pub(crate) async fn handle_permission_request(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    perms: &PendingPermissions,
    cwd: &str,
    req: RequestPermissionRequest,
    responder: Responder<RequestPermissionResponse>,
) {
    let request_id = uuid::Uuid::new_v4().to_string();
    let session_id = req.session_id.0.to_string();

    let options: Vec<PermissionOptionInfo> = req
        .options
        .iter()
        .map(|opt| PermissionOptionInfo {
            option_id: opt.option_id.to_string(),
            name: opt.name.clone(),
            kind: match opt.kind {
                PermissionOptionKind::AllowOnce => "allow_once".into(),
                PermissionOptionKind::AllowAlways => "allow_always".into(),
                PermissionOptionKind::RejectOnce => "reject_once".into(),
                PermissionOptionKind::RejectAlways => "reject_always".into(),
                _ => "unknown".into(),
            },
        })
        .collect();

    let mut tool_call_value = serde_json::to_value(&req.tool_call).unwrap_or_default();

    // Resolve line numbers in rawInput for edit tool permission requests
    if let Some(obj) = tool_call_value.as_object_mut() {
        let key = ["rawInput", "raw_input"]
            .into_iter()
            .find(|k| obj.contains_key(*k));
        if let Some(key) = key {
            match obj.get_mut(key) {
                // rawInput is a JSON object: inject _start_line in place
                Some(v) if v.is_object() => {
                    inject_start_line(v, Some(cwd));
                }
                // rawInput is a JSON string: parse, inject, write back as object
                Some(serde_json::Value::String(text)) => {
                    let text = text.clone();
                    if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                        if inject_start_line(&mut parsed, Some(cwd)) {
                            obj.insert(key.to_string(), parsed);
                        }
                    } else if text.contains("@@\n") || text.contains("@@\r\n") {
                        if let Some(resolved) = crate::parsers::resolve_patch_text(&text, Some(cwd))
                        {
                            obj.insert(key.to_string(), serde_json::Value::String(resolved));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let tool_call_id = permission_tool_call_id(&tool_call_value).to_string();
    let option_count = options.len();
    let allow_option_count = options
        .iter()
        .filter(|option| option.kind.starts_with("allow_"))
        .count();
    PermissionRuntime::new(state, emitter, perms)
        .admit(
            responder,
            QueuedPermission {
                request_id,
                tool_call: tool_call_value,
                options,
            },
            PermissionRequestMeta {
                session_id: &session_id,
                tool_call_id: &tool_call_id,
                option_count,
                allow_option_count,
            },
        )
        .await;
}

fn permission_tool_call_id(value: &serde_json::Value) -> &str {
    value
        .get("toolCallId")
        .or_else(|| value.get("tool_call_id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

async fn respond_permission_request(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    perms: &PendingPermissions,
    request_id: String,
    option_id: String,
) {
    PermissionRuntime::new(state, emitter, perms)
        .resolve(request_id, option_id)
        .await;
}

async fn set_session_mode(
    session: &mut sacp::ActiveSession<'_, Agent>,
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    mode_id: String,
) -> Result<(), sacp::Error> {
    let req = SetSessionModeRequest::new(session.session_id().clone(), mode_id.clone());
    session
        .connection()
        .send_request_to(Agent, req)
        .block_task()
        .await?;

    emit_with_state(state, emitter, AcpEvent::ModeChanged { mode_id }).await;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn set_session_config_option(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    agent_type: AgentType,
    config_id: String,
    value_id: String,
) -> Result<(), sacp::Error> {
    if agent_type == AgentType::Grok {
        return crate::acp::grok::set_config_option(
            cx, session_id, state, emitter, config_id, value_id,
        )
        .await;
    }
    let updated = set_session_config_option_inner(cx, session_id, config_id, value_id).await?;
    emit_session_config_options_values(state, emitter, agent_type, updated).await;
    Ok(())
}

/// Wire-level half of `set_session_config_option`: send the JSON-RPC request and
/// return the agent's new config-options list, without touching SessionState or
/// emitting events. Used at session-init to apply saved preferences before the
/// single emit_session_config_options call so the frontend never sees an
/// "agent default → user preference" flicker.
async fn set_session_config_option_inner(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    config_id: String,
    value_id: String,
) -> Result<Vec<SessionConfigOption>, sacp::Error> {
    let req = SetSessionConfigOptionRequest::new(session_id.clone(), config_id, value_id);
    let untyped_req = UntypedMessage::new("session/set_config_option", req).map_err(|e| {
        sacp::util::internal_error(format!("Failed to build config option request: {e}"))
    })?;

    let raw_response = cx.send_request_to(Agent, untyped_req).block_task().await?;
    let response: SetSessionConfigOptionResponse =
        serde_json::from_value(raw_response).map_err(|e| {
            sacp::util::internal_error(format!("Failed to parse config option response: {e}"))
        })?;

    Ok(response.config_options)
}

/// Apply config-option preferences to a freshly-attached session before the
/// initial `session_config_options` event is emitted to the frontend. Mode
/// preferences are applied separately by [`apply_preferred_session_mode`].
///
/// The frontend ships saved config values to the backend on connect; this
/// function applies them so the first snapshot already reflects the effective
/// values without a client-side rewrite and sync-back cycle.
///
/// Returns the (possibly updated) list of config options that the caller
/// should emit. Failures on individual preferences are logged and skipped so
/// a stale or invalid preference cannot block session startup.
#[allow(clippy::too_many_arguments)]
async fn apply_preferred_session_config_options(
    cx: &ConnectionTo<Agent>,
    session: &mut sacp::ActiveSession<'_, Agent>,
    preferred_config_values: &BTreeMap<String, String>,
    initial_config_options: Vec<SessionConfigOption>,
) -> Vec<SessionConfigOption> {
    if preferred_config_values.is_empty() {
        return initial_config_options;
    }

    let session_id = session.session_id().clone();
    let mut options = initial_config_options;
    let preferences = preferred_config_values
        .get_key_value("model")
        .into_iter()
        .chain(
            preferred_config_values
                .iter()
                .filter(|(config_id, _)| config_id.as_str() != "model"),
        );
    for (config_id, value_id) in preferences {
        let Some((resolved_config_id, resolved_value_id)) =
            resolve_preferred_session_config(&options, config_id, value_id)
        else {
            tracing::debug!(
                "[ACP] skipping unsupported preferred config '{config_id}'='{value_id}'"
            );
            continue;
        };
        // Skip the round-trip when the resolved live value already matches.
        // Standard options must be advertised and validated by the resolver;
        // only the legacy Codex "mode" fallback may remain unadvertised.
        let already_matches = options.iter().any(|o| {
            o.id.to_string() == resolved_config_id
                && matches!(
                    &o.kind,
                    SessionConfigKind::Select(s)
                        if s.current_value.to_string() == resolved_value_id
                )
        });
        if already_matches {
            continue;
        }
        match set_session_config_option_inner(
            cx,
            &session_id,
            resolved_config_id.clone(),
            resolved_value_id.clone(),
        )
        .await
        {
            Ok(updated) => options = updated,
            Err(e) => tracing::error!(
                "[ACP] failed to apply preferred config '{config_id}'='{value_id}' as \
                 '{resolved_config_id}'='{resolved_value_id}' on connect: {e}"
            ),
        }
    }

    options
}

fn validate_requested_session_mode(
    modes: Option<&SessionModeState>,
    agent_type: AgentType,
    requested_mode_id: &str,
    mode_source: &str,
) -> Option<String> {
    let Some(modes) = modes else {
        tracing::debug!(
            agent = ?agent_type,
            mode_id = requested_mode_id,
            mode_source,
            "[ACP] Agent did not advertise session modes; preserving Agent default"
        );
        return None;
    };
    let current_mode_id = modes.current_mode_id.to_string();
    let is_advertised = modes
        .available_modes
        .iter()
        .any(|mode| mode.id.to_string() == requested_mode_id);
    if !is_advertised {
        tracing::warn!(
            agent = ?agent_type,
            mode_id = requested_mode_id,
            mode_source,
            current_mode_id,
            available_mode_count = modes.available_modes.len(),
            "[ACP] skipped unsupported session mode on connect"
        );
        return None;
    }
    Some(current_mode_id)
}

async fn apply_preferred_session_mode(
    session: &mut sacp::ActiveSession<'_, Agent>,
    context: (&Arc<RwLock<SessionState>>, &EventEmitter),
    agent_type: AgentType,
    preferred_mode_id: Option<&str>,
) {
    let (requested_mode_id, mode_source) = match preferred_mode_id {
        Some(mode_id) => (mode_id, "user_or_caller"),
        None => (automatic_mode_id(agent_type), "product_default"),
    };
    let Some(current_mode_id) = validate_requested_session_mode(
        session.modes().as_ref(),
        agent_type,
        requested_mode_id,
        mode_source,
    ) else {
        return;
    };
    tracing::info!(
        agent = ?agent_type,
        mode_id = requested_mode_id,
        mode_source,
        "[ACP] resolved session mode on connect"
    );
    let (state, emitter) = context;
    if current_mode_id != requested_mode_id {
        if let Err(error) =
            set_session_mode(session, state, emitter, requested_mode_id.to_string()).await
        {
            tracing::error!(
                agent = ?agent_type,
                mode_id = requested_mode_id,
                mode_source,
                error = %error,
                "[ACP] failed to apply resolved session mode on connect"
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn apply_and_emit_session_config_options(
    cx: &ConnectionTo<Agent>,
    session: &mut sacp::ActiveSession<'_, Agent>,
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    agent_type: AgentType,
    grok_meta: Option<&serde_json::Map<String, serde_json::Value>>,
    grok_effort_specs: Option<&crate::acp::grok::EffortSpecs>,
    preferred_mode_id: Option<&str>,
    preferred_config_values: &BTreeMap<String, String>,
    initial_config_options: Vec<SessionConfigOption>,
) {
    apply_preferred_session_mode(session, (state, emitter), agent_type, preferred_mode_id).await;
    if agent_type == AgentType::Grok {
        let specs = grok_effort_specs.cloned().unwrap_or_default();
        let mut options =
            crate::acp::grok::synthesize_options(grok_meta, &specs).unwrap_or_default();
        state.write().await.grok_effort_specs = (!specs.is_empty()).then_some(specs.clone());
        crate::acp::grok::apply_preferred_options(
            cx,
            session.session_id(),
            &mut options,
            preferred_config_values,
            &specs,
        )
        .await;
        emit_session_config_options_info(state, emitter, options).await;
        return;
    }
    let updated = apply_preferred_session_config_options(
        cx,
        session,
        preferred_config_values,
        initial_config_options,
    )
    .await;
    emit_session_config_options_values(state, emitter, agent_type, updated).await;
}

const TERMINAL_POLL_INTERVAL_MS: u64 = 200;
const TERMINAL_POLL_MISSING_LIMIT: u8 = 10;

/// Hard cap on the size of a single ACP event's `raw_output` payload.
///
/// Agents (e.g. Claude Code, Codex) frequently send `tool_call_update`
/// notifications where `raw_output` is the **full accumulated** tool output
/// rather than an incremental delta. For long-running terminal tools this
/// leads to O(N²) bytes flowing through the event pipeline and multi-GB
/// transient allocations (serde_json Value trees, IPC buffers, broadcast
/// channel backlog). This constant caps any single emitted chunk so the
/// pipeline never sees a multi-MB event.
const MAX_SINGLE_EMIT_BYTES: usize = 64 * 1024;

/// Byte length of the tail we retain per tool-call to verify that the next
/// incoming snapshot is a cumulative extension of the previous one. Small
/// enough to keep the cache bounded even in pathological sessions, large
/// enough that a matching tail is an extremely unlikely coincidence.
const MAX_CACHED_TAIL_BYTES: usize = 8 * 1024;

/// Hard cap on the number of tool-call entries the cache retains. Prevents
/// unbounded growth in long sessions where agents forget to mark tool calls
/// as completed. Entries are evicted FIFO by generation counter.
const MAX_CACHE_ENTRIES: usize = 256;

/// Prefix used when an emitted chunk had to be truncated.
const TRUNCATION_MARKER: &str = "[...truncated...]\n";

#[derive(Debug)]
struct CachedOutput {
    /// Total byte length of the last observed `raw_output`.
    total_len: usize,
    /// Tail of the last observed `raw_output`, up to `MAX_CACHED_TAIL_BYTES`
    /// bytes. Always aligned to a UTF-8 character boundary at the start.
    tail: String,
    /// Monotonic insertion/update tick used for FIFO eviction.
    generation: u64,
}

/// Per-session cache of the last `raw_output` fingerprint emitted for each
/// tool call. Enables delta detection: when an agent sends cumulative
/// snapshots, we forward only the suffix (with `raw_output_append=true`)
/// and keep the fingerprint bounded so it works even when the full output
/// grows into the multi-MB range.
#[derive(Debug, Default)]
struct ToolCallOutputCache {
    entries: HashMap<String, CachedOutput>,
    next_generation: u64,
}

impl ToolCallOutputCache {
    /// Diff an incoming full `raw_output` snapshot for `tool_call_id` against
    /// the cache and return what should be emitted downstream.
    ///
    /// Returns `None` when the incoming snapshot is identical to the
    /// previously emitted one (nothing to send). Otherwise returns
    /// `(payload, append)` where:
    /// - `append=true` — `payload` is a (possibly truncated) suffix delta;
    ///   the frontend should append it to the existing chunks.
    /// - `append=false` — `payload` is a (possibly truncated) replacement
    ///   for the full tool output; the frontend should reset chunks.
    fn consume(&mut self, tool_call_id: &str, curr: &str) -> Option<(String, bool)> {
        let curr_len = curr.len();

        let decision: Option<(String, bool)> = match self.entries.get(tool_call_id) {
            Some(prev) if curr_len >= prev.total_len && self.is_extension_of(prev, curr) => {
                if curr_len == prev.total_len {
                    // Identical output — nothing to emit. Cache stays fresh.
                    return None;
                }
                let suffix = &curr[prev.total_len..];
                Some(build_emit_payload(suffix, true))
            }
            _ => Some(build_emit_payload(curr, false)),
        };

        // Update cache snapshot to current state so the next update can
        // still detect a prefix extension.
        let tail =
            trim_partial_ansi_tail(truncate_tail_at_char_boundary(curr, MAX_CACHED_TAIL_BYTES))
                .to_string();
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        self.entries.insert(
            tool_call_id.to_string(),
            CachedOutput {
                total_len: curr_len,
                tail,
                generation,
            },
        );
        self.enforce_entry_cap();
        decision
    }

    /// Seed the cache with an initial snapshot for `tool_call_id`, WITHOUT
    /// attempting to diff against any prior state. Used for the initial
    /// `SessionUpdate::ToolCall` notification, whose frontend reducer
    /// treats `raw_output` as a full replacement.
    fn seed(&mut self, tool_call_id: &str, curr: &str) -> Option<String> {
        let (payload, _append) = build_emit_payload(curr, false);
        let tail =
            trim_partial_ansi_tail(truncate_tail_at_char_boundary(curr, MAX_CACHED_TAIL_BYTES))
                .to_string();
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        self.entries.insert(
            tool_call_id.to_string(),
            CachedOutput {
                total_len: curr.len(),
                tail,
                generation,
            },
        );
        self.enforce_entry_cap();
        if payload.is_empty() {
            None
        } else {
            Some(payload)
        }
    }

    /// Drop cached state for a tool call that has finished. Keeps the
    /// session-scoped cache bounded in long-running sessions.
    fn remove_if_final(&mut self, tool_call_id: &str, status: Option<&str>) {
        if matches!(status, Some("completed" | "failed" | "cancelled" | "error")) {
            self.entries.remove(tool_call_id);
        }
    }

    /// Returns true when the cached fingerprint matches `curr` at the
    /// expected offset — i.e. `curr` is a prefix extension (or identity)
    /// of the previously observed snapshot.
    fn is_extension_of(&self, prev: &CachedOutput, curr: &str) -> bool {
        let tail_start = prev.total_len.saturating_sub(prev.tail.len());
        curr.get(tail_start..prev.total_len)
            .is_some_and(|slice| slice == prev.tail.as_str())
    }

    /// Evict oldest entries (by `generation`) once the cache exceeds the
    /// entry cap. Linear scan over a bounded map, so O(MAX_CACHE_ENTRIES)
    /// per eviction — acceptable at this size.
    fn enforce_entry_cap(&mut self) {
        while self.entries.len() > MAX_CACHE_ENTRIES {
            let Some(oldest_id) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.generation)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.entries.remove(&oldest_id);
        }
    }
}

/// Apply the per-event size cap + truncation marker. Returns `(payload,
/// append)`. An empty `text` yields an empty `payload`; callers should
/// decide whether to suppress the emission in that case.
fn build_emit_payload(text: &str, append: bool) -> (String, bool) {
    let truncated =
        trim_partial_ansi_tail(truncate_tail_at_char_boundary(text, MAX_SINGLE_EMIT_BYTES));
    let out = if truncated.len() < text.len() {
        format!("{TRUNCATION_MARKER}{truncated}")
    } else {
        truncated.to_string()
    };
    (out, append)
}

/// Return a substring of `s` whose byte length is `<= max_bytes`, aligned to
/// a UTF-8 character boundary and taken from the TAIL of `s` (so the most
/// recent output is preserved when truncation is required).
fn truncate_tail_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

/// If the very end of `s` contains a partial ANSI escape sequence, trim it
/// so downstream ANSI parsers (e.g. the frontend `ansi-to-react` renderer)
/// don't see a half-emitted escape.
///
/// Handles the three common ACP-stream cases:
/// - CSI (`ESC [ ... final`): terminator is a byte in 0x40..=0x7E after
///   the `[` introducer.
/// - OSC (`ESC ] ... ST|BEL`): terminator is BEL (0x07) or `ESC \`.
/// - Simple two-byte escape (`ESC <byte>`): complete as soon as the byte
///   following ESC is present.
///
/// ESC is ASCII (1 byte), always a valid UTF-8 char boundary, so slicing
/// at `esc_pos` cannot produce an invalid UTF-8 string.
fn trim_partial_ansi_tail(s: &str) -> &str {
    let bytes = s.as_bytes();
    let Some(esc_pos) = bytes.iter().rposition(|&b| b == 0x1B) else {
        return s;
    };
    let after = &bytes[esc_pos + 1..];
    if after.is_empty() {
        return &s[..esc_pos];
    }
    let terminated = match after[0] {
        b'[' => after[1..].iter().any(|&b| (0x40..=0x7E).contains(&b)),
        b']' => {
            after[1..].contains(&0x07)
                || after[1..].windows(2).any(|w| w[0] == 0x1B && w[1] == b'\\')
        }
        // Two-byte escape sequences (ESC M, ESC D, …) are complete as
        // soon as the second byte is present.
        _ => true,
    };
    if terminated {
        s
    } else {
        &s[..esc_pos]
    }
}

#[derive(Debug, Default)]
struct TrackedTerminalToolCall {
    terminal_ids: Vec<String>,
    status: Option<String>,
    terminal_offsets: HashMap<String, u64>,
    terminal_exit_reported: HashSet<String>,
    has_emitted_output: bool,
    missing_polls: u8,
}

#[derive(Debug, Default)]
struct TerminalPollResult {
    output: Option<String>,
    append: bool,
    any_found: bool,
    all_exited: bool,
}

fn is_final_tool_call_status(status: Option<&str>) -> bool {
    matches!(status, Some("completed" | "failed"))
}

fn merge_terminal_ids(existing: &mut Vec<String>, incoming: Vec<String>) -> bool {
    let mut changed = false;
    for terminal_id in incoming {
        if !existing.iter().any(|id| id == &terminal_id) {
            existing.push(terminal_id);
            changed = true;
        }
    }
    changed
}

fn extract_terminal_ids(content: &[ToolCallContent]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terminal_ids = Vec::new();
    for item in content {
        if let ToolCallContent::Terminal(terminal) = item {
            let terminal_id = terminal.terminal_id.to_string();
            if seen.insert(terminal_id.clone()) {
                terminal_ids.push(terminal_id);
            }
        }
    }
    terminal_ids
}

fn track_terminal_tool_calls(
    update: &SessionUpdate,
    tracked: &mut HashMap<String, TrackedTerminalToolCall>,
) -> bool {
    match update {
        SessionUpdate::ToolCall(tc) => {
            let terminal_ids = extract_terminal_ids(&tc.content);
            if terminal_ids.is_empty() {
                return false;
            }

            let status = format!("{:?}", tc.status).to_lowercase();
            let entry = tracked.entry(tc.tool_call_id.to_string()).or_default();
            let changed = merge_terminal_ids(&mut entry.terminal_ids, terminal_ids);
            entry.status = Some(status);
            changed
        }
        SessionUpdate::ToolCallUpdate(tcu) => {
            let mut changed = false;
            let mut should_track = false;

            let terminal_ids = tcu
                .fields
                .content
                .as_ref()
                .map(|content| extract_terminal_ids(content))
                .unwrap_or_default();
            if !terminal_ids.is_empty() {
                should_track = true;
            }

            if tracked.contains_key(&tcu.tool_call_id.to_string()) {
                should_track = true;
            }

            if !should_track {
                return false;
            }

            let entry = tracked.entry(tcu.tool_call_id.to_string()).or_default();
            if !terminal_ids.is_empty() {
                changed = merge_terminal_ids(&mut entry.terminal_ids, terminal_ids);
            }

            if let Some(status) = tcu.fields.status {
                let status_str = format!("{:?}", status).to_lowercase();
                if entry.status.as_deref() != Some(status_str.as_str()) {
                    changed = true;
                }
                entry.status = Some(status_str);
            }

            changed
        }
        _ => false,
    }
}

fn format_terminal_exit_status(exit_status: &TerminalExitStatus) -> String {
    let mut parts = Vec::new();
    if let Some(code) = exit_status.exit_code {
        parts.push(format!("exit code: {code}"));
    }
    if let Some(signal) = &exit_status.signal {
        parts.push(format!("signal: {signal}"));
    }
    if parts.is_empty() {
        "finished".to_string()
    } else {
        parts.join(", ")
    }
}

async fn poll_terminal_tool_call_output(
    terminal_runtime: &TerminalRuntime,
    session_id: &SessionId,
    tracked: &mut TrackedTerminalToolCall,
) -> Result<TerminalPollResult, TerminalRuntimeError> {
    let mut chunks: Vec<String> = Vec::new();
    let mut any_found = false;
    let mut all_exited = true;
    let include_headers = tracked.terminal_ids.len() > 1;

    for terminal_id in &tracked.terminal_ids {
        let from_offset = tracked.terminal_offsets.get(terminal_id).copied();
        let response = match terminal_runtime
            .terminal_output_delta(session_id.0.as_ref(), terminal_id, from_offset)
            .await
        {
            Ok(response) => response,
            Err(TerminalRuntimeError::InvalidParams(_)) => continue,
            Err(err) => return Err(err),
        };

        any_found = true;
        tracked
            .terminal_offsets
            .insert(terminal_id.clone(), response.next_offset);

        if response.exit_status.is_none() {
            all_exited = false;
        }

        let mut chunk = String::new();
        if include_headers {
            chunk.push_str(&format!("[Terminal: {terminal_id}]\n"));
        }

        if response.had_gap {
            chunk.push_str("[output truncated]\n");
        }

        if !response.output.is_empty() {
            chunk.push_str(&response.output);
            if !chunk.ends_with('\n') {
                chunk.push('\n');
            }
        }

        if response.truncated && from_offset.is_none() {
            chunk.push_str("[output truncated]\n");
        }

        if let Some(exit_status) = response.exit_status {
            if tracked.terminal_exit_reported.insert(terminal_id.clone()) {
                chunk.push_str(&format!(
                    "[terminal exited: {}]\n",
                    format_terminal_exit_status(&exit_status)
                ));
            }
        }

        if chunk.ends_with('\n') {
            chunk.pop();
        }
        if !chunk.is_empty() {
            chunks.push(chunk);
        }
    }

    if !any_found {
        all_exited = false;
    }

    let append = tracked.has_emitted_output;
    if !chunks.is_empty() {
        tracked.has_emitted_output = true;
    }

    Ok(TerminalPollResult {
        output: if chunks.is_empty() {
            None
        } else {
            Some(chunks.join("\n\n"))
        },
        append,
        any_found,
        all_exited,
    })
}

async fn emit_terminal_output_update(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    tool_call_id: &str,
    output: String,
    append: bool,
) {
    // Safety cap: when a subprocess writes very fast between poll ticks,
    // the delta produced by `poll_terminal_tool_call_output` can still be
    // up to ~1 MB (the terminal buffer limit). Enforce the pipeline-wide
    // single-event cap (with ANSI-safe truncation) before emission so the
    // WS/IPC fanout never carries a multi-MB payload.
    let (payload, _append) = build_emit_payload(&output, append);
    emit_with_state(
        state,
        emitter,
        AcpEvent::ToolCallUpdate {
            tool_call_id: tool_call_id.to_string(),
            title: None,
            status: None,
            content: None,
            raw_input: None,
            raw_output: Some(payload),
            raw_output_append: Some(append),
            locations: None,
            meta: None,
            images: None,
        },
    )
    .await;
}

async fn poll_tracked_terminal_tool_calls(
    terminal_runtime: &TerminalRuntime,
    session_id: &SessionId,
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    tracked: &mut HashMap<String, TrackedTerminalToolCall>,
) {
    if tracked.is_empty() {
        return;
    }

    let tool_call_ids: Vec<String> = tracked.keys().cloned().collect();
    let mut remove_ids: Vec<String> = Vec::new();

    for tool_call_id in tool_call_ids {
        let Some(entry) = tracked.get_mut(&tool_call_id) else {
            continue;
        };
        if entry.terminal_ids.is_empty() {
            remove_ids.push(tool_call_id.clone());
            continue;
        }

        let poll_result =
            match poll_terminal_tool_call_output(terminal_runtime, session_id, entry).await {
                Ok(result) => result,
                Err(err) => {
                    tracing::error!(
                        "[ACP] Failed to poll terminal output for tool call {}: {:?}",
                        tool_call_id,
                        err
                    );
                    continue;
                }
            };

        if poll_result.any_found {
            entry.missing_polls = 0;
        } else {
            entry.missing_polls = entry.missing_polls.saturating_add(1);
        }

        if let Some(output) = poll_result.output {
            emit_terminal_output_update(state, emitter, &tool_call_id, output, poll_result.append)
                .await;
        }

        if (is_final_tool_call_status(entry.status.as_deref())
            && (!poll_result.any_found || poll_result.all_exited))
            || entry.missing_polls >= TERMINAL_POLL_MISSING_LIMIT
        {
            remove_ids.push(tool_call_id.clone());
        }
    }

    for tool_call_id in remove_ids {
        tracked.remove(&tool_call_id);
    }
}

fn inline_code(text: &str) -> String {
    let mut longest = 0;
    let mut current = 0;
    for ch in text.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    let fence = "`".repeat(longest + 1);
    let needs_padding = text
        .chars()
        .next()
        .is_some_and(|ch| ch == '`' || ch.is_whitespace())
        || text
            .chars()
            .next_back()
            .is_some_and(|ch| ch == '`' || ch.is_whitespace());
    let padding = if needs_padding { " " } else { "" };
    format!("{fence}{padding}{text}{padding}{fence}")
}

fn map_image_block(
    agent_type: AgentType,
    data: String,
    mime_type: String,
    uri: Option<String>,
    local_path: Option<String>,
) -> Result<ContentBlock, String> {
    if agent_type != AgentType::Codex {
        return Ok(ContentBlock::Image(
            ImageContent::new(data, mime_type).uri(uri),
        ));
    }
    let path = local_path
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Codex image attachment is missing its prepared local file".to_string())?;
    if !Path::new(&path).is_absolute() {
        return Err("Codex image attachment local file must be absolute".to_string());
    }
    let path = path.replace('\r', " ").replace('\n', " ");
    let text = format!("{LOCAL_FILE_PROMPT_PREFIX}{}", inline_code(&path));
    Ok(ContentBlock::Text(TextContent::new(text)))
}

fn map_prompt_blocks(
    agent_type: AgentType,
    blocks: Vec<PromptInputBlock>,
) -> Result<Vec<ContentBlock>, String> {
    let mut budget = PromptResourceBudget::default();
    let mapped = blocks
        .into_iter()
        .map(|block| map_prompt_block(agent_type, block, &mut budget))
        .collect::<Result<Vec<_>, _>>()?;
    if budget.bounded_count > 0 {
        tracing::warn!(
            bounded_count = budget.bounded_count,
            original_bytes = budget.original_bytes,
            final_bytes = budget.final_bytes,
            "[ACP] bounded inline prompt resources"
        );
    }
    Ok(mapped)
}

fn map_prompt_block(
    agent_type: AgentType,
    block: PromptInputBlock,
    budget: &mut PromptResourceBudget,
) -> Result<ContentBlock, String> {
    match block {
        PromptInputBlock::Text { text } => Ok(ContentBlock::Text(TextContent::new(text))),
        PromptInputBlock::Image {
            data,
            mime_type,
            uri,
            local_path,
        } => map_image_block(agent_type, data, mime_type, uri, local_path),
        PromptInputBlock::Resource {
            uri,
            mime_type,
            text,
            blob,
        } => map_resource_block(uri, mime_type, text, blob, budget),
        PromptInputBlock::ResourceLink {
            uri,
            name,
            mime_type,
            description,
        } => {
            let mut link = ResourceLink::new(name, uri);
            link.mime_type = mime_type;
            link.description = description;
            Ok(ContentBlock::ResourceLink(link))
        }
    }
}

#[derive(Default)]
struct PromptResourceBudget {
    bounded_count: usize,
    original_bytes: usize,
    final_bytes: usize,
}

fn map_resource_block(
    uri: String,
    mime_type: Option<String>,
    text: Option<String>,
    blob: Option<String>,
    budget: &mut PromptResourceBudget,
) -> Result<ContentBlock, String> {
    let metadata_bytes = uri
        .len()
        .saturating_add(mime_type.as_ref().map_or(0, String::len));
    if metadata_bytes >= MAX_INLINE_RESOURCE_WIRE_BYTES {
        return Err("ACP inline resource metadata exceeds the wire budget".to_string());
    }
    let raw_value_bytes = text
        .as_ref()
        .map_or_else(|| blob.as_ref().map_or(0, String::len), String::len);
    let valid = text.is_some() || blob.as_deref().is_none_or(valid_inline_blob);
    let original_bytes = metadata_bytes.saturating_add(raw_value_bytes);
    if valid && raw_value_bytes < MAX_INLINE_RESOURCE_WIRE_BYTES {
        let original = embedded_resource(uri.clone(), mime_type.clone(), text, blob);
        let wire_bytes = resource_wire_bytes(&original)?;
        budget.original_bytes = budget.original_bytes.saturating_add(wire_bytes);
        if wire_bytes <= MAX_INLINE_RESOURCE_WIRE_BYTES
            && budget.final_bytes.saturating_add(wire_bytes) <= MAX_PROMPT_RESOURCE_BYTES
        {
            budget.final_bytes = budget.final_bytes.saturating_add(wire_bytes);
            return Ok(original);
        }
    } else {
        budget.original_bytes = budget.original_bytes.saturating_add(original_bytes);
    }
    let placeholder = resource_placeholder(&uri, mime_type.as_deref(), raw_value_bytes);
    let bounded = embedded_resource(uri, mime_type, Some(placeholder), None);
    let wire_bytes = resource_wire_bytes(&bounded)?;
    if wire_bytes > MAX_INLINE_RESOURCE_WIRE_BYTES
        || budget.final_bytes.saturating_add(wire_bytes) > MAX_PROMPT_RESOURCE_BYTES
    {
        return Err("ACP inline resource prompt budget is exhausted".to_string());
    }
    budget.bounded_count += 1;
    budget.final_bytes = budget.final_bytes.saturating_add(wire_bytes);
    Ok(bounded)
}

fn embedded_resource(
    uri: String,
    mime_type: Option<String>,
    text: Option<String>,
    blob: Option<String>,
) -> ContentBlock {
    let resource = match (text, blob) {
        (Some(value), _) => EmbeddedResourceResource::TextResourceContents(
            TextResourceContents::new(value, uri).mime_type(mime_type),
        ),
        (None, Some(value)) => EmbeddedResourceResource::BlobResourceContents(
            BlobResourceContents::new(value, uri).mime_type(mime_type),
        ),
        (None, None) => EmbeddedResourceResource::TextResourceContents(
            TextResourceContents::new("", uri).mime_type(mime_type),
        ),
    };
    ContentBlock::Resource(EmbeddedResource::new(resource))
}

fn resource_wire_bytes(block: &ContentBlock) -> Result<usize, String> {
    serde_json::to_vec(block)
        .map(|encoded| encoded.len().saturating_add(1))
        .map_err(|error| format!("cannot measure ACP prompt resource: {error}"))
}

fn valid_inline_blob(value: &str) -> bool {
    value.len() < MAX_INLINE_RESOURCE_WIRE_BYTES && STANDARD.decode(value.as_bytes()).is_ok()
}

fn resource_placeholder(uri: &str, mime_type: Option<&str>, bytes: usize) -> String {
    format!(
        "[resource omitted: uri={}, mime_type={}, bytes={}]",
        truncate_utf8(uri, MAX_RESOURCE_URI_DISPLAY_BYTES),
        mime_type.unwrap_or("unknown"),
        bytes
    )
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &value[..end])
}

fn is_codex_compaction_prompt(agent_type: AgentType, blocks: &[PromptInputBlock]) -> bool {
    if agent_type != AgentType::Codex || blocks.len() != 1 {
        return false;
    }
    let PromptInputBlock::Text { text } = &blocks[0] else {
        return false;
    };
    let command = text.trim();
    command == "/compact" || command.starts_with("/compact ")
}

async fn verified_compaction_stop_reason(
    raw_reason: &'static str,
    marker: Option<&Result<crate::acp::codex_rollout_migration::CompactionMarker, String>>,
    session_id: &str,
) -> &'static str {
    let Some(marker) = marker else {
        return raw_reason;
    };
    if raw_reason != "end_turn" {
        return raw_reason;
    }
    let marker = match marker {
        Ok(marker) => marker,
        Err(error) => {
            tracing::warn!(
                session_id,
                error = %error,
                "[ACP] unable to record Codex compaction marker"
            );
            return "compaction_not_applied";
        }
    };
    match crate::acp::codex_rollout_migration::wait_for_new_compaction(marker).await {
        Ok(true) => "end_turn",
        Ok(false) => {
            tracing::warn!(
                session_id,
                "[ACP] Codex compact turn ended without a new compacted rollout record"
            );
            "compaction_not_applied"
        }
        Err(error) => {
            tracing::warn!(
                session_id,
                error = %error,
                "[ACP] failed to verify Codex compaction result"
            );
            "compaction_not_applied"
        }
    }
}

/// Result when the conversation loop exits due to a fork request.
struct ForkExitInfo {
    fork_response: sacp::schema::ForkSessionResponse,
    original_session_id: String,
    reply: tokio::sync::oneshot::Sender<Result<crate::acp::types::ForkProtocolResult, AcpError>>,
    connection: ConnectionTo<Agent>,
}

/// After `run_conversation_loop` returns, handle normal exit or fork transition.
///
/// When fork is requested, the original session has already been dropped by the
/// caller.  We attach to the forked session (S2) directly using the
/// `ForkSessionResponse` — no separate `session/load` is needed because S2 was
/// just created in-memory by the agent on this connection.
#[allow(clippy::too_many_arguments)]
async fn handle_fork_or_exit(
    loop_result: Result<Option<ForkExitInfo>, sacp::Error>,
    conn_id: &str,
    emitter: &EventEmitter,
    state: &Arc<RwLock<SessionState>>,
    agent_type: AgentType,
    perms: &PendingPermissions,
    cmd_rx: &mut mpsc::Receiver<ConnectionCommand>,
    terminal_runtime: Arc<TerminalRuntime>,
    _cwd: &std::path::Path,
    cwd_string: &str,
    prompt_ledger: &background_watch::PromptLedger,
    route_binding: &crate::acp::runtime_host::RuntimeHostRouteBinding,
    // Threaded through from run_connection so the forked session's
    // run_conversation_loop call has the same delegation cascade
    // capability as the original.
    delegation_injection: Option<&DelegationInjection>,
    stderr_tail: &Arc<StderrTail>,
) -> Result<(), sacp::Error> {
    let fork_info = match loop_result {
        Ok(Some(info)) => info,
        Ok(None) => return Ok(()),
        Err(e) => return Err(e),
    };

    let cx = fork_info.connection;
    let fork_resp = fork_info.fork_response;
    let new_sid = fork_resp.session_id.0.to_string();
    if !route_binding.bind_session(new_sid.clone()) {
        return Err(sacp::util::internal_error(
            "ACP runtime route expired before forked session binding",
        ));
    }

    tracing::info!(
        "[ACP] Fork transition: attaching to forked session {} (original: {})",
        new_sid,
        fork_info.original_session_id
    );

    // Reply protocol-level result to manager.fork_session, which will combine
    // it with the freshly-created sibling row id to produce the wire ForkResultInfo.
    let _ = fork_info
        .reply
        .send(Ok(crate::acp::types::ForkProtocolResult {
            forked_session_id: new_sid.clone(),
            original_session_id: fork_info.original_session_id,
        }));

    // Build a NewSessionResponse from the ForkSessionResponse so we can
    // attach directly — the forked session is already live on this process.
    let initial_config_options = fork_resp.config_options.clone();
    let new_resp = NewSessionResponse::new(fork_resp.session_id)
        .modes(fork_resp.modes)
        .config_options(fork_resp.config_options)
        .meta(fork_resp.meta);
    let mut session = cx.attach_session(new_resp, Default::default())?;

    emit_with_state(
        state,
        emitter,
        AcpEvent::SessionStarted {
            session_id: new_sid.clone(),
        },
    )
    .await;
    emit_session_modes(state, emitter, session.modes()).await;
    emit_session_config_options_values(
        state,
        emitter,
        agent_type,
        initial_config_options.unwrap_or_default(),
    )
    .await;
    emit_selectors_ready(state, emitter).await;

    let loop_result = run_conversation_loop(
        &mut session,
        conn_id,
        emitter,
        state,
        agent_type,
        perms,
        cmd_rx,
        terminal_runtime.clone(),
        cwd_string,
        true, // fork already succeeded on this process
        prompt_ledger,
        delegation_injection,
        stderr_tail,
    )
    .await;
    terminal_runtime.release_all_for_session(&new_sid).await;
    drop(session);

    // Recursively handle nested forks
    Box::pin(handle_fork_or_exit(
        loop_result,
        conn_id,
        emitter,
        state,
        agent_type,
        perms,
        cmd_rx,
        terminal_runtime,
        _cwd,
        cwd_string,
        prompt_ledger,
        route_binding,
        delegation_injection,
        stderr_tail,
    ))
    .await
}

/// Main conversation command loop: wait for frontend commands and process them.
///
/// Map ACP `StopReason` to a stable lowercase string carried in the
/// `TurnComplete` event. Covers all 5 spec variants so non-success reasons
/// (`Refusal`/`MaxTokens`/`MaxTurnRequests`) keep their semantics instead of
/// collapsing to `"unknown"` — the lifecycle subscriber and frontend rely on
/// this distinction. The wildcard arm exists because the upstream enum is
/// `#[non_exhaustive]`.
fn stop_reason_to_str(reason: StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::Cancelled => "cancelled",
        StopReason::Refusal => "refusal",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        _ => "unknown",
    }
}

#[derive(Default)]
struct PromptInputStats {
    block_count: usize,
    text_blocks: usize,
    image_blocks: usize,
    resource_blocks: usize,
    resource_link_blocks: usize,
    payload_bytes: usize,
    context_bytes: usize,
}

struct PromptLogContext<'a> {
    connection_id: &'a str,
    session_id: String,
    agent_type: AgentType,
    is_compaction: bool,
    stats: PromptInputStats,
    started_at: std::time::Instant,
}

#[derive(Default)]
struct ToolCallFailureStats {
    pending: usize,
    in_progress: usize,
    completed: usize,
    failed: usize,
    permission_pending: bool,
}

impl PromptLogContext<'_> {
    fn started(&self) {
        tracing::info!(
            connection_id = self.connection_id,
            session_id = %self.session_id,
            agent_type = %self.agent_type,
            input_block_count = self.stats.block_count,
            text_block_count = self.stats.text_blocks,
            image_block_count = self.stats.image_blocks,
            resource_block_count = self.stats.resource_blocks,
            resource_link_block_count = self.stats.resource_link_blocks,
            payload_bytes = self.stats.payload_bytes,
            injected_context_bytes = self.stats.context_bytes,
            is_compaction = self.is_compaction,
            "[ACP] prompt started"
        );
    }

    fn completed(&self, stop_reason: &str, had_output: bool, tool_call_count: usize) {
        tracing::info!(
            connection_id = self.connection_id,
            session_id = %self.session_id,
            agent_type = %self.agent_type,
            stop_reason,
            elapsed_ms = self.started_at.elapsed().as_millis(),
            had_output,
            tool_call_count,
            is_compaction = self.is_compaction,
            "[ACP] prompt completed"
        );
    }

    fn failed(
        &self,
        error: &impl std::fmt::Display,
        tool_call_count: usize,
        tool_stats: &ToolCallFailureStats,
    ) {
        let detail = safe_error_detail(&error.to_string());
        tracing::error!(
            connection_id = self.connection_id,
            session_id = %self.session_id,
            agent_type = %self.agent_type,
            error_kind = prompt_error_kind(&detail),
            error = %detail,
            elapsed_ms = self.started_at.elapsed().as_millis(),
            tool_call_count,
            pending_tool_call_count = tool_stats.pending,
            in_progress_tool_call_count = tool_stats.in_progress,
            completed_tool_call_count = tool_stats.completed,
            failed_tool_call_count = tool_stats.failed,
            permission_pending = tool_stats.permission_pending,
            is_compaction = self.is_compaction,
            "[ACP] prompt failed"
        );
    }

    fn interrupted(&self, reason: &str, had_output: bool, tool_call_count: usize) {
        tracing::warn!(
            connection_id = self.connection_id,
            session_id = %self.session_id,
            agent_type = %self.agent_type,
            reason,
            elapsed_ms = self.started_at.elapsed().as_millis(),
            had_output,
            tool_call_count,
            is_compaction = self.is_compaction,
            "[ACP] prompt interrupted"
        );
    }
}

fn tool_call_failure_stats(state: &SessionState) -> ToolCallFailureStats {
    let mut stats = ToolCallFailureStats {
        permission_pending: state.pending_permission.is_some(),
        ..Default::default()
    };
    for tool_call in state.active_tool_calls.values() {
        match tool_call.status {
            ToolCallStatus::Pending => stats.pending += 1,
            ToolCallStatus::InProgress => stats.in_progress += 1,
            ToolCallStatus::Completed => stats.completed += 1,
            ToolCallStatus::Failed => stats.failed += 1,
        }
    }
    stats
}

fn prompt_input_stats(blocks: &[PromptInputBlock]) -> PromptInputStats {
    let mut stats = PromptInputStats {
        block_count: blocks.len(),
        ..Default::default()
    };
    for block in blocks {
        match block {
            PromptInputBlock::Text { text } => {
                stats.text_blocks += 1;
                stats.payload_bytes += text.len();
            }
            PromptInputBlock::Image { data, .. } => {
                stats.image_blocks += 1;
                stats.payload_bytes += data.len();
            }
            PromptInputBlock::Resource { text, blob, .. } => {
                stats.resource_blocks += 1;
                stats.payload_bytes += text.as_ref().map_or(0, String::len);
                stats.payload_bytes += blob.as_ref().map_or(0, String::len);
            }
            PromptInputBlock::ResourceLink { .. } => stats.resource_link_blocks += 1,
        }
    }
    stats
}

fn safe_error_detail(value: &str) -> String {
    const MAX_CHARS: usize = 2_048;
    const SENSITIVE_MARKERS: &[&str] = &[
        "authorization",
        "bearer ",
        "api_key=",
        "api-key=",
        "token=",
        "secret=",
    ];
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    if SENSITIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return "<sensitive error detail redacted>".to_string();
    }
    normalized.chars().take(MAX_CHARS).collect()
}

fn is_prompt_transport_error(error: &sacp::Error) -> bool {
    if !matches!(error.code, sacp::schema::ErrorCode::InternalError) {
        return false;
    }
    error
        .data
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .is_some_and(|detail| {
            (detail.starts_with("response to `session/prompt` never received:")
                || detail.starts_with("failed to send outgoing request `session/prompt"))
        })
}

fn prompt_error_kind(detail: &str) -> &'static str {
    if detail.starts_with("stream disconnected before completion:") {
        "stream_disconnected"
    } else {
        "acp_prompt_request_failed"
    }
}

fn agent_runtime_error_kind(agent_type: AgentType, text: &str) -> Option<&'static str> {
    let text = text.trim();
    (agent_type == AgentType::Codex
        && text.starts_with("stream disconnected before completion:")
        && text.contains("Incomplete response returned"))
    .then_some("codex_stream_incomplete")
}

/// Recognize adapter diagnostics that were incorrectly emitted as agent text.
/// Signatures stay narrow so model-authored warnings and error analysis remain visible.
fn is_agent_runtime_diagnostic(agent_type: AgentType, text: &str) -> bool {
    if agent_type != AgentType::Codex {
        return false;
    }
    let text = text.trim();
    let model_metadata_warning = text.starts_with("Warning: Model metadata for ")
        && text.contains(" not found.")
        && text.contains("Defaulting to fallback metadata;")
        && text.contains("this can degrade performance and cause issues.");
    let skill_budget_warning = text
        .starts_with("Warning: Skill descriptions were shortened to fit the ")
        && text.contains("% skills context budget.")
        && text.contains("Codex can still see every skill, but some descriptions are shorter.")
        && text.contains("Disable unused skills or plugins to leave more room for the rest.");

    model_metadata_warning || skill_budget_warning
}

/// True when a `SessionUpdate` represents actual agent-produced output for
/// the current turn. Used to detect "silent EndTurn" cases where an agent
/// (notably OpenCode) reports the turn ended successfully but never emitted
/// any reply or tool call — in practice this means the model-side request
/// was swallowed and the user would otherwise see a blank conversation
/// transition silently to `PendingReview`. Metadata-only updates
/// (`UserMessageChunk`, `Plan`, `*ModeUpdate`, `ConfigOptionUpdate`,
/// `SessionInfoUpdate`, `AvailableCommandsUpdate`, `UsageUpdate`) do not
/// count.
fn is_agent_output_update(agent_type: AgentType, update: &SessionUpdate) -> bool {
    match update {
        SessionUpdate::AgentMessageChunk(ContentChunk {
            content: ContentBlock::Text(text),
            ..
        }) => !is_agent_runtime_diagnostic(agent_type, &text.text),
        SessionUpdate::AgentMessageChunk(_) => true,
        SessionUpdate::AgentThoughtChunk(_)
        | SessionUpdate::ToolCall(_)
        | SessionUpdate::ToolCallUpdate(_) => true,
        _ => false,
    }
}

/// Build an `AcpEvent::Error` for a non-success stop reason so the user gets a
/// toast instead of a silent transition to `PendingReview`. Returns `None` for
/// `end_turn` (success) and `cancelled` (already user-driven).
///
/// `Refusal` is included because OpenCode (and similar agents) map backend /
/// gateway errors to `Refusal` per the ACP spec gap — see
/// <https://shashikantjagtap.net/openclaw-acp-what-coding-agent-users-need-to-know-about-protocol-gaps/>.
/// `empty` is a synthesized reason emitted by `run_conversation_loop` when
/// the agent reports `EndTurn` without producing any agent output.
fn turn_failure_error_event(
    reason_str: &str,
    agent_type: AgentType,
    empty: Option<&EmptyTurnReport>,
) -> Option<AcpEvent> {
    let (code, message, details) = match reason_str {
        "refusal" => (
            "turn_failed_refusal",
            format!("{agent_type} refused to continue this turn."),
            None,
        ),
        "max_tokens" => (
            "turn_failed_max_tokens",
            format!("{agent_type} reached the maximum token limit for this turn."),
            None,
        ),
        "max_turn_requests" => (
            "turn_failed_max_turn_requests",
            format!("{agent_type} reached the maximum number of allowed requests for this turn."),
            None,
        ),
        "unknown" => (
            "turn_failed_unknown",
            format!("{agent_type} ended the turn with an unrecognized stop reason."),
            None,
        ),
        "empty" => match empty {
            Some(report) => (report.code(), report.message(agent_type), report.details()),
            None => (
                "turn_failed_empty",
                format!("{agent_type} ended the turn without producing any response."),
                None,
            ),
        },
        "compaction_not_applied" => (
            "compaction_not_applied",
            format!("{agent_type} ended the compact turn without writing a new compacted summary."),
            None,
        ),
        _ => return None,
    };
    Some(AcpEvent::Error {
        message,
        agent_type: agent_type.to_string(),
        code: Some(code.to_string()),
        details,
        // Non-terminal: this Error is paired with a `TurnComplete`
        // carrying the same stop reason. The connection stays alive and
        // the broker's pending entry is drained by `complete_call` with
        // the correct child-side mapping (`ChildRefusal` /
        // `ChildMaxTokens` / …). See F1 in the v0.14.3 sub-agent
        // delegation post-mortem.
        terminal: false,
    })
}

async fn wait_for_native_background_adoption(state: &Arc<RwLock<SessionState>>) -> bool {
    loop {
        let wake = {
            let snapshot = state.read().await;
            match snapshot.native_background_turn.as_ref() {
                None => return false,
                Some(turn) if turn.adopted_generation.is_some() => return true,
                Some(_) => snapshot.native_background_notify.clone().notified_owned(),
            }
        };
        wake.await;
    }
}

fn codex_thread_status(update: &SessionUpdate) -> Option<&str> {
    let SessionUpdate::SessionInfoUpdate(info) = update else {
        return None;
    };
    info.meta
        .as_ref()?
        .get("codex")?
        .get("threadStatus")?
        .get("type")?
        .as_str()
}

struct NativeBackgroundFinishContext<'a> {
    state: &'a Arc<RwLock<SessionState>>,
    emitter: &'a EventEmitter,
    permissions: &'a PendingPermissions,
    session_id: &'a str,
    agent_type: AgentType,
}

async fn finish_native_background_turn(
    context: NativeBackgroundFinishContext<'_>,
    thread_status: &str,
) {
    let stop_reason = match thread_status {
        "idle" => "end_turn",
        "systemError" => "unknown",
        _ => return,
    };
    let active = {
        let mut snapshot = context.state.write().await;
        let current_generation = snapshot.turn_generation;
        let turn_in_flight = snapshot.turn_in_flight;
        let Some(turn) = snapshot.native_background_turn.as_mut() else {
            return;
        };
        if turn.adopted_generation.is_none() {
            turn.terminal_status = Some(thread_status.to_string());
            return;
        }
        turn_in_flight && turn.adopted_generation == Some(current_generation)
    };
    if !active {
        return;
    }
    if let Some(error) = turn_failure_error_event(stop_reason, context.agent_type, None) {
        emit_with_state(context.state, context.emitter, error).await;
    }
    PermissionRuntime::new(context.state, context.emitter, context.permissions)
        .drain_then_emit(
            "native_background_turn_completed",
            AcpEvent::TurnComplete {
                session_id: context.session_id.to_string(),
                stop_reason: stop_reason.to_string(),
                agent_type: context.agent_type.to_string(),
            },
        )
        .await;
}

/// Returns `Ok(None)` on normal exit (disconnect / channel closed) or
/// `Ok(Some(ForkExitInfo))` when the loop should be restarted on a forked session.
#[allow(clippy::too_many_arguments)]
async fn run_conversation_loop<'a>(
    session: &mut sacp::ActiveSession<'a, Agent>,
    conn_id: &str,
    emitter: &EventEmitter,
    state: &Arc<RwLock<SessionState>>,
    agent_type: AgentType,
    perms: &PendingPermissions,
    cmd_rx: &mut mpsc::Receiver<ConnectionCommand>,
    terminal_runtime: Arc<TerminalRuntime>,
    cwd: &str,
    supports_fork: bool,
    prompt_ledger: &background_watch::PromptLedger,
    // Source of the broker reference used to cascade-cancel pending
    // delegations on parent prompt cancel / non-success TurnComplete.
    // `None` for test paths that don't wire delegation.
    delegation_injection: Option<&DelegationInjection>,
    stderr_tail: &Arc<StderrTail>,
) -> Result<Option<ForkExitInfo>, sacp::Error> {
    // Session-scoped cache for diffing cumulative `raw_output` snapshots
    // into incremental deltas. Shared across the idle loop and the active
    // turn loop so tool calls that span turns stay consistent.
    let mut raw_output_cache = ToolCallOutputCache::default();
    // Session-scoped CodeBuddy live state: authoritative title rewrites
    // (tool_call_id → "agent" / inner `mcp__…` name) so a later status-only
    // update can't downgrade an Agent / delegation card mid-stream, plus the
    // open-sub-agent window used to suppress a sub-agent's interleaved
    // thought/message chunks. See `emit_conversation_update`. Shared across the
    // idle and turn loops.
    let mut cb_state = CodeBuddyLiveState::default();
    let mut drop_log_throttle = DropLogThrottle::default();
    loop {
        // Wait for either a user command or a session update (e.g. available_commands_update)
        let cmd = loop {
            tokio::select! {
                biased;
                cmd = cmd_rx.recv() => break cmd,
                update = session.read_update() => {
                    match update {
                        Ok(SessionMessage::SessionMessage(dispatch)) => {
                            let h = emitter.clone();
                            let st = Arc::clone(state);
                            let cwd_opt = Some(cwd);
                            let session_id = session.session_id().0.to_string();
                            let dispatch = fix_usage_update_nulls(dispatch);
                            let _ = MatchDispatch::new(dispatch)
                                .if_notification(
                                    async |notif: SessionNotification| {
                                        let background_status = codex_thread_status(&notif.update)
                                            .map(str::to_owned);
                                        emit_conversation_update(&st, &h, agent_type, notif.update, cwd_opt, &mut raw_output_cache, &mut cb_state).await;
                                        if let Some(status) = background_status {
                                            finish_native_background_turn(
                                                NativeBackgroundFinishContext {
                                                    state: &st,
                                                    emitter: &h,
                                                    permissions: perms,
                                                    session_id: &session_id,
                                                    agent_type,
                                                },
                                                &status,
                                            )
                                            .await;
                                        }
                                        Ok(())
                                    },
                                )
                                .await
                                .otherwise(async |dispatch| {
                                    maybe_emit_claude_sdk_ext_notification(&st, &h, dispatch).await;
                                    Ok(())
                                })
                                .await;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            drop_log_throttle.record("idle", &e);
                        }
                    }
                }
            }
        };
        match cmd {
            Some(ConnectionCommand::Prompt {
                blocks,
                user_context,
                user_messages,
                accepted,
            }) => {
                prompt_ledger.record_prompt_blocks(&blocks);
                let is_compaction = is_codex_compaction_prompt(agent_type, &blocks);
                let sid = session.session_id().clone();
                let mut stats = prompt_input_stats(&blocks);
                stats.context_bytes = user_context.as_ref().map_or(0, |value| value.len());
                let prompt_log = PromptLogContext {
                    connection_id: conn_id,
                    session_id: sid.0.to_string(),
                    agent_type,
                    is_compaction,
                    stats,
                    started_at: std::time::Instant::now(),
                };
                prompt_log.started();
                let compaction_marker = if is_compaction {
                    Some(
                        crate::acp::codex_rollout_migration::compaction_marker(
                            session.session_id().0.as_ref(),
                        )
                        .await,
                    )
                } else {
                    None
                };
                let mut prompt_blocks = match map_prompt_blocks(agent_type, blocks) {
                    Ok(prompt_blocks) => prompt_blocks,
                    Err(error) => {
                        tracing::error!(
                            agent_type = %agent_type,
                            error = %error,
                            "[ACP] rejected invalid image transport before Agent send"
                        );
                        emit_with_state(
                            state,
                            emitter,
                            AcpEvent::Error {
                                message: error,
                                agent_type: agent_type.to_string(),
                                code: None,
                                details: None,
                                terminal: false,
                            },
                        )
                        .await;
                        PermissionRuntime::new(state, emitter, perms)
                            .drain_then_emit(
                                "invalid_image_transport",
                                AcpEvent::TurnComplete {
                                    session_id: session.session_id().0.to_string(),
                                    stop_reason: "cancelled".into(),
                                    agent_type: agent_type.to_string(),
                                },
                            )
                            .await;
                        prompt_log.interrupted("invalid_image_transport", false, 0);
                        continue;
                    }
                };
                if let Some(context) = user_context {
                    prompt_blocks
                        .insert(0, ContentBlock::Text(TextContent::new(context.to_string())));
                }
                if prompt_blocks.is_empty() {
                    // Defensive: the manager rejects empty prompts before the
                    // concurrency gate is set / the command is enqueued (see
                    // `send_prompt_inner`), and `map_prompt_blocks` is 1:1, so an
                    // empty prompt should never reach here. If one ever did, it
                    // would carry no turn-in-flight gate, so just surface the
                    // error and keep the idle loop alive.
                    emit_with_state(
                        state,
                        emitter,
                        AcpEvent::Error {
                            message: "Prompt must contain at least one content block".into(),
                            agent_type: agent_type.to_string(),
                            code: None,
                            details: None,
                            // Recoverable: idle loop continues, awaiting the
                            // next user command. Connection stays alive.
                            terminal: false,
                        },
                    )
                    .await;
                    prompt_log.interrupted("empty_prompt_rejected", false, 0);
                    continue;
                }
                emit_with_state(
                    state,
                    emitter,
                    AcpEvent::StatusChanged {
                        status: ConnectionStatus::Prompting,
                    },
                )
                .await;

                // Broadcast the user's prompt to cross-client viewers BEFORE
                // issuing the agent request. Emitting here (rather than at the
                // manager enqueue site) guarantees its seq strictly precedes the
                // turn's assistant/status events — viewers apply events in seq
                // order, so otherwise the reply could render above the message.
                // It also means a prompt that is never processed (rejected /
                // dropped) broadcasts nothing. `apply_event` records it as
                // `pending_user_message` so a client attaching mid-turn still
                // renders the user turn from the snapshot.
                for (message_id, blocks) in user_messages {
                    emit_with_state(state, emitter, AcpEvent::UserMessage { message_id, blocks })
                        .await;
                }

                if let Some(trace) = state.read().await.startup_trace.clone() {
                    trace.first_prompt_dispatched();
                }
                // Clone connection and session ID before entering the
                // select loop so we can send CancelNotification without
                // conflicting with session.read_update()'s mutable borrow.
                let cx = session.connection();
                let prompt_request = PromptRequest::new(sid.clone(), prompt_blocks);
                // Use Box::pin (heap) instead of tokio::pin! (stack) so the
                // future can be moved into a background task on cancel.
                let mut prompt_response = Box::pin(
                    cx.clone()
                        .send_request_to(Agent, prompt_request)
                        .block_task(),
                );
                if let Some(accepted) = accepted {
                    let _ = accepted.send(());
                }
                let mut tracked_terminal_tool_calls: HashMap<String, TrackedTerminalToolCall> =
                    HashMap::new();
                let mut terminal_poll_interval = tokio::time::interval(
                    std::time::Duration::from_millis(TERMINAL_POLL_INTERVAL_MS),
                );
                terminal_poll_interval
                    .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut disconnect_requested = false;
                let mut output_probe = TurnOutputProbe::new(stderr_tail.mark());
                let mut tool_call_count = 0usize;
                // `startedNewTurn` may let the wrapper begin producing the next
                // turn before the old prompt response has unwound locally. Hold
                // those updates until lifecycle settlement adopts the turn and
                // emits its UserMessage first.
                let mut native_background_updates = Vec::new();
                // A CodeBuddy native sub-agent's full lifecycle (Agent tool call
                // open → completed) happens within one turn, so reset the
                // suppression window at each turn start. This bounds the tracking
                // sets and guarantees a sub-agent that ended without a terminal
                // frame (cancel/abort) can never suppress the NEXT turn's
                // main-agent thinking. `title_overrides` intentionally persists
                // (a card's identity is session-stable).
                cb_state.open_subagents.clear();
                cb_state.closed_subagents.clear();

                // Read updates until turn completes.
                // We must also listen for commands (e.g. RespondPermission)
                // to avoid deadlocking when the agent awaits a permission response.
                loop {
                    tokio::select! {
                        update = session.read_update() => {
                            let update = match update {
                                Ok(u) => u,
                                Err(e) => {
                                    output_probe.note_dropped(DropSite::Decode, &e);
                                    drop_log_throttle.record(DropSite::Decode.label(), &e);
                                    continue;
                                }
                            };
                            match update {
                                SessionMessage::SessionMessage(dispatch) => {
                                    let h = emitter.clone();
                                    let st = Arc::clone(state);
                                    let runtime = terminal_runtime.clone();
                                    let session_id = sid.clone();
                                    let cwd_opt = Some(cwd);
                                    let dispatch = fix_usage_update_nulls(dispatch);
                                    if let Err(e) = MatchDispatch::new(dispatch)
                                        .if_notification(
                                            async |notif: SessionNotification| {
                                                let pending_background = {
                                                    let snapshot = st.read().await;
                                                    snapshot
                                                        .native_background_turn
                                                        .as_ref()
                                                        .is_some_and(|turn| {
                                                            turn.source_generation
                                                                == snapshot.turn_generation
                                                                && turn.adopted_generation.is_none()
                                                        })
                                                };
                                                if pending_background {
                                                    if let Some(status) = codex_thread_status(&notif.update) {
                                                        finish_native_background_turn(
                                                            NativeBackgroundFinishContext {
                                                                state: &st,
                                                                emitter: &h,
                                                                permissions: perms,
                                                                session_id: session_id.0.as_ref(),
                                                                agent_type,
                                                            },
                                                            status,
                                                        )
                                                        .await;
                                                    }
                                                    native_background_updates.push(notif.update);
                                                    return Ok(());
                                                }
                                                let background_status = codex_thread_status(&notif.update)
                                                    .map(str::to_owned);
                                                let should_poll_now = track_terminal_tool_calls(
                                                    &notif.update,
                                                    &mut tracked_terminal_tool_calls,
                                                );
                                                output_probe.note_update(is_agent_output_update(
                                                    agent_type,
                                                    &notif.update,
                                                ));
                                                if matches!(&notif.update, SessionUpdate::ToolCall(_)) {
                                                    tool_call_count += 1;
                                                }
                                                emit_conversation_update(&st, &h, agent_type, notif.update, cwd_opt, &mut raw_output_cache, &mut cb_state).await;
                                                if let Some(status) = background_status {
                                                    finish_native_background_turn(
                                                        NativeBackgroundFinishContext {
                                                            state: &st,
                                                            emitter: &h,
                                                            permissions: perms,
                                                            session_id: session_id.0.as_ref(),
                                                            agent_type,
                                                        },
                                                        &status,
                                                    )
                                                    .await;
                                                }
                                                if should_poll_now {
                                                    poll_tracked_terminal_tool_calls(
                                                        runtime.as_ref(),
                                                        &session_id,
                                                        &st,
                                                        &h,
                                                        &mut tracked_terminal_tool_calls,
                                                    )
                                                    .await;
                                                }
                                                Ok(())
                                            },
                                        )
                                        .await
                                        .otherwise(async |dispatch| {
                                            maybe_emit_claude_sdk_ext_notification(&st, &h, dispatch).await;
                                            Ok(())
                                        })
                                        .await
                                    {
                                        output_probe.note_dropped(DropSite::Dispatch, &e);
                                        drop_log_throttle.record(DropSite::Dispatch.label(), &e);
                                    }
                                }
                                SessionMessage::StopReason(reason) => {
                                    if !tracked_terminal_tool_calls.is_empty() {
                                        poll_tracked_terminal_tool_calls(
                                            terminal_runtime.as_ref(),
                                            &sid,
                                            state,
                                            emitter,
                                            &mut tracked_terminal_tool_calls,
                                        )
                                        .await;
                                    }
                                    let raw_reason_str = stop_reason_to_str(reason);
                                    let (base_reason, empty_report) = finish_turn_reason(
                                        &output_probe,
                                        raw_reason_str,
                                        stderr_tail,
                                        !is_compaction,
                                    );
                                    let reason_str = verified_compaction_stop_reason(
                                        base_reason,
                                        compaction_marker.as_ref(),
                                        sid.0.as_ref(),
                                    )
                                    .await;
                                    prompt_log.completed(
                                        reason_str,
                                        output_probe.saw_agent_output(),
                                        tool_call_count,
                                    );
                                    if let Some(err_event) = turn_failure_error_event(
                                        reason_str,
                                        agent_type,
                                        empty_report.as_ref(),
                                    ) {
                                        emit_with_state(state, emitter, err_event).await;
                                    }
                                    PermissionRuntime::new(state, emitter, perms)
                                        .drain_then_emit(
                                            "turn_completed",
                                            AcpEvent::TurnComplete {
                                                session_id: sid.0.to_string(),
                                                stop_reason: reason_str.into(),
                                                agent_type: agent_type.to_string(),
                                            },
                                        )
                                        .await;
                                    // Cascade-cancel any pending delegations
                                    // whenever the parent's turn ended for a
                                    // reason other than clean `end_turn`. The
                                    // `end_turn` path lets the legitimate
                                    // delegation completion drain naturally;
                                    // every other reason (cancelled / refusal /
                                    // max_tokens / max_turn_requests / empty /
                                    // unknown) means the parent will never
                                    // consume the in-flight result, so the
                                    // child must be torn down. The connection
                                    // stays alive (only the turn ended), so use
                                    // the turn-scoped cancel that keeps the
                                    // parent's `consumed` tool_call memory — a
                                    // late re-emit must not re-register and
                                    // mis-bind the next same-key delegation.
                                    //
                                    // Await inline: the fast tracker +
                                    // parked-call drain MUST finish before the
                                    // loop accepts the next prompt so it stays
                                    // scoped to the just-ended turn. The broker
                                    // backgrounds the slow child teardown
                                    // (spawner.cancel/disconnect) internally, so
                                    // this won't block on slow agents; its
                                    // idempotent drain also lets the cleanup-
                                    // guard cascade at run_connection exit run
                                    // without race-double-drain.
                                    if reason_str != "end_turn" {
                                        if let Some(inj) = delegation_injection {
                                            inj.broker.cancel_by_parent_turn(conn_id).await;
                                        }
                                    }
                                    break;
                                }
                                _ => {}
                            }
                        }
                        prompt_result = &mut prompt_response => {
                            let response = match prompt_result {
                                Ok(response) => response,
                                Err(error) => {
                                    let tool_stats = {
                                        let snapshot = state.read().await;
                                        tool_call_failure_stats(&snapshot)
                                    };
                                    prompt_log.failed(&error, tool_call_count, &tool_stats);
                                    if is_prompt_transport_error(&error) {
                                        return Err(error);
                                    }
                                    let detail = safe_error_detail(&error.to_string());
                                    tracing::warn!(
                                        connection_id = conn_id,
                                        session_id = %sid.0,
                                        agent_type = %agent_type,
                                        error_kind = "acp_prompt_request_failed",
                                        error = %detail,
                                        tool_call_count,
                                        "[ACP] prompt RPC failed; keeping connection alive"
                                    );
                                    emit_with_state(
                                        state,
                                        emitter,
                                        AcpEvent::Error {
                                            message: detail.clone(),
                                            agent_type: agent_type.to_string(),
                                            code: Some("acp_prompt_request_failed".into()),
                                            details: Some(detail),
                                            terminal: false,
                                        },
                                    )
                                    .await;
                                    terminal_runtime
                                        .release_all_for_session(sid.0.as_ref())
                                        .await;
                                    tracked_terminal_tool_calls.clear();
                                    PermissionRuntime::new(state, emitter, perms)
                                        .drain_then_emit(
                                            "prompt_error",
                                            AcpEvent::TurnComplete {
                                                session_id: sid.0.to_string(),
                                                stop_reason: "prompt_error".into(),
                                                agent_type: agent_type.to_string(),
                                            },
                                        )
                                        .await;
                                    if let Some(inj) = delegation_injection {
                                        inj.broker.cancel_by_parent_turn(conn_id).await;
                                    }
                                    break;
                                }
                            };
                            // AIR terminal failures use the prompt response as
                            // their canonical carrier. Emit before TurnComplete
                            // so a higher-revision error can supersede a retry
                            // warning before the clean-boundary settle runs.
                            let terminal_failure = crate::acp::session_failure::from_air_meta(
                                response.meta.as_ref(),
                            );
                            if let Some(record) = terminal_failure.as_ref() {
                                emit_with_state(
                                    state,
                                    emitter,
                                    AcpEvent::SessionFailure {
                                        record: record.clone(),
                                    },
                                )
                                .await;
                            }
                            let reason = response.stop_reason;
                            if !tracked_terminal_tool_calls.is_empty() {
                                poll_tracked_terminal_tool_calls(
                                    terminal_runtime.as_ref(),
                                    &sid,
                                    state,
                                    emitter,
                                    &mut tracked_terminal_tool_calls,
                                )
                                .await;
                            }
                            let raw_reason_str = stop_reason_to_str(reason);
                            // Adapters may disguise a typed terminal AIR error
                            // as end_turn. Its banner is authoritative, so do
                            // not stack a generic empty-turn diagnosis on it.
                            let (base_reason, empty_report) = if terminal_failure
                                .as_ref()
                                .is_some_and(|record| record.severity == "error")
                            {
                                (raw_reason_str, None)
                            } else {
                                finish_turn_reason(
                                    &output_probe,
                                    raw_reason_str,
                                    stderr_tail,
                                    !is_compaction,
                                )
                            };
                            let reason_str = verified_compaction_stop_reason(
                                base_reason,
                                compaction_marker.as_ref(),
                                sid.0.as_ref(),
                            )
                            .await;
                            prompt_log.completed(
                                reason_str,
                                output_probe.saw_agent_output(),
                                tool_call_count,
                            );
                            if let Some(err_event) = turn_failure_error_event(
                                reason_str,
                                agent_type,
                                empty_report.as_ref(),
                            ) {
                                emit_with_state(state, emitter, err_event).await;
                            }
                            PermissionRuntime::new(state, emitter, perms)
                                .drain_then_emit(
                                    "turn_completed",
                                    AcpEvent::TurnComplete {
                                        session_id: sid.0.to_string(),
                                        stop_reason: reason_str.into(),
                                        agent_type: agent_type.to_string(),
                                    },
                                )
                                .await;
                            // Mirror the StopReason-message branch above:
                            // cascade-cancel on any non-`end_turn` reason
                            // so in-flight delegations don't dangle when
                            // the parent's turn ended without consuming
                            // their result. Turn-scoped (connection stays
                            // alive → keep `consumed`) and awaited inline
                            // (fast drain before the next prompt; broker
                            // backgrounds the slow child teardown) for the
                            // same reasons as that branch — see above.
                            if reason_str != "end_turn" {
                                if let Some(inj) = delegation_injection {
                                    inj.broker.cancel_by_parent_turn(conn_id).await;
                                }
                            }
                            break;
                        }
                        _ = terminal_poll_interval.tick(), if !tracked_terminal_tool_calls.is_empty() => {
                            poll_tracked_terminal_tool_calls(
                                terminal_runtime.as_ref(),
                                &sid,
                                state,
                                emitter,
                                &mut tracked_terminal_tool_calls,
                            )
                            .await;
                        }
                        cmd = cmd_rx.recv() => {
                            match cmd {
                                Some(ConnectionCommand::RespondPermission {
                                    request_id,
                                    option_id,
                                }) => {
                                    respond_permission_request(
                                        state,
                                        emitter,
                                        perms,
                                        request_id,
                                        option_id,
                                    )
                                    .await;
                                }
                                Some(ConnectionCommand::SetMode { mode_id }) => {
                                    let req = SetSessionModeRequest::new(sid.clone(), mode_id.clone());
                                    match cx.send_request_to(Agent, req).block_task().await {
                                        Ok(_) => {
                                            emit_with_state(
                                                state,
                                                emitter,
                                                AcpEvent::ModeChanged { mode_id },
                                            )
                                            .await;
                                        }
                                        Err(e) => {
                                            emit_with_state(
                                                state,
                                                emitter,
                                                AcpEvent::Error {
                                                    message: format!("Failed to set mode: {e}"),
                                                    agent_type: agent_type.to_string(),
                                                    code: None,
                                                    details: None,
                                                    // Recoverable: just a failed mode toggle.
                                                    terminal: false,
                                                },
                                            )
                                            .await;
                                        }
                                    }
                                }
                                Some(ConnectionCommand::SetConfigOption {
                                    config_id,
                                    value_id,
                                }) => {
                                    if let Err(e) = set_session_config_option(
                                        &cx,
                                        &sid,
                                        state,
                                        emitter,
                                        agent_type,
                                        config_id,
                                        value_id,
                                    )
                                    .await
                                    {
                                        emit_with_state(
                                            state,
                                            emitter,
                                            AcpEvent::Error {
                                                message: format!("Failed to set config option: {e}"),
                                                agent_type: agent_type.to_string(),
                                                code: None,
                                                details: None,
                                                // Recoverable: just a failed config-option toggle.
                                                terminal: false,
                                            },
                                        )
                                        .await;
                                    }
                                }
                                Some(ConnectionCommand::Cancel) => {
                                    // Send CancelNotification to agent to stop the current turn
                                    let _ = cx.send_notification_to(
                                        Agent,
                                        CancelNotification::new(sid.clone()),
                                    );
                                    // Also terminate any command runtimes created for this
                                    // session so cancellation does not hang on long-running
                                    // terminal tools.
                                    terminal_runtime
                                        .release_all_for_session(sid.0.as_ref())
                                        .await;
                                    tracked_terminal_tool_calls.clear();
                                    prompt_log.completed(
                                        "cancelled",
                                        output_probe.saw_agent_output(),
                                        tool_call_count,
                                    );
                                    // Immediately emit TurnComplete so the frontend
                                    // transitions out of "prompting" and the user can
                                    // send new messages.  Don't wait for the agent --
                                    // it may be slow to respond or not respond at all.
                                    PermissionRuntime::new(state, emitter, perms)
                                        .drain_then_emit(
                                            "turn_cancelled",
                                            AcpEvent::TurnComplete {
                                                session_id: sid.0.to_string(),
                                                stop_reason: "cancelled".into(),
                                                agent_type: agent_type.to_string(),
                                            },
                                        )
                                        .await;
                                    // Cascade-cancel any in-flight delegations owned by
                                    // this parent connection. Idempotent with the
                                    // cleanup-guard cancel_by_parent at the end of
                                    // run_connection (#1: empty pending → no-op).
                                    // Without this, a user-initiated cancel of a parent
                                    // prompt mid-delegation would leave the child agent
                                    // running indefinitely (broker no longer applies a
                                    // timeout; only an MCP `notifications/cancelled` or
                                    // a parent/child disconnect would otherwise tear
                                    // the delegation down). Turn-scoped: the
                                    // connection stays alive after a prompt cancel,
                                    // so keep the parent's `consumed` tool_call
                                    // memory (a re-emit must not mis-bind the next
                                    // same-key delegation); the cleanup-guard
                                    // teardown still clears everything when the
                                    // connection finally goes away.
                                    //
                                    // Await inline so the fast tracker +
                                    // parked-call drain is ordered before the
                                    // next prompt (keeping it scoped to the
                                    // just-ended turn); the broker backgrounds
                                    // the slow child teardown internally, so the
                                    // user-visible Cancel path doesn't wait on
                                    // (potentially slow) child agent teardown.
                                    // The user already saw the parent's
                                    // TurnComplete above, and the broker's
                                    // drain-first lock guarantees no double
                                    // DelegationCompleted emit.
                                    if let Some(inj) = delegation_injection {
                                        inj.broker.cancel_by_parent_turn(conn_id).await;
                                    }
                                    // Drain the prompt response in the background so
                                    // the SACP library doesn't log "receiver dropped"
                                    // errors when the agent eventually responds.
                                    tokio::spawn(async move {
                                        let _ = prompt_response.await;
                                    });
                                    break;
                                }
                                Some(ConnectionCommand::SafeCancel {
                                    expected_turn_generation,
                                }) => {
                                    let turn_generation = state.read().await.turn_generation;
                                    if turn_generation == expected_turn_generation {
                                        tracing::info!(
                                            connection_id = conn_id,
                                            turn_generation,
                                            "[agent-input] requesting safe turn cancellation"
                                        );
                                        if let Err(error) = cx.send_notification_to(
                                            Agent,
                                            CancelNotification::new(sid.clone()),
                                        ) {
                                            tracing::warn!(
                                                connection_id = conn_id,
                                                error = %error,
                                                "[agent-input] safe cancellation request failed"
                                            );
                                        }
                                    } else {
                                        tracing::debug!(
                                            connection_id = conn_id,
                                            expected_turn_generation,
                                            turn_generation,
                                            "[agent-input] ignoring stale safe cancellation"
                                        );
                                    }
                                }
                                Some(ConnectionCommand::NativeSteer {
                                    message_id,
                                    blocks,
                                    codex_image_validation,
                                    expected_turn_generation,
                                    reply,
                                    settled,
                                }) => {
                                    let available = {
                                        let snapshot = state.read().await;
                                        snapshot.turn_generation == expected_turn_generation
                                            && snapshot.native_steering_available
                                    };
                                    let user_blocks = crate::acp::user_blocks_from_prompt(&blocks);
                                    let validation = match codex_image_validation {
                                        Some((data_dir, scope)) => {
                                            crate::acp::agent_image_input::validate_codex_image_inputs(
                                                &data_dir,
                                                scope,
                                                &blocks,
                                            )
                                            .await
                                        }
                                        None => Ok(()),
                                    };
                                    let mut outcome = if let Err(error) = validation {
                                        tracing::warn!(
                                            connection_id = conn_id,
                                            error = %error,
                                            "[agent-input] native steer image validation failed"
                                        );
                                        NativeSteerOutcome::Failed(error.to_string())
                                    } else if available {
                                        send_native_steer(&cx, &sid, agent_type, blocks).await
                                    } else {
                                        NativeSteerOutcome::Unsupported
                                    };
                                    if outcome == NativeSteerOutcome::StartedNewTurn {
                                        let mut snapshot = state.write().await;
                                        if snapshot.turn_generation == expected_turn_generation
                                            && snapshot.turn_in_flight
                                            && snapshot.native_background_turn.is_none()
                                        {
                                            snapshot.native_background_turn = Some(
                                                crate::acp::session_state::NativeBackgroundTurn {
                                                    message_id,
                                                    blocks: user_blocks,
                                                    source_generation: expected_turn_generation,
                                                    adopted_generation: None,
                                                    terminal_status: None,
                                                },
                                            );
                                            tracing::info!(
                                                connection_id = conn_id,
                                                source_generation = expected_turn_generation,
                                                "[agent-input] registered wrapper-owned native turn"
                                            );
                                        } else {
                                            tracing::error!(
                                                connection_id = conn_id,
                                                source_generation = expected_turn_generation,
                                                current_generation = snapshot.turn_generation,
                                                turn_in_flight = snapshot.turn_in_flight,
                                                already_registered = snapshot.native_background_turn.is_some(),
                                                "[agent-input] could not register wrapper-owned native turn"
                                            );
                                            outcome = NativeSteerOutcome::Failed(
                                                "native steer result could not be attached to the active generation"
                                                    .into(),
                                            );
                                        }
                                    }
                                    let _ = reply.send(outcome);
                                    if settled.await.is_err() {
                                        tracing::warn!(
                                            connection_id = conn_id,
                                            expected_turn_generation,
                                            "[agent-input] native steer settlement acknowledgement dropped"
                                        );
                                    }
                                }
                                Some(ConnectionCommand::Disconnect) | None => {
                                    tracing::info!(
                                        connection_id = conn_id,
                                        phase = "prompting",
                                        "[ACP] disconnect requested"
                                    );
                                    let _ = cx.send_notification_to(
                                        Agent,
                                        CancelNotification::new(sid.clone()),
                                    );
                                    terminal_runtime
                                        .release_all_for_session(sid.0.as_ref())
                                        .await;
                                    tracked_terminal_tool_calls.clear();
                                    PermissionRuntime::new(state, emitter, perms)
                                        .drain("connection_disconnected")
                                        .await;
                                    prompt_log.interrupted(
                                        "disconnect_requested",
                                        output_probe.saw_agent_output(),
                                        tool_call_count,
                                    );
                                    disconnect_requested = true;
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                if disconnect_requested {
                    tracing::info!(
                        connection_id = conn_id,
                        "[ACP] closing connection loop after disconnect"
                    );
                    break;
                }

                if wait_for_native_background_adoption(state).await {
                    for update in native_background_updates {
                        let terminal_status = codex_thread_status(&update).map(str::to_owned);
                        emit_conversation_update(
                            state,
                            emitter,
                            agent_type,
                            update,
                            Some(cwd),
                            &mut raw_output_cache,
                            &mut cb_state,
                        )
                        .await;
                        let Some(status) = terminal_status else {
                            continue;
                        };
                        finish_native_background_turn(
                            NativeBackgroundFinishContext {
                                state,
                                emitter,
                                permissions: perms,
                                session_id: session.session_id().0.as_ref(),
                                agent_type,
                            },
                            &status,
                        )
                        .await;
                    }
                } else {
                    emit_with_state(
                        state,
                        emitter,
                        AcpEvent::StatusChanged {
                            status: ConnectionStatus::Connected,
                        },
                    )
                    .await;
                }
            }
            Some(ConnectionCommand::RespondPermission {
                request_id,
                option_id,
            }) => {
                respond_permission_request(state, emitter, perms, request_id, option_id).await;
            }
            Some(ConnectionCommand::SetMode { mode_id }) => {
                if let Err(e) = set_session_mode(session, state, emitter, mode_id).await {
                    emit_with_state(
                        state,
                        emitter,
                        AcpEvent::Error {
                            message: format!("Failed to set mode: {e}"),
                            agent_type: agent_type.to_string(),
                            code: None,
                            details: None,
                            // Recoverable: idle SetMode failure leaves the
                            // connection alive — same rationale as the
                            // mid-prompt SetMode site above.
                            terminal: false,
                        },
                    )
                    .await;
                }
            }
            Some(ConnectionCommand::SetConfigOption {
                config_id,
                value_id,
            }) => {
                let cx = session.connection();
                let sid = session.session_id().clone();
                if let Err(e) = set_session_config_option(
                    &cx, &sid, state, emitter, agent_type, config_id, value_id,
                )
                .await
                {
                    emit_with_state(
                        state,
                        emitter,
                        AcpEvent::Error {
                            message: format!("Failed to set config option: {e}"),
                            agent_type: agent_type.to_string(),
                            code: None,
                            details: None,
                            // Recoverable: idle SetConfigOption failure leaves
                            // the connection alive.
                            terminal: false,
                        },
                    )
                    .await;
                }
            }
            Some(ConnectionCommand::Cancel) => {
                let cx = session.connection();
                let sid = session.session_id().clone();
                let _ = cx.send_notification_to(Agent, CancelNotification::new(sid.clone()));
                terminal_runtime
                    .release_all_for_session(sid.0.as_ref())
                    .await;
                PermissionRuntime::new(state, emitter, perms)
                    .drain("idle_turn_cancelled")
                    .await;
                // Cascade-cancel any pending delegations owned by this parent.
                // Reached when Cancel arrives between prompts (idle path); the
                // inner Cancel handler covers mid-prompt. Both must trigger
                // because the per-prompt cancel path doesn't tear down the
                // parent connection, so the cleanup-guard cancel_by_parent
                // at run_connection's exit wouldn't fire. Turn-scoped for that
                // same reason: the connection stays alive, so keep the parent's
                // `consumed` tool_call memory (a re-emit must not mis-bind the
                // next same-key delegation).
                //
                // Awaited inline (fast drain before the next prompt; broker
                // backgrounds the slow child teardown): see inner Cancel
                // handler above for rationale.
                if let Some(inj) = delegation_injection {
                    inj.broker.cancel_by_parent_turn(conn_id).await;
                }
            }
            Some(ConnectionCommand::SafeCancel {
                expected_turn_generation,
            }) => {
                tracing::debug!(
                    connection_id = conn_id,
                    expected_turn_generation,
                    "[agent-input] ignoring safe cancellation while connection is idle"
                );
            }
            Some(ConnectionCommand::NativeSteer { reply, .. }) => {
                let _ = reply.send(NativeSteerOutcome::Unsupported);
            }
            Some(ConnectionCommand::Fork { reply }) => {
                if !supports_fork {
                    let _ = reply.send(Err(AcpError::protocol(
                        "This agent does not support session/fork".to_string(),
                    )));
                    continue;
                }
                let cx = session.connection();
                let sid = session.session_id().clone();
                tracing::info!(
                    "[ACP] Sending session/fork for session_id={} cwd={}",
                    sid.0,
                    cwd
                );
                let result = crate::acp::fork::fork_session(&cx, &sid, cwd).await;
                match result {
                    Ok(fork_response) => {
                        tracing::info!(
                            "[ACP] Fork succeeded: new_session_id={}",
                            fork_response.session_id.0
                        );
                        return Ok(Some(ForkExitInfo {
                            fork_response,
                            original_session_id: sid.0.to_string(),
                            reply,
                            connection: cx,
                        }));
                    }
                    Err(e) => {
                        tracing::error!("[ACP] Fork failed: {e}");
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Some(ConnectionCommand::Disconnect) | None => {
                break;
            }
        }
    }
    Ok(None)
}

/// Serialize tool-call `content` blocks into a single human-readable string.
///
/// `include_diffs = false` skips `Diff` blocks. Used when the edit has been
/// hoisted into a synthesized canonical `raw_input` (see
/// `synthesize_edit_input_from_diffs`): without this the same edit ships twice
/// (doubling the event) and the hunkless full-file `--- /+++` blob stays in the
/// tool `output`, where `extractEditLineChangeStats` mis-counts it as full-file
/// +/- totals in the card header even though the body shows the compact diff.
fn serialize_tool_call_content(content: &[ToolCallContent], include_diffs: bool) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for item in content {
        match item {
            ToolCallContent::Content(c) => {
                if let ContentBlock::Text(text) = &c.content {
                    parts.push(text.text.clone());
                }
            }
            ToolCallContent::Diff(diff) if include_diffs => {
                let path = diff.path.display();
                let mut diff_text = format!("--- {path}\n+++ {path}\n");
                if let Some(old) = &diff.old_text {
                    for line in old.lines() {
                        diff_text.push_str(&format!("-{line}\n"));
                    }
                }
                for line in diff.new_text.lines() {
                    diff_text.push_str(&format!("+{line}\n"));
                }
                parts.push(diff_text);
            }
            ToolCallContent::Terminal(t) => {
                parts.push(format!("[Terminal: {}]", t.terminal_id));
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Synthesize a canonical edit `raw_input` from `ToolCallContent::Diff` block(s).
///
/// codex-acp reports file edits as ACP `Diff` content blocks and leaves
/// `raw_input` empty — the edit lives only in `content`, and the ACP `title` is
/// the diff header `--- <path>`. With no `raw_input` the frontend classifier
/// (`inferLiveToolName`) falls back to `normalizeToolName(title)`, which returns
/// unrecognized strings verbatim, so the tool call renders as a generic tool
/// literally *named* `--- <path>` (wrench icon, raw header as the title) instead
/// of an edit card. The historical path is unaffected because the JSONL parser
/// stores codex's native `*** Begin Patch` text.
///
/// Reconstructing from the already-serialized `--- /+++` string would be lossy
/// (content lines beginning with `-`/`+`/`---`/`+++`, the old/new boundary,
/// CRLF). Here the structured `Diff` is still intact, so map it losslessly:
/// - exactly one Diff  -> `{"file_path","old_string","new_string"}`
/// - multiple Diffs    -> `{"changes":{"<path>":{"old_text","new_text"},…}}`
///
/// Both shapes classify as `"edit"` (`inferFromInput`) and render through the
/// existing `EditToolInput` / `EditChangesToolInput` → `generateUnifiedDiff`
/// pipeline (a real hunk diff, minimal even for full-file old/new). Returns
/// `None` when `content` carries no `Diff`, so callers only fall back to it when
/// the agent supplied no `raw_input` of its own.
fn synthesize_edit_input_from_diffs(content: &[ToolCallContent]) -> Option<String> {
    // Keep `old_text` as `Option`: ACP reports `None` for a newly created file
    // (`Diff.old_text` semantics). That distinction is the whole point of this
    // function's fix — collapsing `None` to `""` and emitting an edit shape
    // makes the frontend build a `--- a/<path>` diff, which `isAddedFileDiff`
    // does NOT match, so a freshly created file mis-renders as a modification
    // (the historical apply_patch `*** Add File:` path classifies it correctly).
    let diffs: Vec<(String, Option<String>, String)> = content
        .iter()
        .filter_map(|item| match item {
            ToolCallContent::Diff(diff) => Some((
                diff.path.display().to_string(),
                diff.old_text.clone(),
                diff.new_text.clone(),
            )),
            _ => None,
        })
        .collect();

    match diffs.as_slice() {
        [] => None,
        // New file (old_text absent) → write shape. `inferFromInput` classifies
        // `{file_path, content}` as `write`, whose diff builder emits the
        // `--- /dev/null` header `isAddedFileDiff` keys on → renders as a new
        // file, matching the reloaded-from-DB path.
        [(path, None, new)] => Some(
            serde_json::json!({
                "file_path": path,
                "content": new,
            })
            .to_string(),
        ),
        // Edit → canonical `{old_string,new_string}` for the frontend's
        // `generateUnifiedDiff` (a real hunk diff, minimal even for full-file
        // old/new).
        [(path, Some(old), new)] => Some(
            serde_json::json!({
                "file_path": path,
                "old_string": old,
                "new_string": new,
            })
            .to_string(),
        ),
        many => {
            let mut changes = serde_json::Map::new();
            for (path, old, new) in many {
                // Per-entry, mirror the single-diff split: a new file gets a
                // ready-made creation diff (`buildChunkFromEditChange` returns
                // it verbatim → `--- /dev/null` → new file); an edit hands
                // old/new text to the frontend to diff.
                let entry = match old {
                    None => serde_json::json!({ "diff": build_new_file_diff(path, new) }),
                    Some(old) => serde_json::json!({ "old_text": old, "new_text": new }),
                };
                changes.insert(path.clone(), entry);
            }
            Some(serde_json::json!({ "changes": changes }).to_string())
        }
    }
}

/// Build a minimal unified diff for a newly created file: the `--- /dev/null`
/// header the frontend's `isAddedFileDiff` keys on, then every line of
/// `new_text` as an addition. Byte-for-byte identical to the frontend `write`
/// op's diff builder (`session-files.ts`), so a multi-file batch's new-file
/// entries render exactly like a single-file creation.
fn build_new_file_diff(path: &str, new_text: &str) -> String {
    // `split('\n')` (not `lines()`) mirrors the frontend `content.split("\n")`:
    // it keeps the trailing empty segment from a final newline, so the `+N`
    // count and the trailing `+` addition line match exactly.
    let lines: Vec<&str> = new_text.split('\n').collect();
    let mut out = format!("--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@", lines.len());
    for line in lines {
        out.push('\n');
        out.push('+');
        out.push_str(line);
    }
    out
}

/// Extract `ContentBlock::Image` payloads from a `ToolCallContent` slice.
/// Returns `None` when no images are present so the upstream `images` field
/// on `AcpEvent::ToolCall(Update)` stays absent for non-image tool calls
/// (preserves replace-on-update semantics: an absent field means "keep
/// prior", a `Some(vec)` replaces).
fn extract_tool_call_images(content: &[ToolCallContent]) -> Option<Vec<ToolCallImageInfo>> {
    let mut imgs: Vec<ToolCallImageInfo> = Vec::new();
    for item in content {
        if let ToolCallContent::Content(c) = item {
            if let ContentBlock::Image(img) = &c.content {
                imgs.push(ToolCallImageInfo {
                    data: img.data.clone(),
                    mime_type: img.mime_type.clone(),
                    uri: img.uri.clone(),
                });
            }
        }
    }
    if imgs.is_empty() {
        None
    } else {
        Some(imgs)
    }
}

/// Extract image payloads from a raw MCP `tools/call` result JSON value.
///
/// Covers the case where the agent puts the MCP `CallToolResult` verbatim into
/// `raw_output` rather than wrapping it in an ACP `ContentBlock::Image`. The
/// MCP spec uses `{"type":"image","data":"<base64>","mimeType":"image/png"}`
/// (camelCase `mimeType`). Both camelCase and snake_case variants are accepted
/// for maximum compatibility.
///
/// Returns `None` when no usable images are found so replace-on-update semantics
/// (absent field ≡ "keep prior") are preserved.
fn extract_images_from_raw_mcp_output(
    raw_output: Option<&serde_json::Value>,
) -> Option<Vec<ToolCallImageInfo>> {
    let content = raw_output?.get("content").and_then(|c| c.as_array())?;
    let imgs: Vec<ToolCallImageInfo> = content
        .iter()
        .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("image"))
        .filter_map(|item| {
            let data = item
                .get("data")
                .and_then(|d| d.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            // Accept both "mimeType" (MCP standard / camelCase) and
            // "mime_type" (snake_case used internally).
            let mime_type = item
                .get("mimeType")
                .or_else(|| item.get("mime_type"))
                .and_then(|m| m.as_str())
                .map(str::trim)
                .filter(|m| !m.is_empty() && m.starts_with("image/"))?;
            let uri = item.get("uri").and_then(|u| u.as_str()).map(str::to_string);
            Some(ToolCallImageInfo {
                data: data.to_string(),
                mime_type: mime_type.to_string(),
                uri,
            })
        })
        .collect();
    if imgs.is_empty() {
        None
    } else {
        Some(imgs)
    }
}

/// If the output looks like numbered lines (`   115→content`), strip them
/// and return `{"start_line":N,"content":"..."}` — same as the historical path.
fn structurize_live_output(text: &str) -> String {
    if let Some(json) = crate::parsers::strip_numbered_lines(text) {
        return json;
    }
    text.to_string()
}

/// Resolve line numbers for live tool call input.
///
/// Resolve line numbers for live tool call input (string form).
///
/// - For apply_patch with bare `@@`: resolve line numbers in place.
/// - For canonical edit JSON: inject `_start_line`.
fn resolve_live_tool_input(text: &str, cwd: Option<&str>) -> String {
    if text.contains("@@\n") || text.contains("@@\r\n") {
        if let Some(resolved) = crate::parsers::resolve_patch_text(text, cwd) {
            return resolved;
        }
    }
    if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(text) {
        if inject_start_line(&mut parsed, cwd) {
            return parsed.to_string();
        }
    }
    text.to_string()
}

/// Try to inject `_start_line` into a JSON object with `file_path` + `old_string`.
/// Returns true if injected.
fn inject_start_line(value: &mut serde_json::Value, cwd: Option<&str>) -> bool {
    let obj = match value.as_object_mut() {
        Some(o) => o,
        None => return false,
    };
    let fp = obj
        .get("file_path")
        .or_else(|| obj.get("path"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let old_str = obj
        .get("old_string")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if let (Some(fp), Some(old_str)) = (fp, old_str) {
        if let Some(sl) = find_string_start_line(&fp, &old_str, cwd) {
            obj.insert("_start_line".to_string(), serde_json::json!(sl));
            return true;
        }
    }
    false
}

/// Find the 1-based start line of `needle` in the file at `path`.
fn find_string_start_line(path: &str, needle: &str, cwd: Option<&str>) -> Option<u64> {
    if needle.is_empty() {
        return None;
    }
    let file_lines = crate::parsers::load_file_lines(path, cwd)?;
    let file_content = file_lines.join("\n");
    let byte_offset = file_content.find(needle)?;
    Some(file_content[..byte_offset].matches('\n').count() as u64 + 1)
}

fn json_value_to_text(val: &Option<serde_json::Value>) -> Option<String> {
    match val {
        Some(serde_json::Value::String(text)) => Some(text.clone()),
        Some(v) if !v.is_null() => Some(v.to_string()),
        _ => None,
    }
}

/// Mirrors `parsers/opencode.rs:425-429` (and `parsers/codebuddy.rs`'s
/// `subagent_type → "Agent"` rewrite) so streaming and reload-from-DB render the
/// same Agent card. The SQLite-side condition is
/// `tool == "task" && state.input.subagent_type IS NOT NULL`, where `tool` is the
/// agent's **internal** tool name. ACP only exposes a user-facing `title` (e.g.
/// "Explore project structure") rather than the internal tool name, so we cannot
/// replicate the `tool == "task"` half of the AND here. We instead anchor on a
/// known sub-agent-capable `agent_type` (OpenCode and CodeBuddy — both surface a
/// description-style title and the standard `{…, subagent_type}` input, and never
/// emit a bare top-level `subagent_type` for anything but a sub-agent) plus the
/// non-empty `subagent_type` string in `raw_input` — together these uniquely
/// identify a sub-agent invocation in practice. Other agents stay excluded to
/// avoid any cross-agent collision a generic `subagent_type` field could cause.
fn is_subagent_invocation(agent_type: AgentType, raw_input: &Option<String>) -> bool {
    if !matches!(agent_type, AgentType::OpenCode | AgentType::CodeBuddy) {
        return false;
    }
    let Some(text) = raw_input.as_deref() else {
        return false;
    };
    // Cheap substring guard avoids parsing large `raw_input` payloads
    // (e.g. prompts with many KB of context) when the field is absent.
    if !text.contains("subagent_type") {
        return false;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    value
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

/// CodeBuddy routes MCP tools through its `DeferExecuteTool` virtualization
/// layer, which surfaces over ACP as a tool call whose `raw_input` wraps the real
/// call as `{ "toolName": "mcp__…", "params": { … } }`. Return that inner
/// `toolName` so the caller can rewrite the live `title` to it — making the
/// frontend resolve the dedicated card (delegation / question / …), mirroring the
/// historical unwrap in `parsers/codebuddy.rs`. `raw_input` is left untouched
/// (the cards peel `params` themselves, and that keeps `inferFromInput` from
/// misclassifying `cancel_delegation`'s `{task_id}` as a generic task).
fn codebuddy_deferred_tool_name(
    agent_type: AgentType,
    raw_input: &Option<String>,
) -> Option<String> {
    if agent_type != AgentType::CodeBuddy {
        return None;
    }
    let text = raw_input.as_deref()?;
    // Cheap substring guard before parsing a potentially large payload.
    if !text.contains("toolName") {
        return None;
    }
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    crate::parsers::codebuddy::deferred_tool_name(&value).map(|s| s.to_string())
}

/// CodeBuddy ships a deferred MCP tool's RESULT as a single re-serialized
/// `{ "type": "text", "text": <inner> }` content part (the OpenAI-Agents content
/// shape), where `<inner>` is the MCP `CallToolResult` content text — for the
/// delegation companion, the compact report / `{ "tasks": [...] }` JSON. The
/// dedicated cards (`parseStatusReport` / `parseToolOutput`) expect that bare
/// inner payload (the content-only host shape they already handle for Claude
/// Code), NOT this wrapper, so a live `get_delegation_status` / `cancel_delegation`
/// poll otherwise renders as raw JSON text. Peel the wrapper to its inner `text`,
/// mirroring the historical `deferred_result_envelope` normalization in
/// `parsers/codebuddy.rs`.
///
/// Gated on CodeBuddy + the exact wrapper shape (`type == "text"` with a string
/// `text`): a non-deferred result (Bash/Read/ToolSearch/…) is never a lone
/// `{type,text}` object, and no delegation report carries a top-level `type`, so
/// those pass through untouched. Unlike the title rewrite, this needs no
/// `raw_input`, so it also normalizes a result-only `ToolCallUpdate` that omits it.
fn unwrap_codebuddy_deferred_output(agent_type: AgentType, text: &str) -> Option<String> {
    if agent_type != AgentType::CodeBuddy {
        return None;
    }
    // Cheap substring guard before parsing a potentially large payload.
    if !text.contains("\"type\"") {
        return None;
    }
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let obj = value.as_object()?;
    if obj.get("type").and_then(|t| t.as_str()) != Some("text") {
        return None;
    }
    obj.get("text").and_then(|t| t.as_str()).map(str::to_string)
}

/// True when a CodeBuddy tool call's ACP `_meta` identifies it as a native
/// sub-agent (`Agent`) invocation. CodeBuddy tags this in `_meta` from the FIRST
/// frame (`codebuddy.ai/toolName == "Agent"`) and later adds
/// `codebuddy.ai/isSubagent` / `subagentType` — whereas the `subagent_type`
/// field in `raw_input` (see `is_subagent_invocation`) only streams in dozens of
/// frames later. Reading the meta lets the title rewrite fire on frame 1, so the
/// Agent pill never spends an opening window classified as a generic tool (and
/// its child tool calls, which carry `codebuddy.ai/parentToolCallId` every frame,
/// nest from the start). Gated on CodeBuddy so the generic `codebuddy.ai/*` keys
/// can never affect another agent.
fn codebuddy_meta_marks_subagent(
    agent_type: AgentType,
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    if agent_type != AgentType::CodeBuddy {
        return false;
    }
    let Some(meta) = meta else {
        return false;
    };
    if meta.get("codebuddy.ai/toolName").and_then(|v| v.as_str()) == Some("Agent") {
        return true;
    }
    if meta
        .get("codebuddy.ai/isSubagent")
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        return true;
    }
    meta.get("codebuddy.ai/subagentType")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// True when a CodeBuddy sub-agent tool call's `_meta` marks it as a BACKGROUND
/// sub-agent (`codebuddy.ai/isBackground == true`). A background sub-agent runs
/// concurrently with the main agent, so the suppression-window invariant (parent
/// blocked → only sub-agent chunks in the window) does NOT hold for it — see
/// `track_subagent_window`, which excludes it from the window. Gated on CodeBuddy.
fn codebuddy_meta_marks_background(
    agent_type: AgentType,
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    if agent_type != AgentType::CodeBuddy {
        return false;
    }
    meta.and_then(|m| m.get("codebuddy.ai/isBackground"))
        .and_then(|v| v.as_bool())
        == Some(true)
}

/// True when a CodeBuddy thought/message `ContentChunk`'s own `_meta` marks the
/// chunk as sub-agent output (`codebuddy.ai/isSubagent`, or a
/// `codebuddy.ai/parentToolCallId` link to the Agent call). This is a precision
/// supplement to the open-sub-agent window — CodeBuddy is not confirmed to
/// populate chunk `_meta`, so suppression never relies on it alone. Gated on
/// CodeBuddy.
fn codebuddy_chunk_marks_subagent(
    agent_type: AgentType,
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    if agent_type != AgentType::CodeBuddy {
        return false;
    }
    let Some(meta) = meta else {
        return false;
    };
    if meta
        .get("codebuddy.ai/isSubagent")
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        return true;
    }
    meta.get("codebuddy.ai/parentToolCallId")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// Whether a live thought/message chunk should be dropped from the top-level
/// stream because it belongs to a CodeBuddy sub-agent (whose work is already
/// represented by the Agent pill + its nested tool calls). Matches Claude Code,
/// which never streams a sub-agent's internal reasoning onto the main session.
///
/// Suppress while we're inside an open sub-agent window OR when the chunk's own
/// meta marks it. The window safety rests on a structural invariant: the window
/// only ever holds FOREGROUND (blocking) sub-agents — a synchronous `Agent` tool
/// call suspends the parent model until the tool returns its result, so between
/// that call's open frame and its terminal frame the main session carries ONLY
/// the sub-agent's chunks, never main-agent output. BACKGROUND sub-agents (which
/// run concurrently and could interleave main-agent output) are deliberately
/// excluded from the window by `track_subagent_window`, so `window_open` can
/// never cause a main-agent chunk to be dropped. Gated on CodeBuddy; every other
/// agent always emits.
fn should_suppress_subagent_chunk(
    agent_type: AgentType,
    window_open: bool,
    chunk_meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    if agent_type != AgentType::CodeBuddy {
        return false;
    }
    window_open || codebuddy_chunk_marks_subagent(agent_type, chunk_meta)
}

/// Maintain the set of OPEN CodeBuddy sub-agent tool calls (`open`). `is_agent`
/// is true once `resolve_rewritten_title` classified this `tool_call_id` as a
/// native sub-agent (`"agent"`). A non-final status opens the window; a final
/// status (`completed` / `failed`) closes it and records the id in `closed`, so a
/// stray late non-final frame can't re-open an already-finished sub-agent.
///
/// `is_background` (from `codebuddy_meta_marks_background`) EXCLUDES a sub-agent
/// from the window: a background sub-agent runs concurrently with the main agent,
/// so the "window holds only sub-agent chunks" invariant that makes
/// `should_suppress_subagent_chunk` safe would not hold. We treat a background
/// marker exactly like a terminal frame (remove + record closed) so it can never
/// suppress interleaved main-agent output. (`isBackground` can stream in a frame
/// or two after the call opens, so a background sub-agent's earliest chunks may be
/// briefly suppressed before the marker arrives — an accepted, rare imperfection;
/// the user-reported case is foreground, where the marker is `false`.)
///
/// Gated on CodeBuddy so a single-agent-type connection of any other agent stays
/// inert.
fn track_subagent_window(
    agent_type: AgentType,
    is_agent: bool,
    is_background: bool,
    status: Option<&str>,
    tool_call_id: &str,
    open: &mut HashSet<String>,
    closed: &mut HashSet<String>,
) {
    if agent_type != AgentType::CodeBuddy || !is_agent {
        return;
    }
    let is_final = matches!(status, Some("completed") | Some("failed"));
    if is_final || is_background {
        open.remove(tool_call_id);
        closed.insert(tool_call_id.to_string());
    } else if !closed.contains(tool_call_id) {
        open.insert(tool_call_id.to_string());
    }
}

/// Per-session CodeBuddy live-stream state threaded through
/// `emit_conversation_update`. Consolidates the authoritative title rewrites and
/// the open-sub-agent suppression window so CodeBuddy's sparse, multi-frame
/// sub-agent stream resolves to a stable Agent pill (whose children nest) with
/// its interleaved thought/message chunks suppressed. Created per connection,
/// shared across the idle and active-turn loops; the historical-replay path uses
/// a throwaway instance. Mirrors `ToolCallOutputCache`'s lifetime.
#[derive(Default)]
struct CodeBuddyLiveState {
    /// tool_call_id → authoritative title: `"agent"` for a native sub-agent, or
    /// the inner `mcp__…` name for a `DeferExecuteTool` MCP call. Re-asserted on
    /// every later frame so a status-only update can't downgrade the card.
    title_overrides: HashMap<String, String>,
    /// Sub-agent tool calls currently OPEN (classified, not yet completed/failed).
    /// While non-empty, interleaved thought/message chunks belong to a sub-agent
    /// and are suppressed from the top-level stream (matching Claude Code).
    open_subagents: HashSet<String>,
    /// Sub-agent tool calls that already reached a final status — guards against a
    /// stray late non-final frame re-opening a finished sub-agent.
    closed_subagents: HashSet<String>,
    /// Objective of the Codex `/goal` run currently open on this connection (set
    /// by the latest `active` `session_info_update` goal, cleared on any terminal
    /// status). Lets a later `goal:null` close the run by objective — and be a
    /// no-op when no run is open. See `crate::acp::codex_goal::next_goal_marker`.
    ///
    /// This lives here (not in `SessionState`) because `CodeBuddyLiveState` and
    /// `SessionState` share one lifetime: a browser refresh / reconnect re-attaches
    /// to the *running* connection (`find_connection_for_reuse`), keeping both; a
    /// brand-new connection resets both together (empty live blocks + fresh state).
    /// So this state never resets while goal blocks it would close still exist.
    codex_open_goal: Option<String>,
    /// Monotonic per-connection counter for synthetic goal tool-call ids. Occurrence
    /// (not content) addressing keeps two runs that share an objective from
    /// colliding in the reducer's id-keyed live block list.
    codex_goal_seq: u64,
    /// Cursor announces MCP calls as `MCP: tool` before their identity exists
    /// and never repeats the title/input. Keep only those ids eligible for the
    /// bounded completion-result recovery below.
    cursor_generic_mcp_ids: HashSet<String>,
}

const CURSOR_IDENTITYLESS_MCP_TITLE: &str = "MCP: tool";

struct CursorToolSnapshot<'a> {
    agent_type: AgentType,
    tool_call_id: &'a str,
    title: Option<&'a str>,
    status: Option<&'a str>,
    content: &'a mut Option<String>,
    raw_output: Option<&'a str>,
}

impl CodeBuddyLiveState {
    fn recover_cursor_delegation(&mut self, snapshot: CursorToolSnapshot<'_>) -> Option<String> {
        if snapshot.agent_type != AgentType::Cursor {
            return None;
        }
        if snapshot.title == Some(CURSOR_IDENTITYLESS_MCP_TITLE) {
            self.cursor_generic_mcp_ids
                .insert(snapshot.tool_call_id.to_string());
        }
        if !matches!(snapshot.status, Some("completed" | "failed")) {
            return None;
        }
        let eligible = self.cursor_generic_mcp_ids.remove(snapshot.tool_call_id);
        if !eligible {
            return None;
        }
        let task_id = crate::acp::delegation::broker::delegation_ack_task_id(
            snapshot.content.as_deref(),
            snapshot.raw_output,
        )?;
        *snapshot.content = Some(format!(
            "{}{}{}",
            crate::acp::delegation::broker::DELEGATION_ACK_PREFIX,
            task_id,
            crate::acp::delegation::broker::DELEGATION_ACK_SUFFIX,
        ));
        let title = "delegate_to_agent".to_string();
        self.title_overrides
            .insert(snapshot.tool_call_id.to_string(), title.clone());
        Some(title)
    }
}

/// Resolve a tool call's title, honoring an authoritative rewrite recorded for
/// the session in `overrides` (tool_call_id → resolved title).
///
/// Returns `Some(name)` when this event identifies a built-in capability gateway
/// invocation, a CodeBuddy `DeferExecuteTool`, or a sub-agent invocation —
/// recording it — OR when a PRIOR event already classified this `tool_call_id`
/// and this event lost the marker (the override is re-asserted).
/// Returns `None` only when the call was never classified, so the caller falls
/// back to the event's own title.
///
/// Sub-agent detection fires on EITHER `raw_input.subagent_type`
/// (`is_subagent_invocation`) OR `meta_marks_subagent` — the precomputed
/// `codebuddy_meta_marks_subagent` result. The meta signal is what makes the pill
/// stable: CodeBuddy carries `codebuddy.ai/toolName == "Agent"` from the very
/// first frame, whereas `subagent_type` only reaches `raw_input` dozens of frames
/// later, so meta-first detection records the override immediately and every
/// later (sparse) frame re-asserts it.
///
/// The re-assertion is the fix for CodeBuddy's status-only `ToolCallUpdate`s:
/// they arrive without the original `subagent_type`/`toolName` payload but WITH
/// the agent's raw (non-agent) title. Without it the frontend
/// (`inferLiveToolName` → `getToolName`) downgrades the Agent / delegation card
/// back to a generic tool call mid-stream — which also un-nests its children.
/// `on_update` only tunes the (PII-safe, id-only) trace wording.
fn resolve_rewritten_title(
    agent_type: AgentType,
    raw_input: &Option<String>,
    tool_call_id: &str,
    on_update: bool,
    meta_marks_subagent: bool,
    overrides: &mut HashMap<String, String>,
) -> Option<String> {
    match crate::acp::builtin_mcp::invoked_tool_name(raw_input) {
        crate::acp::builtin_mcp::GatewayToolIdentity::Resolved(logical_name) => {
            tracing::info!(
                "[ACP][{agent_type}] restored built-in gateway tool identity (tool_call_id={tool_call_id}, on_update={on_update})"
            );
            overrides.insert(tool_call_id.to_string(), logical_name.clone());
            return Some(logical_name);
        }
        crate::acp::builtin_mcp::GatewayToolIdentity::UnresolvedGateway => {
            if let Some(logical_name) = overrides.get(tool_call_id) {
                return Some(logical_name.clone());
            }
        }
        crate::acp::builtin_mcp::GatewayToolIdentity::NotGateway => {}
    }
    if let Some(inner) = codebuddy_deferred_tool_name(agent_type, raw_input) {
        tracing::info!(
            "[ACP][{agent_type}] unwrapped DeferExecuteTool to its real MCP tool (tool_call_id={tool_call_id}, on_update={on_update})"
        );
        overrides.insert(tool_call_id.to_string(), inner.clone());
        return Some(inner);
    }
    if is_subagent_invocation(agent_type, raw_input) || meta_marks_subagent {
        tracing::info!(
            "[ACP][{agent_type}] subagent detected, rewrote tool title to 'agent' (tool_call_id={tool_call_id}, on_update={on_update})"
        );
        overrides.insert(tool_call_id.to_string(), "agent".to_string());
        return Some("agent".to_string());
    }
    overrides.get(tool_call_id).cloned()
}

fn map_plan_priority(priority: &PlanEntryPriority) -> String {
    match priority {
        PlanEntryPriority::High => "high",
        PlanEntryPriority::Medium => "medium",
        PlanEntryPriority::Low => "low",
        _ => "unknown",
    }
    .to_string()
}

fn map_plan_status(status: &PlanEntryStatus) -> String {
    match status {
        PlanEntryStatus::Pending => "pending",
        PlanEntryStatus::InProgress => "in_progress",
        PlanEntryStatus::Completed => "completed",
        _ => "unknown",
    }
    .to_string()
}

fn map_plan_entries(plan: &Plan) -> Vec<PlanEntryInfo> {
    plan.entries
        .iter()
        .map(|entry| PlanEntryInfo {
            content: entry.content.clone(),
            priority: map_plan_priority(&entry.priority),
            status: map_plan_status(&entry.status),
        })
        .collect()
}

fn parse_claude_sdk_message_params(
    params: &serde_json::Value,
) -> Option<(String, serde_json::Value)> {
    let obj = params.as_object()?;
    let session_id = obj.get("sessionId")?.as_str()?.to_string();
    let message = obj.get("message")?.clone();
    Some((session_id, message))
}

fn is_claude_api_retry_message(message: &serde_json::Value) -> bool {
    let obj = match message.as_object() {
        Some(obj) => obj,
        None => return false,
    };
    let message_type = obj.get("type").and_then(|v| v.as_str());
    let message_subtype = obj.get("subtype").and_then(|v| v.as_str());
    matches!(message_type, Some("system")) && matches!(message_subtype, Some("api_retry"))
}

fn map_claude_sdk_ext_notification(notification: &UntypedMessage) -> Option<AcpEvent> {
    if notification.method() != "_claude/sdkMessage" {
        return None;
    }

    let (session_id, message) = parse_claude_sdk_message_params(notification.params())?;
    if !is_claude_api_retry_message(&message) {
        return None;
    }
    Some(AcpEvent::ClaudeSdkMessage {
        session_id,
        message,
    })
}

async fn maybe_emit_claude_sdk_ext_notification(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    dispatch: Dispatch,
) {
    let Dispatch::Notification(notification) = dispatch else {
        return;
    };

    if let Some(event) = map_claude_sdk_ext_notification(&notification) {
        emit_with_state(state, emitter, event).await;
    }
}

/// Fix null fields in `usage_update` notifications that would otherwise fail deserialization.
///
/// Some ACP agents send `"used": null` in usage_update notifications, but the
/// upstream schema expects `u64`. This function patches the raw JSON params
/// so that `null` numeric fields default to `0`.
fn fix_usage_update_nulls(mut dispatch: Dispatch) -> Dispatch {
    if let Dispatch::Notification(ref mut msg) = dispatch {
        if let Some(update) = msg.params.get_mut("update") {
            if update.get("sessionUpdate").and_then(|v| v.as_str()) == Some("usage_update") {
                if update.get("used").map(|v| v.is_null()).unwrap_or(false) {
                    update["used"] = serde_json::Value::from(0u64);
                }
                if update.get("size").map(|v| v.is_null()).unwrap_or(false) {
                    update["size"] = serde_json::Value::from(0u64);
                }
            }
        }
    }
    dispatch
}

/// Convert a SessionUpdate into AcpEvent(s) and emit to frontend.
///
/// `raw_output_cache` is a per-session cache used to detect cumulative
/// snapshots from agents and convert them into incremental deltas so the
/// event pipeline never carries a full N-MB tool output more than once.
///
/// `cb_state` is the per-session `CodeBuddyLiveState`: the authoritative
/// title-rewrite map (so a status-only update can't downgrade an Agent /
/// delegation card and un-nest its children) plus the open-sub-agent window used
/// to suppress a CodeBuddy sub-agent's interleaved thought/message chunks.
/// Mirrors `raw_output_cache`'s lifetime.
async fn emit_conversation_update(
    state: &Arc<RwLock<SessionState>>,
    emitter: &EventEmitter,
    agent_type: AgentType,
    update: SessionUpdate,
    cwd: Option<&str>,
    raw_output_cache: &mut ToolCallOutputCache,
    cb_state: &mut CodeBuddyLiveState,
) {
    match update {
        SessionUpdate::UserMessageChunk(_) => {
            // User echo chunks are informational for transcript sync and
            // currently not rendered in live ACP UI.
        }
        SessionUpdate::AgentMessageChunk(ContentChunk {
            content: ContentBlock::Text(text),
            meta,
            ..
        }) => {
            if let Some(error_kind) = agent_runtime_error_kind(agent_type, &text.text) {
                tracing::error!(
                    agent_type = %agent_type,
                    error_kind,
                    error = %safe_error_detail(&text.text),
                    "[ACP] agent runtime error"
                );
            }
            // Drop a CodeBuddy sub-agent's interleaved message text — it belongs
            // to the Agent pill, not the main thread (see
            // `should_suppress_subagent_chunk`). No-op for every other agent.
            if !is_agent_runtime_diagnostic(agent_type, &text.text)
                && !should_suppress_subagent_chunk(
                    agent_type,
                    !cb_state.open_subagents.is_empty(),
                    meta.as_ref(),
                )
            {
                emit_with_state(state, emitter, AcpEvent::ContentDelta { text: text.text }).await;
            }
        }
        SessionUpdate::AgentMessageChunk(_) => {
            // Non-text chunks are currently not surfaced in live streaming UI.
        }
        SessionUpdate::AgentThoughtChunk(ContentChunk {
            content: ContentBlock::Text(text),
            meta,
            ..
        }) => {
            // Same suppression for a sub-agent's interleaved reasoning.
            if !should_suppress_subagent_chunk(
                agent_type,
                !cb_state.open_subagents.is_empty(),
                meta.as_ref(),
            ) {
                emit_with_state(state, emitter, AcpEvent::Thinking { text: text.text }).await;
            }
        }
        SessionUpdate::AgentThoughtChunk(_) => {
            // Non-text thought chunks are currently ignored.
        }
        SessionUpdate::ToolCall(tc) => {
            let tool_call_id = tc.tool_call_id.to_string();
            // CodeBuddy double-wraps a deferred MCP result as a `{type,text}`
            // content part; peel it (in both the content and raw_output channels)
            // so the dedicated delegation cards parse it instead of showing raw JSON.
            // codex-acp reports file edits as a `Diff` content block with no
            // `raw_input`; synthesize a canonical edit so the call classifies/
            // renders as an edit instead of a tool named after the raw diff
            // header (see synthesize_edit_input_from_diffs). When we do, drop the
            // `Diff` from `content` — it's the same edit re-serialized hunklessly,
            // which would otherwise double the event and skew the header +/- stats.
            // Blank raw_input is treated as absent (matches the frontend guard).
            let grok_use_tool = (agent_type == AgentType::Grok)
                .then(|| crate::acp::grok::unwrap_use_tool(tc.raw_input.as_ref()))
                .flatten();
            let own_raw_input = match &grok_use_tool {
                Some((_, input)) => {
                    json_value_to_text(&Some(input.clone())).filter(|text| !text.trim().is_empty())
                }
                None => json_value_to_text(&tc.raw_input).filter(|text| !text.trim().is_empty()),
            };
            let synthesized_edit = if own_raw_input.is_none() {
                synthesize_edit_input_from_diffs(&tc.content)
            } else {
                None
            };
            let mut content = serialize_tool_call_content(&tc.content, synthesized_edit.is_none())
                .map(|c| unwrap_codebuddy_deferred_output(agent_type, &c).unwrap_or(c));
            // Prefer structured content images; fall back to parsing the raw MCP
            // `tools/call` result in `raw_output` for agents that surface the
            // MCP {"type":"image","data":"...","mimeType":"..."} payload there
            // instead of wrapping it in an ACP ContentBlock::Image.
            let images = extract_tool_call_images(&tc.content)
                .or_else(|| extract_images_from_raw_mcp_output(tc.raw_output.as_ref()));
            let raw_input = synthesized_edit
                .or(own_raw_input)
                .map(|text| resolve_live_tool_input(&text, cwd));
            // Initial tool_call notification — the frontend reducer
            // treats `raw_output` as a full replacement, so we bypass
            // the diff path and seed the cache with the current snapshot.
            let raw_output_text = if agent_type == AgentType::Grok {
                crate::acp::grok::live_tool_output(&content, &tc.raw_output)
            } else {
                json_value_to_text(&tc.raw_output)
                    .map(|text| unwrap_codebuddy_deferred_output(agent_type, &text).unwrap_or(text))
                    .map(|text| structurize_live_output(&text))
            };
            let raw_output = raw_output_text
                .as_deref()
                .and_then(|text| raw_output_cache.seed(&tool_call_id, text));
            let locations = if tc.locations.is_empty() {
                None
            } else {
                serde_json::to_value(&tc.locations).ok()
            };
            // Read the CodeBuddy sub-agent markers from `_meta` BEFORE it's moved
            // into the emitted `Value` below — `meta_marks_subagent` is the early,
            // reliable signal (frame 1) that keeps the Agent pill from flickering;
            // `meta_marks_background` keeps a concurrent sub-agent out of the
            // suppression window (see fn docs).
            let meta_marks_subagent = codebuddy_meta_marks_subagent(agent_type, tc.meta.as_ref());
            let meta_marks_background =
                codebuddy_meta_marks_background(agent_type, tc.meta.as_ref());
            let meta = tc.meta.map(serde_json::Value::Object);
            let status = format!("{:?}", tc.status).to_lowercase();
            raw_output_cache.remove_if_final(&tool_call_id, Some(status.as_str()));
            // Avoid logging titles/payloads below — they can be model-generated
            // user task descriptions (PII-adjacent) and would create noise in
            // server-mode log sinks. The opaque tool_call_id is enough to
            // correlate these events with downstream traces.
            if let Some((name, _)) = &grok_use_tool {
                cb_state
                    .title_overrides
                    .insert(tool_call_id.clone(), name.clone());
            }
            // Resolve (and record) any authoritative title rewrite so a later
            // status-only update can't downgrade this card (see fn doc).
            let mut title = resolve_rewritten_title(
                agent_type,
                &raw_input,
                &tool_call_id,
                false,
                meta_marks_subagent,
                &mut cb_state.title_overrides,
            )
            .unwrap_or(tc.title);
            if let Some(recovered) = cb_state.recover_cursor_delegation(CursorToolSnapshot {
                agent_type,
                tool_call_id: &tool_call_id,
                title: Some(&title),
                status: Some(status.as_str()),
                content: &mut content,
                raw_output: raw_output_text.as_deref(),
            }) {
                title = recovered;
            }
            // Open/close the sub-agent suppression window for this call. `title ==
            // "agent"` iff this is a classified native sub-agent (DeferExecuteTool
            // rewrites to an `mcp__…` name, never "agent").
            track_subagent_window(
                agent_type,
                title == "agent",
                meta_marks_background,
                Some(status.as_str()),
                &tool_call_id,
                &mut cb_state.open_subagents,
                &mut cb_state.closed_subagents,
            );
            emit_with_state(
                state,
                emitter,
                AcpEvent::ToolCall {
                    tool_call_id,
                    title,
                    kind: format!("{:?}", tc.kind).to_lowercase(),
                    status,
                    content,
                    raw_input,
                    raw_output,
                    locations,
                    meta,
                    images,
                },
            )
            .await;
        }
        SessionUpdate::ToolCallUpdate(tcu) => {
            let tool_call_id = tcu.tool_call_id.to_string();
            // Peel CodeBuddy's `{type,text}` deferred-MCP wrapper here too — the
            // result often arrives on an update (see raw_output below).
            // Same Diff→canonical-edit hoist as the initial ToolCall path: the
            // edit may first arrive on an update. Drop the redundant Diff from
            // `content` when hoisted. The reducer preserves a prior raw_input on
            // status-only updates (`action.raw_input ?? block.info.raw_input`).
            let grok_use_tool = (agent_type == AgentType::Grok)
                .then(|| crate::acp::grok::unwrap_use_tool(tcu.fields.raw_input.as_ref()))
                .flatten();
            let own_raw_input =
                match &grok_use_tool {
                    Some((_, input)) => json_value_to_text(&Some(input.clone()))
                        .filter(|text| !text.trim().is_empty()),
                    None => json_value_to_text(&tcu.fields.raw_input)
                        .filter(|text| !text.trim().is_empty()),
                };
            let synthesized_edit = if own_raw_input.is_none() {
                tcu.fields
                    .content
                    .as_deref()
                    .and_then(synthesize_edit_input_from_diffs)
            } else {
                None
            };
            let mut content = tcu
                .fields
                .content
                .as_deref()
                .and_then(|c| serialize_tool_call_content(c, synthesized_edit.is_none()))
                .map(|c| unwrap_codebuddy_deferred_output(agent_type, &c).unwrap_or(c));
            let images = tcu
                .fields
                .content
                .as_deref()
                .and_then(extract_tool_call_images);
            let raw_input = synthesized_edit
                .or(own_raw_input)
                .map(|text| resolve_live_tool_input(&text, cwd));
            // Diff the incoming raw_output against the last snapshot we
            // emitted for this tool call. This turns cumulative snapshots
            // from agents (Claude Code, Codex, …) into incremental deltas
            // with `raw_output_append=true`, collapsing the O(N²) transfer
            // problem to O(N) while capping any single emitted chunk to
            // MAX_SINGLE_EMIT_BYTES.
            let raw_output_text = if agent_type == AgentType::Grok {
                crate::acp::grok::live_tool_output(&content, &tcu.fields.raw_output)
            } else {
                json_value_to_text(&tcu.fields.raw_output)
                    .map(|text| unwrap_codebuddy_deferred_output(agent_type, &text).unwrap_or(text))
                    .map(|text| structurize_live_output(&text))
            };
            let (raw_output, raw_output_append) = match raw_output_text.as_deref() {
                Some(text) => match raw_output_cache.consume(&tool_call_id, text) {
                    Some((payload, append)) => (Some(payload), Some(append)),
                    None => (None, None),
                },
                None => (None, None),
            };
            let locations = tcu
                .fields
                .locations
                .as_ref()
                .filter(|l| !l.is_empty())
                .and_then(|l| serde_json::to_value(l).ok());
            let meta_marks_subagent = codebuddy_meta_marks_subagent(agent_type, tcu.meta.as_ref());
            let meta_marks_background =
                codebuddy_meta_marks_background(agent_type, tcu.meta.as_ref());
            let meta = tcu.meta.clone().map(serde_json::Value::Object);
            let status = tcu.fields.status.map(|s| format!("{:?}", s).to_lowercase());
            raw_output_cache.remove_if_final(&tool_call_id, status.as_deref());
            if let Some((name, _)) = &grok_use_tool {
                cb_state
                    .title_overrides
                    .insert(tool_call_id.clone(), name.clone());
            }
            // Re-assert any authoritative title rewrite (see fn doc): an update
            // that carries the subagent/deferred marker classifies (and records)
            // the card, and — the key fix — a later status-only update that LOST
            // the marker but carries the agent's raw (non-agent) title still
            // resolves to the recorded override, so the Agent/delegation card and
            // its child nesting (`getToolName === "agent"`) don't revert to a
            // generic tool call mid-stream. Falls back to the event's own title
            // for never-classified tool calls.
            let mut title = resolve_rewritten_title(
                agent_type,
                &raw_input,
                &tool_call_id,
                true,
                meta_marks_subagent,
                &mut cb_state.title_overrides,
            )
            .or(tcu.fields.title);
            if let Some(recovered) = cb_state.recover_cursor_delegation(CursorToolSnapshot {
                agent_type,
                tool_call_id: &tool_call_id,
                title: title.as_deref(),
                status: status.as_deref(),
                content: &mut content,
                raw_output: raw_output_text.as_deref(),
            }) {
                title = Some(recovered);
            }
            // Keep/close the sub-agent suppression window by status (an update
            // resolving to "agent" is a classified native sub-agent).
            track_subagent_window(
                agent_type,
                title.as_deref() == Some("agent"),
                meta_marks_background,
                status.as_deref(),
                &tool_call_id,
                &mut cb_state.open_subagents,
                &mut cb_state.closed_subagents,
            );
            emit_with_state(
                state,
                emitter,
                AcpEvent::ToolCallUpdate {
                    tool_call_id,
                    title,
                    status,
                    content,
                    raw_input,
                    raw_output,
                    raw_output_append,
                    locations,
                    meta,
                    images,
                },
            )
            .await;
        }
        SessionUpdate::CurrentModeUpdate(update) => {
            emit_with_state(
                state,
                emitter,
                AcpEvent::ModeChanged {
                    mode_id: update.current_mode_id.to_string(),
                },
            )
            .await;
        }
        SessionUpdate::Plan(plan) => {
            emit_with_state(
                state,
                emitter,
                AcpEvent::PlanUpdate {
                    entries: map_plan_entries(&plan),
                },
            )
            .await;
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            emit_session_config_options_values(state, emitter, agent_type, update.config_options)
                .await;
        }
        SessionUpdate::AvailableCommandsUpdate(update) => {
            // Some agents (e.g. Claude Code with overlapping user/project slash
            // commands) emit duplicate entries sharing the same name. Keep the
            // first occurrence so downstream consumers don't render duplicates;
            // the frontend reducer also dedupes as a defensive measure.
            let mut seen = HashSet::new();
            let commands: Vec<AvailableCommandInfo> = update
                .available_commands
                .iter()
                .filter(|cmd| seen.insert(cmd.name.clone()))
                .map(|cmd| {
                    let input_hint = cmd.input.as_ref().map(|input| match input {
                        sacp::schema::AvailableCommandInput::Unstructured(u) => u.hint.clone(),
                        _ => String::new(),
                    });
                    AvailableCommandInfo {
                        name: cmd.name.clone(),
                        description: cmd.description.clone(),
                        input_hint,
                    }
                })
                .collect();
            emit_with_state(state, emitter, AcpEvent::AvailableCommands { commands }).await;
        }
        SessionUpdate::UsageUpdate(update) => {
            emit_with_state(
                state,
                emitter,
                AcpEvent::UsageUpdate {
                    used: update.used,
                    size: update.size,
                },
            )
            .await;
        }
        SessionUpdate::SessionInfoUpdate(info) => {
            if let Some(title) = info
                .title
                .value()
                .map(String::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
            {
                emit_with_state(
                    state,
                    emitter,
                    AcpEvent::SessionTitleUpdated {
                        title: title.to_string(),
                    },
                )
                .await;
            }

            // codex-acp v1.1.0 (#263) reports `/goal` transitions as structured
            // session metadata instead of live "Goal updated (…)" agent text:
            // the goal object rides under `_meta.codex.goal`. Map it onto iyw-claw's
            // canonical create_goal/update_goal synthetic tool call so the
            // existing goal-card pipeline (groupGoalRuns/GoalCard) renders it —
            // byte-identical to the history path (parsers/codex.rs). Non-Codex
            // agents don't populate the `codex` key, so this is a no-op for them.
            // Session titles are handled generically above; this branch remains
            // Codex-specific because only Codex populates `_meta.codex.goal`.
            if let Some(goal) = info
                .meta
                .as_ref()
                .and_then(|m| m.get("codex"))
                .and_then(|codex| codex.get("goal"))
            {
                if let Some(marker) =
                    crate::acp::codex_goal::next_goal_marker(&mut cb_state.codex_open_goal, goal)
                {
                    cb_state.codex_goal_seq += 1;
                    let tool_call_id =
                        crate::acp::codex_goal::goal_tool_call_id(cb_state.codex_goal_seq);
                    emit_with_state(
                        state,
                        emitter,
                        AcpEvent::ToolCall {
                            tool_call_id,
                            title: marker.title,
                            kind: "other".to_string(),
                            status: "completed".to_string(),
                            content: None,
                            raw_input: Some(marker.input_json),
                            raw_output: Some(marker.output_json),
                            locations: None,
                            meta: None,
                            images: None,
                        },
                    )
                    .await;
                }
            }

            if let Some(record) = crate::acp::session_failure::from_air_meta(info.meta.as_ref()) {
                emit_with_state(state, emitter, AcpEvent::SessionFailure { record }).await;
            }
        }
        other => {
            tracing::info!(
                target: "acp.session",
                update_kind = session_update_kind(&other),
                "Unhandled ACP SessionUpdate"
            );
        }
    }
}

fn session_update_kind(update: &SessionUpdate) -> &'static str {
    match update {
        SessionUpdate::UserMessageChunk(_) => "user_message_chunk",
        SessionUpdate::AgentMessageChunk(_) => "agent_message_chunk",
        SessionUpdate::AgentThoughtChunk(_) => "agent_thought_chunk",
        SessionUpdate::ToolCall(_) => "tool_call",
        SessionUpdate::ToolCallUpdate(_) => "tool_call_update",
        SessionUpdate::Plan(_) => "plan",
        SessionUpdate::AvailableCommandsUpdate(_) => "available_commands_update",
        SessionUpdate::CurrentModeUpdate(_) => "current_mode_update",
        SessionUpdate::ConfigOptionUpdate(_) => "config_option_update",
        SessionUpdate::SessionInfoUpdate(_) => "session_info_update",
        SessionUpdate::UsageUpdate(_) => "usage_update",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::delegation::companion::CompanionFeatures;

    /// `show_image` is the one companion tool with no settings toggle, so
    /// `companion_features_arg` hard-codes `images`. Assert it survives with
    /// every optional group off: if a refactor ever makes `images`
    /// conditional, `show_image` drops out of `tools/list` and every image
    /// skill silently loses inline rendering with no error to trace.
    #[test]
    fn images_stays_on_with_every_optional_group_off() {
        assert_eq!(
            companion_features_arg(false, false, false, false, false),
            "images"
        );
    }

    /// Round-trip the emitted string through the companion's own parser. A
    /// token typo on either side (`image` vs `images`) would leave the
    /// `--features` arg looking plausible while `allows_tool("show_image")`
    /// silently returns false.
    #[test]
    fn emitted_features_parse_back_into_show_image_access() {
        for flag in [false, true] {
            let features = companion_features_arg(flag, flag, flag, flag, flag);
            let parsed = CompanionFeatures::parse(Some(&features));
            assert!(parsed.images, "images lost in round-trip for {features:?}");
            assert!(
                parsed.allows_tool("show_image"),
                "show_image gated off for {features:?}"
            );
        }
    }

    /// Each settings-gated group reaches the tool it owns, in both
    /// directions. `memory-proposal` is appended by the caller rather than
    /// this function, so it stays out of scope here.
    #[test]
    fn optional_groups_track_their_tools() {
        let gated = [
            "delegate_to_agent",
            "check_user_feedback",
            "ask_user_question",
            "get_session_info",
            "append_user_memory",
        ];

        let all =
            CompanionFeatures::parse(Some(&companion_features_arg(true, true, true, true, true)));
        for tool in gated {
            assert!(all.allows_tool(tool), "{tool} missing when all flags on");
        }

        let none = CompanionFeatures::parse(Some(&companion_features_arg(
            false, false, false, false, false,
        )));
        for tool in gated {
            assert!(!none.allows_tool(tool), "{tool} exposed when all flags off");
        }
    }
}
