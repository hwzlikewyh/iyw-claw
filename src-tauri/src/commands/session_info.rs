//! `get_session_info` backing logic + settings persistence.
//!
//! Two surfaces live here, mirroring `crate::commands::question`:
//!
//!   * [`DbSessionInfoLookup`] — the production [`SessionInfoAccess`] impl the
//!     delegation listener calls to resolve a referenced session
//!     (`iyw-claw://session/<id>`) into metadata + token stats and, on demand, a
//!     bounded compacted view of its recent messages. It reuses
//!     [`get_folder_conversation_core`] (which reads the conversation row, uses
//!     its bound `external_id` + `agent_type` to pick the right parser, and parses
//!     the on-disk transcript off the runtime via `spawn_blocking`).
//!   * The `session_info.enabled` settings knob (**default true**) — read at MCP
//!     injection time via [`SessionInfoRuntimeConfig`]. Persist + apply + broadcast
//!     follows the exact shape of the ask-question / feedback toggles.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::acp::session_info::{
    SessionInfo, SessionInfoAccess, SessionInfoConfig, SessionInfoRuntimeConfig,
    SessionMessageItem, SessionMessages, MAX_SESSION_MESSAGES,
};
use crate::app_error::AppCommandError;
use crate::commands::conversations::get_folder_conversation_core;
use crate::db::service::{app_metadata_service, conversation_service, folder_service};
use crate::db::AppDatabase;
use crate::models::agent::AgentType;
use crate::models::message::{ContentBlock, MessageTurn, TurnRole};
use crate::web::event_bridge::{emit_event, EventEmitter, SESSION_INFO_SETTINGS_CHANGED_EVENT};

/// Upper bound on a single compacted turn's text. Keeps one chatty turn from
/// dominating the budget; longer text is truncated with an ellipsis marker.
const PER_TURN_CHARS: usize = 1_500;

/// Overall character budget across all included turns (text AND tool names).
/// Walked newest-first so the most recent context survives; older turns are
/// dropped once this is exhausted.
const OVERALL_CHARS: usize = 16_000;

/// Per-turn caps on the collected tool names, so a turn with thousands of
/// `ToolUse` blocks (or one with a pathologically long tool name) can't inflate
/// the payload past the text budget. Tool-name chars are ALSO charged against
/// [`OVERALL_CHARS`].
const MAX_TOOLS_PER_TURN: usize = 16;
const MAX_TOOL_NAME_CHARS: usize = 64;

/// How long the transcript parse may run before the resolver gives up and returns
/// metadata only. The session keeps existing; the agent simply gets no messages
/// this call (with a `note` explaining why). Generous for large session files,
/// bounded so a pathological file can't pin the round-trip.
///
/// NOTE: this bounds the *response latency*, not the underlying work — the parse
/// runs on tokio's blocking pool (`spawn_blocking` inside
/// `get_folder_conversation_core`), which cannot be canceled, so a timed-out parse
/// keeps running to completion in the background. [`MAX_CONCURRENT_PARSES`] is what
/// actually protects the blocking pool from a flood of large-session reads.
const PARSE_TIMEOUT: Duration = Duration::from_secs(8);

/// Cap on concurrent transcript parses across all `get_session_info` calls. A
/// small bound keeps a burst of reads on large sessions from saturating tokio's
/// blocking pool (each parse pins one blocking thread, uncancelable). Excess
/// concurrent callers do NOT queue — `bounded_parse` uses a non-blocking
/// `try_acquire`, so a call that finds every slot taken returns `Busy` and
/// degrades to metadata-only immediately.
const MAX_CONCURRENT_PARSES: usize = 4;

/// Production [`SessionInfoAccess`]: resolves a session by iyw-claw conversation id
/// against the DB + on-disk transcript. Wraps an `Arc<AppDatabase>` (like
/// `DbDepthLookup` / `DbChildStatusLookup`) plus a semaphore bounding concurrent
/// transcript parses. Construct via [`DbSessionInfoLookup::new`].
pub struct DbSessionInfoLookup {
    pub db: Arc<AppDatabase>,
    /// Limits concurrent `get_folder_conversation_core` parses (see
    /// [`MAX_CONCURRENT_PARSES`]).
    parse_limit: Arc<tokio::sync::Semaphore>,
}

