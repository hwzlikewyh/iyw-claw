use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::i18n::Lang;
use super::session_bridge::{PendingPermission, SessionBridge};
use super::session_topic;
use crate::acp::internal_bus::InternalEventBus;
use crate::acp::manager::ConnectionManager;
use crate::acp::types::{AcpEvent, ConnectionStatus, EventEnvelope, PromptInputBlock};
use crate::chat_channel::types::{MessageLevel, RichMessage};

use crate::db::service::{app_metadata_service, conversation_service, sender_context_service};

use super::manager::ChatChannelManager;

const FLUSH_INTERVAL_SECS: u64 = 10;
const BUFFER_FLUSH_THRESHOLD: usize = 500;
const MAX_MESSAGE_LEN: usize = 2000;
const MESSAGE_LANGUAGE_KEY: &str = "chat_message_language";

pub fn spawn_session_event_subscriber(
    bus: Arc<InternalEventBus>,
    bridge: Arc<Mutex<SessionBridge>>,
    manager: ChatChannelManager,
    conn_mgr: ConnectionManager,
    db_conn: DatabaseConnection,
) -> JoinHandle<()> {
    let mut rx = bus.subscribe();
    let metrics = Arc::clone(bus.metrics());

    tokio::spawn(async move {
        let mut last_heartbeat = Instant::now();

        loop {
            tokio::select! {
                result = rx.recv() => {
                    let envelope_arc = match result {
                        Ok(e) => e,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("[SessionEventSub] lagged {n} events");
                            metrics.lagged_count.fetch_add(n, Ordering::Relaxed);
                            continue;
                        }
                        Err(_) => break,
                    };

                    handle_acp_envelope(
                        envelope_arc.as_ref(),
                        &bridge,
                        &manager,
                        &conn_mgr,
                        &db_conn,
                    )
                    .await;
                }
                _ = tokio::time::sleep(Duration::from_secs(FLUSH_INTERVAL_SECS)) => {
                    if last_heartbeat.elapsed() >= Duration::from_secs(FLUSH_INTERVAL_SECS) {
                        flush_progress(&bridge, &manager, &db_conn).await;
                        last_heartbeat = Instant::now();
                    }
                }
            }
        }
    })
}

async fn get_lang(db: &DatabaseConnection) -> Lang {
    app_metadata_service::get_value(db, MESSAGE_LANGUAGE_KEY)
        .await
        .ok()
        .flatten()
        .map(|v| Lang::from_str_lossy(&v))
        .unwrap_or_default()
}

/// Phase 5: typed-envelope dispatcher. Replaces the prior JSON
/// `payload.get("type").as_str()` switch — every accessor we used to need
/// (type / connection_id / event-specific fields) is now a structural
/// match on `AcpEvent`, with no `unwrap_or("")` defensive fallbacks.
async fn handle_acp_envelope(
    envelope: &EventEnvelope,
    bridge: &Arc<Mutex<SessionBridge>>,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    db: &DatabaseConnection,
) {
    let connection_id = envelope.connection_id.as_str();

    match &envelope.payload {
        AcpEvent::SessionStarted { session_id } => {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.get_mut(connection_id) {
                let _ = conversation_service::update_external_id(
                    db,
                    session.conversation_id,
                    session_id.clone(),
                )
                .await;

                if let Some(prompt_text) = session.pending_prompt.take() {
                    // Clone so the prompt can be RESTORED (not dropped) if a turn
                    // is already in flight — see the TurnInProgress arm below.
                    let blocks = vec![PromptInputBlock::Text {
                        text: prompt_text.clone(),
                    }];
                    if let Err(e) = conn_mgr.send_prompt(connection_id, blocks).await {
                        // A turn is already in flight on this shared connection
                        // (another client raced this kickoff between
                        // SessionStarted and here). Transient, not a failure —
                        // RESTORE the pending prompt so the TurnComplete handler
                        // retries the kickoff once the in-flight turn finishes,
                        // instead of silently dropping the task's initial prompt.
                        if matches!(e, crate::acp::error::AcpError::TurnInProgress) {
                            session.pending_prompt = Some(prompt_text);
                            tracing::warn!(
                                "[SessionEventSub] kickoff deferred; a turn is already in \
                                 progress, will retry on TurnComplete"
                            );
                        } else {
                            tracing::error!("[SessionEventSub] failed to send pending prompt: {e}");
                        }
                    }
                }
            }
        }

        AcpEvent::ContentDelta { text } => {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.get_mut(connection_id) {
                session.content_buffer.push_str(text);
                if session.content_buffer.len() >= BUFFER_FLUSH_THRESHOLD
                    && session.last_flushed.elapsed() >= Duration::from_secs(2)
                {
                    session.last_flushed = Instant::now();
                }
            }
        }

        AcpEvent::ToolCall {
            tool_call_id,
            title,
            raw_input,
            ..
        } => {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.get_mut(connection_id) {
                session.tool_calls.push(title.clone());
                if let Some(input) = raw_input.as_deref() {
                    session
                        .tool_call_inputs
                        .insert(tool_call_id.clone(), input.to_string());
                }
            }
        }

        AcpEvent::ToolCallUpdate {
            tool_call_id,
            title,
            status,
            raw_input,
            raw_output,
            ..
        } => {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.get_mut(connection_id) {
                // Accumulate raw_input if newly available
                if let Some(input) = raw_input.as_deref() {
                    session
                        .tool_call_inputs
                        .insert(tool_call_id.clone(), input.to_string());
                }

                if status.as_deref() == Some("completed") {
                    let effective_title = title.as_deref().unwrap_or("tool");
                    let is_delegation = is_delegation_title(effective_title)
                        || session
                            .tool_call_inputs
                            .get(tool_call_id)
                            .map(|s| extract_agent_type(s).is_some())
                            .unwrap_or(false)
                        || raw_input
                            .as_deref()
                            .map(|s| extract_agent_type(s).is_some())
                            .unwrap_or(false);
                    if is_delegation {
                        let already_rendered = session.delegation_rendered.contains(tool_call_id);
                        let report = parse_delegation_report(raw_output.as_deref());
                        if report.as_ref().is_some_and(|r| r.is_terminal()) {
                            if !already_rendered {
                                session.delegation_rendered.insert(tool_call_id.clone());
                                session.tool_call_inputs.remove(tool_call_id);
                            }
                        } else if !already_rendered {
                            // Running ack (or unparseable output): keep the
                            // stored input for eventual DelegationCompleted, but
                            // never post this process detail to chat channels.
                        }
                    } else {
                        session.tool_call_inputs.remove(tool_call_id);
                    }
                }
            }
        }

        // The async delegation result for the normal (slow) case: the tool
        // output was a running ack (handled above), and the child's final
        // outcome surfaces here. Rendered EXACTLY ONCE via the same stored-input
        // dedup token — if a terminal `ToolCallUpdate` already rendered (and
        // removed it), skip. (A synthetic `parent_tool_use_id` never reaches a
        // bridged session's stored input, so this arm no-ops for synthetic ids,
        // which is correct — the terminal `ToolCallUpdate` is their surface.)
        AcpEvent::DelegationCompleted {
            parent_tool_use_id, ..
        } => {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.get_mut(connection_id) {
                // Render EXACTLY ONCE, gated on the `delegation_rendered` marker:
                // if a terminal `ToolCallUpdate` already rendered this task's
                // result, skip. (A synthetic `parent_tool_use_id` is never emitted
                // here at all, so this arm naturally no-ops for synthetic ids —
                // the terminal `ToolCallUpdate` is their surface.)
                if !session.delegation_rendered.contains(parent_tool_use_id) {
                    session.tool_call_inputs.remove(parent_tool_use_id);
                    session
                        .delegation_rendered
                        .insert(parent_tool_use_id.clone());
                }
            }
        }

        AcpEvent::PermissionRequest {
            request_id,
            options,
            ..
        } => {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.get_mut(connection_id) {
                let channel_id = session.channel_id;
                let sender_id = session.sender_id.clone();
                let target = session.target.clone();

                let auto_approve =
                    sender_context_service::get_or_create(db, channel_id, &sender_id)
                        .await
                        .map(|ctx| ctx.auto_approve)
                        .unwrap_or(false);

                if auto_approve {
                    let option_id = options
                        .iter()
                        .find(|o| o.kind == "allow" || o.kind == "allowForSession")
                        .or_else(|| options.first())
                        .map(|o| o.option_id.clone());

                    drop(guard);

                    if let Some(oid) = option_id {
                        let _ = conn_mgr
                            .respond_permission(connection_id, request_id, &oid)
                            .await;
                    }
                    return;
                }

                session.permission_pending = Some(PendingPermission {
                    request_id: request_id.clone(),
                    tool_description: "permission".to_string(),
                    options: options.clone(),
                    sent_message_id: None,
                });

                drop(guard);

                let lang = get_lang(db).await;
                let body = permission_confirmation_body(lang).to_string();

                let msg = RichMessage {
                    title: Some(match lang {
                        Lang::ZhCn | Lang::ZhTw => "需要确认".to_string(),
                        _ => "Confirmation Needed".to_string(),
                    }),
                    body,
                    fields: Vec::new(),
                    level: MessageLevel::Warning,
                };
                let _ = manager.send_to_target(&target, &msg).await;
            }
        }

        AcpEvent::TurnComplete { stop_reason, .. } => {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.get_mut(connection_id) {
                let channel_id = session.channel_id;
                let target = session.target.clone();
                let conv_id = session.conversation_id;
                let content = std::mem::take(&mut session.content_buffer);
                session.tool_calls.clear();
                session.last_flushed = Instant::now();
                // A kickoff prompt deferred by `SessionStarted` (the connection
                // was already mid-turn for another client) waits here. Take it
                // BEFORE dropping the guard so a second TurnComplete can't
                // double-send it; retry below once the lock is released.
                let deferred_kickoff = session.pending_prompt.take();
                drop(guard);

                let lang = get_lang(db).await;
                let body = format_completion(&content, lang);

                if !body.trim().is_empty() {
                    let msg = RichMessage::info(body);
                    let _ = manager.send_to_target(&target, &msg).await;
                } else if !content.trim().is_empty() {
                    tracing::info!(
                        "[SessionEventSub] assistant completion suppressed after channel \
                         sanitization connection={} channel={} conversation={}",
                        connection_id,
                        channel_id,
                        conv_id
                    );
                }

                if stop_reason == "end_turn" {
                    let _ = conversation_service::update_status(
                        db,
                        conv_id,
                        crate::db::entities::conversation::ConversationStatus::Completed,
                    )
                    .await;
                }

                // Retry the deferred kickoff now the turn that blocked it ended.
                // If yet ANOTHER turn slipped in (another client raced this
                // TurnComplete), restore the prompt for the next TurnComplete —
                // never drop it.
                if let Some(prompt_text) = deferred_kickoff {
                    let blocks = vec![PromptInputBlock::Text {
                        text: prompt_text.clone(),
                    }];
                    if let Err(e) = conn_mgr.send_prompt(connection_id, blocks).await {
                        if matches!(e, crate::acp::error::AcpError::TurnInProgress) {
                            let mut g = bridge.lock().await;
                            if let Some(s) = g.get_mut(connection_id) {
                                s.pending_prompt = Some(prompt_text);
                            }
                            tracing::warn!(
                                "[SessionEventSub] deferred kickoff still blocked; will retry on \
                                 next TurnComplete"
                            );
                        } else {
                            tracing::error!(
                                "[SessionEventSub] failed to send deferred kickoff: {e}"
                            );
                        }
                    }
                }
            }
        }

        AcpEvent::Error { terminal, .. } => {
            // Non-terminal Errors (`turn_failure_error_event`,
            // `session/load` fallback, empty-prompt rejection, SetMode /
            // SetConfigOption failures) leave the ACP connection alive —
            // the next prompt on the same session will still work. Chat
            // channels only receive real assistant content, so ACP errors are
            // log-only here.
            if !*terminal {
                if let Some(channel_id) = {
                    let guard = bridge.lock().await;
                    guard.get(connection_id).map(|s| s.channel_id)
                } {
                    tracing::warn!(
                        "[SessionEventSub] non-terminal ACP error for bridged session \
                         connection={} channel={}",
                        connection_id,
                        channel_id
                    );
                }
                return;
            }

            let mut guard = bridge.lock().await;
            if let Some(session) = guard.remove(connection_id) {
                let channel_id = session.channel_id;
                let sender_id = session.sender_id.clone();
                let target = session.target.clone();
                let conv_id = session.conversation_id;
                drop(guard);

                tracing::warn!(
                    "[SessionEventSub] terminal ACP error for bridged session \
                     connection={} channel={} conversation={}",
                    connection_id,
                    channel_id,
                    conv_id
                );

                let _ = conversation_service::update_status(
                    db,
                    conv_id,
                    crate::db::entities::conversation::ConversationStatus::Cancelled,
                )
                .await;
                session_topic::clear_route(db, channel_id, &sender_id, &target).await;
            }
        }

        AcpEvent::StatusChanged { status } => {
            if matches!(
                status,
                ConnectionStatus::Disconnected | ConnectionStatus::Error
            ) {
                let mut guard = bridge.lock().await;
                if let Some(session) = guard.remove(connection_id) {
                    let channel_id = session.channel_id;
                    let sender_id = session.sender_id.clone();
                    let target = session.target.clone();
                    drop(guard);

                    session_topic::clear_route(db, channel_id, &sender_id, &target).await;
                }
            }
        }

        _ => {}
    }
}