impl DbSessionInfoLookup {
    pub fn new(db: Arc<AppDatabase>) -> Self {
        Self {
            db,
            parse_limit: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PARSES)),
        }
    }
}

#[async_trait]
impl SessionInfoAccess for DbSessionInfoLookup {
    async fn resolve(&self, session_id: i32, max_messages: u32) -> SessionInfo {
        let conn = &self.db.conn;
        // get_by_id is the authoritative existence + metadata source (cheap PK
        // lookup, already filters soft-deleted). Resolve it FIRST so a missing
        // session is cleanly "not found" rather than conflated with a parse error.
        let summary = match conversation_service::get_by_id(conn, session_id).await {
            Ok(s) => s,
            Err(_) => return SessionInfo::not_found(session_id),
        };

        let (workspace_path, workspace_name) =
            match folder_service::get_folder_by_id(conn, summary.folder_id).await {
                Ok(Some(folder)) => (Some(folder.path), Some(folder.name)),
                _ => (None, None),
            };

        let mut info = SessionInfo {
            found: true,
            session_id: summary.id,
            external_id: summary.external_id.clone(),
            agent_type: Some(agent_type_str(summary.agent_type)),
            title: summary.title.clone().filter(|t| !t.trim().is_empty()),
            status: Some(summary.status.clone()),
            model: summary.model.clone(),
            git_branch: summary.git_branch.clone(),
            workspace_path,
            workspace_name,
            message_count: Some(summary.message_count),
            created_at: Some(summary.created_at),
            updated_at: Some(summary.updated_at),
            parent_id: summary.parent_id,
            is_delegation_child: summary.parent_id.is_some(),
            stats: None,
            messages: None,
            note: None,
        };

        if max_messages == 0 {
            return info;
        }
        let max = max_messages.min(MAX_SESSION_MESSAGES);

        // Parse the transcript off the hot path, gated by `parse_limit` and bounded
        // by PARSE_TIMEOUT (see `bounded_parse`). A busy / timeout / parse error all
        // degrade to metadata-only with an explanatory note rather than failing the
        // whole tool call.
        let conn_owned = self.db.conn.clone();
        let parse = async move { get_folder_conversation_core(&conn_owned, session_id).await };
        match bounded_parse(self.parse_limit.clone(), PARSE_TIMEOUT, parse).await {
            ParseSlot::Ready(Ok((detail, parsed_title))) => {
                if info.title.is_none() {
                    info.title = parsed_title.filter(|t| !t.trim().is_empty());
                }
                // get_folder_conversation_core sets summary.message_count to the
                // parsed turn count — more accurate than the stored row.
                info.message_count = Some(detail.summary.message_count);
                info.stats = detail.session_stats;
                info.messages = Some(compact_turns(&detail.turns, max));
            }
            ParseSlot::Ready(Err(_)) => {
                info.note = Some(
                    "Recent messages are unavailable — the session transcript could not be parsed."
                        .to_string(),
                );
            }
            ParseSlot::Busy => {
                info.note = Some(
                    "Recent messages are unavailable — too many session reads are in progress. \
                     Retry, or call again with max_messages: 0 for metadata only."
                        .to_string(),
                );
            }
            ParseSlot::TimedOut => {
                info.note = Some(
                    "Recent messages are unavailable — reading the session transcript timed out. \
                     Retry, or call again with max_messages: 0 for metadata only."
                        .to_string(),
                );
            }
        }
        info
    }
}

/// Outcome of a permit-gated, timeout-bounded parse (see [`bounded_parse`]).
enum ParseSlot<T> {
    /// The parse completed within the timeout; carries its result.
    Ready(T),
    /// No parse slot was free — work was NOT started (caller should degrade now).
    Busy,
    /// A slot was taken and the parse started, but the caller's wait elapsed. The
    /// detached worker keeps its permit until the (uncancelable) work finishes, so
    /// the slot count keeps bounding real blocking concurrency.
    TimedOut,
}

/// Run `parse` under a real concurrency bound: acquire one of `parse_limit`'s
/// permits with a NON-blocking `try_acquire` (so a flood returns [`ParseSlot::Busy`]
/// instead of queueing), spawn the work into a detached task that holds the permit
/// until `parse` actually returns, and wait for it for at most `timeout`.
///
/// The permit lives with the spawned task — NOT with the caller's `timeout` future
/// — so when the caller gives up ([`ParseSlot::TimedOut`]) the permit stays held
/// until the underlying (uncancelable `spawn_blocking`) parse completes. That is
/// what makes `MAX_CONCURRENT_PARSES` bound the number of in-flight blocking parses
/// even under repeated timeouts, rather than just the number of waiting callers.
async fn bounded_parse<Fut, T>(
    parse_limit: Arc<tokio::sync::Semaphore>,
    timeout: Duration,
    parse: Fut,
) -> ParseSlot<T>
where
    Fut: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    // try_acquire (not acquire): don't queue behind busy parses — fail fast so the
    // caller can return metadata-only immediately instead of piling up tasks.
    let Ok(permit) = parse_limit.try_acquire_owned() else {
        return ParseSlot::Busy;
    };
    let handle = tokio::spawn(async move {
        // Hold the permit for the WHOLE parse; released only when `parse` returns.
        let _permit = permit;
        parse.await
    });
    match tokio::time::timeout(timeout, handle).await {
        Ok(Ok(value)) => ParseSlot::Ready(value),
        // Task panicked — treat as a failed parse (no value).
        Ok(Err(_join)) => ParseSlot::TimedOut,
        // Caller's wait elapsed; the detached task keeps running with its permit.
        Err(_) => ParseSlot::TimedOut,
    }
}

/// Snake_case wire form of an [`AgentType`] (e.g. `claude_code`) — matches the
/// `conversation.agent_type` column and the frontend's `ALL_AGENT_TYPES`.
fn agent_type_str(at: AgentType) -> String {
    serde_json::to_value(at)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// Compact the most recent `max` turns into [`SessionMessages`]. Walks the turns
/// newest-first so the freshest context is kept under [`OVERALL_CHARS`], always
/// retaining at least the newest turn, then restores chronological order.
fn compact_turns(turns: &[MessageTurn], max: u32) -> SessionMessages {
    let total = turns.len();
    let max = max as usize;
    let mut items: Vec<SessionMessageItem> = Vec::new();
    let mut budget = OVERALL_CHARS;

    for turn in turns.iter().rev() {
        if items.len() >= max {
            break;
        }
        let item = compact_turn(turn);
        // Charge BOTH the text and the (bounded) tool names against the budget so
        // a turn can't smuggle an oversized payload through `tools`.
        let cost =
            item.text.chars().count() + item.tools.iter().map(|t| t.chars().count()).sum::<usize>();
        // Always keep the newest turn; stop once the budget can't fit the next.
        if !items.is_empty() && cost > budget {
            break;
        }
        budget = budget.saturating_sub(cost);
        items.push(item);
    }
    items.reverse();

    let included = items.len();
    SessionMessages {
        total: total as u32,
        included: included as u32,
        truncated: included < total,
        items,
    }
}

/// One turn → role + truncated text (Text/Thinking blocks) + tool names.
fn compact_turn(turn: &MessageTurn) -> SessionMessageItem {
    let role = match turn.role {
        TurnRole::User => "user",
        TurnRole::Assistant => "assistant",
        TurnRole::System => "system",
    }
    .to_string();

    let mut parts: Vec<&str> = Vec::new();
    let mut tools: Vec<String> = Vec::new();
    for block in &turn.blocks {
        match block {
            ContentBlock::Text { text } | ContentBlock::Thinking { text } => {
                let t = text.trim();
                if !t.is_empty() {
                    parts.push(t);
                }
            }
            // Bound the count per turn and each name's length; dedup on the
            // truncated form so the collected `tools` can't exceed
            // MAX_TOOLS_PER_TURN * MAX_TOOL_NAME_CHARS. Once the cap is hit the
            // guard fails and further ToolUse blocks fall through to `_`.
            ContentBlock::ToolUse { tool_name, .. }
                if tools.len() < MAX_TOOLS_PER_TURN && !tool_name.is_empty() =>
            {
                let name = truncate_chars(tool_name, MAX_TOOL_NAME_CHARS);
                if !tools.contains(&name) {
                    tools.push(name);
                }
            }
            // Tool results, images, and image-generation carry no useful plain
            // text for a compact preview — skipped to keep the payload lean.
            _ => {}
        }
    }
    let text = truncate_chars(&parts.join("\n"), PER_TURN_CHARS);
    SessionMessageItem { role, text, tools }
}

/// Truncate to at most `cap` characters, appending an ellipsis marker when cut.
fn truncate_chars(s: &str, cap: usize) -> String {
    if s.chars().count() <= cap {
        return s.to_string();
    }
    let mut out: String = s.chars().take(cap).collect();
    out.push('…');
    out
}

// ===========================================================================
// Settings persistence — `session_info.enabled` (default ON). Mirrors
// `crate::commands::question`.
// ===========================================================================

pub const KEY_SESSION_INFO_ENABLED: &str = "session_info.enabled";

/// On by default (`enabled: true`). The `get_session_info` tool is read-only and
/// only fires when the user references a session, so — like `ask_user_question` —
/// it ships enabled; a user who never wants agents reading session data can turn
/// it off in settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionInfoSettings {
    pub enabled: bool,
}