async fn flush_progress(
    bridge: &Arc<Mutex<SessionBridge>>,
    _manager: &ChatChannelManager,
    _db: &DatabaseConnection,
) {
    let mut guard = bridge.lock().await;
    for session in guard.all_sessions_mut() {
        if !session.content_buffer.is_empty()
            && session.last_flushed.elapsed() >= Duration::from_secs(FLUSH_INTERVAL_SECS)
        {
            session.last_flushed = Instant::now();
        }
    }
}

fn format_completion(content: &str, lang: Lang) -> String {
    let content = sanitize_channel_content(content);
    if content.is_empty() {
        return String::new();
    }

    if content.len() <= MAX_MESSAGE_LEN {
        return content;
    }

    // Truncate long content (use char boundaries to avoid panic on multi-byte)
    let head_end = content
        .char_indices()
        .nth(500)
        .map(|(i, _)| i)
        .unwrap_or(content.len());
    let head = &content[..head_end];
    let tail_start = content
        .char_indices()
        .rev()
        .nth(499)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let tail = &content[tail_start..];

    match lang {
        Lang::ZhCn | Lang::ZhTw => {
            format!(
                "{head}\n\n...\n\n{tail}\n\n[完整回复: {} 字符]",
                content.len()
            )
        }
        _ => {
            format!(
                "{head}\n\n...\n\n{tail}\n\n[Full response: {} chars]",
                content.len()
            )
        }
    }
}