impl Default for SessionInfoSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl SessionInfoSettings {
    fn into_runtime_config(self) -> SessionInfoConfig {
        SessionInfoConfig {
            enabled: self.enabled,
        }
    }
}

/// Read the persisted key from `app_metadata`, falling back to the default
/// (enabled) for a missing or malformed value. Never errors hard.
pub async fn load_session_info_settings(conn: &DatabaseConnection) -> SessionInfoSettings {
    let mut settings = SessionInfoSettings::default();
    if let Ok(Some(raw)) = app_metadata_service::get_value(conn, KEY_SESSION_INFO_ENABLED).await {
        if let Ok(v) = raw.parse::<bool>() {
            settings.enabled = v;
        }
    }
    settings
}

/// Pull settings from the DB and push the resulting `SessionInfoConfig` onto the
/// shared runtime handle. Idempotent — safe on startup or after any save.
pub async fn apply_persisted_session_info_config(
    conn: &DatabaseConnection,
    config: &SessionInfoRuntimeConfig,
) {
    let settings = load_session_info_settings(conn).await;
    config.set(settings.into_runtime_config()).await;
}

/// Persist + apply + broadcast. Shared by the Tauri command and the HTTP handler
/// so the write + re-apply + notify chain lives in one place.
pub async fn set_session_info_settings_core(
    conn: &DatabaseConnection,
    config: &SessionInfoRuntimeConfig,
    emitter: &EventEmitter,
    desired: SessionInfoSettings,
) -> Result<SessionInfoSettings, AppCommandError> {
    app_metadata_service::upsert_value(
        conn,
        KEY_SESSION_INFO_ENABLED,
        &desired.enabled.to_string(),
    )
    .await
    .map_err(AppCommandError::from)?;
    config.set(desired.clone().into_runtime_config()).await;
    emit_event(emitter, SESSION_INFO_SETTINGS_CHANGED_EVENT, &desired);
    Ok(desired)
}

// -------- Tauri commands -----------------------------------------------------

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_session_info_settings(
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<SessionInfoSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        Ok(load_session_info_settings(&db.conn).await)
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_session_info_settings(
    #[cfg(feature = "tauri-runtime")] app: tauri::AppHandle,
    #[cfg(feature = "tauri-runtime")] db: tauri::State<'_, crate::db::AppDatabase>,
    #[cfg(feature = "tauri-runtime")] config: tauri::State<'_, SessionInfoRuntimeConfig>,
    settings: SessionInfoSettings,
) -> Result<SessionInfoSettings, AppCommandError> {
    #[cfg(feature = "tauri-runtime")]
    {
        let emitter = EventEmitter::Tauri(app);
        set_session_info_settings_core(&db.conn, &config, &emitter, settings).await
    }
    #[cfg(not(feature = "tauri-runtime"))]
    {
        let _ = settings;
        Err(AppCommandError::configuration_invalid("tauri-only command"))
    }
}