fn sanitize_channel_content(content: &str) -> String {
    content
        .lines()
        .filter(|line| !is_internal_process_line(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn is_internal_process_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    lower.contains("using-superpowers")
        || lower.contains("codex cli")
        || lower.contains("claude code")
        || lower.contains("get-content")
        || lower.contains("pwsh.exe")
        || lower.contains("powershell.exe")
        || lower.contains("bash:")
        || lower.contains("tool call")
        || lower.contains("工具调用")
        || lower.contains("正在响应")
        || lower.contains("running in background")
        || lower.starts_with("i will first")
        || lower.starts_with("i'll first")
        || lower.starts_with("i’ll first")
        || trimmed.starts_with("我会先")
        || trimmed.starts_with("我先")
        || trimmed.starts_with("先加载")
        || trimmed.starts_with("我将先")
}

fn permission_confirmation_body(lang: Lang) -> &'static str {
    match lang {
        Lang::ZhCn | Lang::ZhTw => "需要你确认是否允许继续执行。回复“可以”或“拒绝”即可。",
        _ => "Please confirm whether to continue. Reply \"yes\" or \"no\".",
    }
}

/// Title-side match for `delegate_to_agent`. Title is free-form text the
/// host agent composes; some hosts copy the bare MCP method, some prefix
/// it with `mcp__<server>__`, some rephrase it. Match by substring so any
/// of those forms get the delegation-announcement path. The completion-
/// side callsite already pairs this with a raw_input shape check, so a
/// rare false-positive here just sends one announce message that gets
/// overwritten by the completion's actual outcome.
fn is_delegation_title(title: &str) -> bool {
    let normalized = title.to_lowercase().replace([' ', '-'], "_");
    normalized.contains("delegate_to_agent")
}

/// Pull `agent_type` out of the raw_input JSON (e.g. `{"agent_type":"codex",
/// "task":"..."}`). Returns the canonical string the agent supplied so the
/// announce message matches what the user wrote, not a re-mapped label.
fn extract_agent_type(raw_input: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw_input).ok()?;
    parsed
        .get("agent_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// A `delegate_to_agent` tool output parsed into the fields the chat relay
/// needs to classify it (running ack vs terminal) and render the terminal line.
struct DelegationReportView {
    status: Option<String>,
}

impl DelegationReportView {
    fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_deref(),
            Some("completed") | Some("failed") | Some("canceled")
        )
    }
}

/// Parse a `delegate_to_agent` tool output into a [`DelegationReportView`],
/// unwrapping the MCP `CallToolResult` envelope's `structuredContent` and
/// tolerating host wrappers — notably Codex, which serializes MCP output as
/// `"Wall time: N seconds\nOutput:\n<json>"` (sometimes with a trailing cursor
/// char). Mirrors the frontend's lenient extraction so terminal detection works
/// across hosts. Returns `None` when no JSON object can be recovered.
fn parse_delegation_report(raw_output: Option<&str>) -> Option<DelegationReportView> {
    let value = parse_json_lenient(raw_output?)?;
    let report = value.get("structuredContent").unwrap_or(&value);
    Some(DelegationReportView {
        status: report
            .get("status")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

/// Parse JSON tolerant of a textual prefix/suffix around the object (Codex
/// wrapping): try a direct parse, then scan back from the last `}` to the first
/// `{` until a balanced span parses. Bounded by the count of `}` characters.
fn parse_json_lenient(s: &str) -> Option<serde_json::Value> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s.trim()) {
        return Some(v);
    }
    let start = s.find('{')?;
    let mut end = s.rfind('}')?;
    while end > start {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s[start..=end]) {
            return Some(v);
        }
        match s[start..end].rfind('}') {
            Some(rel) => end = start + rel,
            None => break,
        }
    }
    None
}
