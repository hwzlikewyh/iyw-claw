use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::i18n::Lang;
use super::session_bridge::{ActiveSession, PendingPermission, RouteActivation, SessionBridge};
use super::session_commands;
use super::session_topic;
use crate::acp::internal_bus::InternalEventBus;
use crate::acp::manager::ConnectionManager;
use crate::acp::types::{AcpEvent, ConnectionStatus, EventEnvelope, PromptInputBlock};
use crate::chat_channel::types::{MessageLevel, RichMessage};
use crate::web::event_bridge::EventEmitter;

use crate::db::service::{
    app_metadata_service, chat_channel_message_log_service, chat_channel_target_service,
    conversation_binding_service, conversation_service, sender_context_service,
};

use super::manager::ChatChannelManager;

const FLUSH_INTERVAL_SECS: u64 = 10;
const BUFFER_FLUSH_THRESHOLD: usize = 500;
const MAX_MESSAGE_LEN: usize = 2000;
const MESSAGE_LANGUAGE_KEY: &str = "chat_message_language";
/// Deferred kickoff prompts (blocked by an in-flight turn) are retried at
/// most this many times, then surfaced as an explicit failure instead of
/// retrying forever.
const MAX_KICKOFF_RETRIES: u32 = 3;

#[derive(Clone, Copy)]
struct SessionStartRequest<'a> {
    bridge: &'a Arc<Mutex<SessionBridge>>,
    manager: &'a ChatChannelManager,
    conn_mgr: &'a ConnectionManager,
    db: &'a DatabaseConnection,
    connection_id: &'a str,
    session_id: &'a str,
    event_seq: Option<u64>,
}

struct SessionStartContext {
    conversation_id: i32,
    channel_id: i32,
    expected_external_id: Option<String>,
    route: Option<conversation_binding_service::ConversationRoute>,
    activate_route: bool,
}

enum SessionStartPersistence {
    Missing,
    Superseded,
    Rejected {
        session: ActiveSession,
        error: String,
    },
    BindFailed {
        session: ActiveSession,
        error: String,
    },
    Complete(Vec<ActiveSession>),
}

pub fn spawn_session_event_subscriber(
    bus: Arc<InternalEventBus>,
    bridge: Arc<Mutex<SessionBridge>>,
    manager: ChatChannelManager,
    conn_mgr: ConnectionManager,
    db_conn: DatabaseConnection,
    emitter: EventEmitter,
    data_dir: PathBuf,
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
                        &emitter,
                        &data_dir,
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
    emitter: &EventEmitter,
    data_dir: &Path,
) {
    let connection_id = envelope.connection_id.as_str();

    match &envelope.payload {
        AcpEvent::SessionStarted { session_id } => {
            let request = SessionStartRequest {
                bridge,
                manager,
                conn_mgr,
                db,
                connection_id,
                session_id,
                event_seq: Some(envelope.seq),
            };
            if !complete_session_start(request).await {
                return;
            }
            send_pending_prompt(bridge, manager, conn_mgr, db, connection_id).await;
        }

        AcpEvent::UserMessage { .. } => {
            if let Some(session) = bridge.lock().await.get_mut(connection_id) {
                session.recovery_prompt = None;
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

                let route_key = session.route_key.clone();

                drop(guard);

                manager
                    .typing_controller()
                    .pause(manager, channel_id, &route_key, connection_id)
                    .await;

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

        AcpEvent::QuestionRequest { .. } | AcpEvent::ChannelConfirmationRequested { .. } => {
            if let Some((channel_id, route_key)) = session_route(bridge, connection_id).await {
                manager
                    .typing_controller()
                    .pause(manager, channel_id, &route_key, connection_id)
                    .await;
            }
        }

        AcpEvent::PermissionResolved { .. }
        | AcpEvent::QuestionResolved { .. }
        | AcpEvent::ChannelConfirmationResolved { .. } => {
            if let Some((channel_id, route_key)) = session_route(bridge, connection_id).await {
                manager
                    .typing_controller()
                    .resume(manager, channel_id, &route_key, connection_id)
                    .await;
            }
        }

        AcpEvent::TurnComplete { stop_reason, .. } => {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.get_mut(connection_id) {
                let channel_id = session.channel_id;
                let target = session.target.clone();
                let conv_id = session.conversation_id;
                let trace_id = session.trace_id.clone();
                let content = std::mem::take(&mut session.content_buffer);
                session.tool_calls.clear();
                session.last_flushed = Instant::now();
                // A kickoff prompt deferred by `SessionStarted` (the connection
                // was already mid-turn for another client) waits here. Take it
                // BEFORE dropping the guard so a second TurnComplete can't
                // double-send it; retry below once the lock is released.
                let deferred_kickoff = session.pending_prompt.take();
                let route_key = session.route_key.clone();
                drop(guard);

                manager
                    .typing_controller()
                    .stop(manager, channel_id, &route_key, connection_id)
                    .await;

                let lang = get_lang(db).await;
                let body = format_completion(&content, lang);
                let target_id = chat_channel_target_service::find_by_target(db, &target)
                    .await
                    .ok()
                    .flatten()
                    .map(|registered| registered.target_id);

                if !body.trim().is_empty() {
                    let msg = RichMessage::info(body.clone());
                    // The outbound reply is stamped with the session trace so
                    // the message log reconstructs the whole round trip.
                    match manager.send_to_target(&target, &msg).await {
                        Ok(sent_id) => {
                            let _ = chat_channel_message_log_service::create_log_for_target(
                                db,
                                channel_id,
                                "outbound",
                                "agent_reply",
                                &body,
                                "sent",
                                None,
                                trace_id,
                                Some(sent_id.0),
                                target_id.clone(),
                            )
                            .await;
                        }
                        Err(error) => {
                            tracing::error!(
                                "[SessionEventSub] failed to send completion to channel={} \
                                 conversation={}: {error}",
                                channel_id,
                                conv_id
                            );
                            let _ = chat_channel_message_log_service::create_log_for_target(
                                db,
                                channel_id,
                                "outbound",
                                "agent_reply",
                                &body,
                                "failed",
                                Some("CHANNEL_SEND_FAILED".to_string()),
                                trace_id,
                                None,
                                target_id,
                            )
                            .await;
                        }
                    }
                } else if !content.trim().is_empty() {
                    tracing::info!(
                        "[SessionEventSub] assistant completion suppressed after channel \
                         sanitization connection={} channel={} conversation={}",
                        connection_id,
                        channel_id,
                        conv_id
                    );
                    // The agent produced output but every line was filtered as
                    // internal process chatter — surface that to the user so
                    // the turn doesn't look like a silent no-op.
                    let notice = match lang {
                        Lang::ZhCn | Lang::ZhTw => {
                            "本回合没有可展示的内容（回复被通道过滤）。可换一种表述重试。"
                        }
                        _ => "This turn produced no displayable content (the reply was filtered by the channel). Try rephrasing.",
                    };
                    let _ = manager
                        .send_to_target(&target, &RichMessage::info(notice.to_string()))
                        .await;
                } else {
                    // A turn completed without any assistant text at all — make
                    // it visible instead of a silent gap.
                    let notice = match lang {
                        Lang::ZhCn | Lang::ZhTw => "本回合没有生成内容。",
                        _ => "This turn produced no content.",
                    };
                    let _ = manager
                        .send_to_target(&target, &RichMessage::info(notice.to_string()))
                        .await;
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
                // but only a bounded number of times; beyond that surface an
                // explicit failure instead of retrying forever.
                if let Some(prompt_text) = deferred_kickoff {
                    let blocks = vec![PromptInputBlock::Text {
                        text: prompt_text.clone(),
                    }];
                    if let Err(e) = conn_mgr.send_prompt(connection_id, blocks).await {
                        if matches!(e, crate::acp::error::AcpError::TurnInProgress) {
                            let mut g = bridge.lock().await;
                            let give_up = if let Some(s) = g.get_mut(connection_id) {
                                s.pending_prompt_attempts =
                                    s.pending_prompt_attempts.saturating_add(1);
                                if s.pending_prompt_attempts >= MAX_KICKOFF_RETRIES {
                                    s.pending_prompt = None;
                                    true
                                } else {
                                    s.pending_prompt = Some(prompt_text);
                                    false
                                }
                            } else {
                                true
                            };
                            if give_up {
                                let fail_msg = match lang {
                                    Lang::ZhCn | Lang::ZhTw => {
                                        "任务启动提示连续被占用，已放弃自动重试；请重新发送。"
                                    }
                                    _ => "The kickoff prompt was repeatedly blocked; auto-retry stopped. Please resend.",
                                };
                                let _ = manager
                                    .send_to_target(
                                        &target,
                                        &RichMessage::error(fail_msg.to_string()),
                                    )
                                    .await;
                            } else {
                                tracing::warn!(
                                    "[SessionEventSub] deferred kickoff still blocked; will retry on \
                                     next TurnComplete"
                                );
                            }
                        } else {
                            tracing::error!(
                                "[SessionEventSub] failed to send deferred kickoff: {e}"
                            );
                        }
                    }
                }
            }
        }

        AcpEvent::Error {
            message, terminal, ..
        } => {
            // Non-terminal Errors (`turn_failure_error_event`,
            // `session/load` fallback, empty-prompt rejection, SetMode /
            // SetConfigOption failures) leave the ACP connection alive —
            // the next prompt on the same session will still work. Chat
            // channels only receive real assistant content, so non-terminal
            // ACP errors stay log-only here.
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
                let trace_id = session.trace_id.clone();
                let route_key = session.route_key.clone();
                drop(guard);

                manager
                    .typing_controller()
                    .stop(manager, channel_id, &route_key, connection_id)
                    .await;

                tracing::warn!(
                    "[SessionEventSub] terminal ACP error for bridged session \
                     connection={} channel={} conversation={}: {message}",
                    connection_id,
                    channel_id,
                    conv_id
                );

                // A terminal error means the session can no longer reply —
                // surface an explicit failure instead of a silent gap.
                let lang = get_lang(db).await;
                let fail_body = match lang {
                    Lang::ZhCn | Lang::ZhTw => {
                        format!("会话执行失败：{message}")
                    }
                    _ => format!("Session failed: {message}"),
                };
                let _ = manager
                    .send_to_target(&target, &RichMessage::error(fail_body.clone()))
                    .await;
                let target_id = chat_channel_target_service::find_by_target(db, &target)
                    .await
                    .ok()
                    .flatten()
                    .map(|registered| registered.target_id);
                let _ = chat_channel_message_log_service::create_log_for_target(
                    db,
                    channel_id,
                    "outbound",
                    "agent_reply",
                    &fail_body,
                    "failed",
                    Some("AGENT_SESSION_FAILED".to_string()),
                    trace_id,
                    None,
                    target_id,
                )
                .await;

                let _ = conversation_service::update_status(
                    db,
                    conv_id,
                    crate::db::entities::conversation::ConversationStatus::Cancelled,
                )
                .await;
                session_topic::clear_route_if_connection(
                    db,
                    channel_id,
                    &sender_id,
                    &target,
                    connection_id,
                )
                .await;
                promote_fallback_session(bridge, manager, conn_mgr, db, &session).await;
            }
        }

        AcpEvent::SessionLoadFailed {
            message,
            session_id,
            ..
        } => {
            let session = bridge.lock().await.remove(connection_id);
            let Some(session) = session else { return };
            let lang = get_lang(db).await;
            handle_session_load_failure(
                db,
                manager,
                conn_mgr,
                emitter,
                bridge,
                data_dir,
                connection_id,
                session,
                message,
                Some(session_id),
                lang,
            )
            .await;
        }

        AcpEvent::StatusChanged { status }
            if matches!(
                status,
                ConnectionStatus::Disconnected | ConnectionStatus::Error
            ) =>
        {
            let mut guard = bridge.lock().await;
            if let Some(session) = guard.remove(connection_id) {
                let channel_id = session.channel_id;
                let sender_id = session.sender_id.clone();
                let target = session.target.clone();
                let route_key = session.route_key.clone();
                drop(guard);

                manager
                    .typing_controller()
                    .stop(manager, channel_id, &route_key, connection_id)
                    .await;

                session_topic::clear_route_if_connection(
                    db,
                    channel_id,
                    &sender_id,
                    &target,
                    connection_id,
                )
                .await;
                promote_fallback_session(bridge, manager, conn_mgr, db, &session).await;
            }
        }

        AcpEvent::StatusChanged {
            status: ConnectionStatus::Prompting,
        } => {
            if let Some((channel_id, route_key, target)) =
                session_route_target(bridge, connection_id).await
            {
                manager
                    .typing_controller()
                    .start(
                        manager.clone_ref(),
                        target,
                        route_key,
                        connection_id.to_string(),
                    )
                    .await;
                tracing::debug!(channel_id, "[ChatChannel] typing lease started");
            }
        }

        _ => {}
    }
}

fn session_load_failure_message(lang: Lang, message: &str, retrying: bool) -> String {
    match (lang, retrying) {
        (Lang::ZhCn | Lang::ZhTw, true) => format!(
            "Agent 无法恢复原会话（{message}）。原对话绑定已保留，正在自动以可见历史摘要继续处理本条消息。"
        ),
        (Lang::ZhCn | Lang::ZhTw, false) => format!(
            "Agent 无法恢复原会话（{message}）。原对话绑定已保留；下一条消息会自动以可见历史摘要继续。"
        ),
        (_, true) => format!(
            "The Agent could not restore the bound session ({message}). The conversation binding is preserved and this message is being retried automatically with a visible history recap."
        ),
        (_, false) => format!(
            "The Agent could not restore the bound session ({message}). The conversation binding is preserved; your next message will continue from a visible history recap."
        ),
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_session_load_failure(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    data_dir: &Path,
    connection_id: &str,
    session: ActiveSession,
    message: &str,
    expected_external_id: Option<&str>,
    lang: Lang,
) {
    let channel_id = session.channel_id;
    let target = session.target.clone();
    manager
        .typing_controller()
        .stop(manager, channel_id, &session.route_key, connection_id)
        .await;
    let external_id_cleared = if let Some(expected_external_id) = expected_external_id {
        match conversation_service::clear_external_id_if_matches(
            db,
            session.conversation_id,
            expected_external_id,
        )
        .await
        {
            Ok(cleared) => {
                tracing::info!(
                    connection_id,
                    conversation_id = session.conversation_id,
                    cleared,
                    "[SessionEventSub] recovery failure external id CAS completed"
                );
                cleared
            }
            Err(error) => {
                tracing::warn!(
                    connection_id,
                    conversation_id = session.conversation_id,
                    error = %error,
                    "[SessionEventSub] recovery failure external id CAS failed"
                );
                false
            }
        }
    } else {
        tracing::warn!(
            connection_id,
            conversation_id = session.conversation_id,
            "[SessionEventSub] recovery failure missing expected external id"
        );
        false
    };
    session_topic::clear_route_if_connection(
        db,
        channel_id,
        &session.sender_id,
        &target,
        connection_id,
    )
    .await;
    if !external_id_cleared {
        tracing::info!(
            connection_id,
            conversation_id = session.conversation_id,
            "[SessionEventSub] stale recovery failure ignored"
        );
        return;
    }
    let retrying = session.recovery_prompt.is_some();
    let body = session_load_failure_message(lang, message, retrying);
    let _ = manager
        .send_to_target(&target, &RichMessage::error(body))
        .await;
    if promote_fallback_session(bridge, manager, conn_mgr, db, &session).await {
        return;
    }
    if let Some(prompt) = session.recovery_prompt.clone() {
        retry_failed_session_load(
            db, manager, conn_mgr, emitter, bridge, data_dir, session, prompt, lang,
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn catch_up_session_load_failure(
    bridge: &Arc<Mutex<SessionBridge>>,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    db: &DatabaseConnection,
    emitter: &EventEmitter,
    data_dir: &Path,
    connection_id: &str,
) -> bool {
    let recovery_failed = match conn_mgr.get_state(connection_id).await {
        Some(state) => state.read().await.recovery_failed,
        None => true,
    };
    if !recovery_failed {
        return false;
    }
    let Some(session) = bridge.lock().await.remove(connection_id) else {
        return true;
    };
    let expected_external_id = session.restoring_external_id.clone();
    let lang = get_lang(db).await;
    let message = match lang {
        Lang::ZhCn | Lang::ZhTw => "原 Agent 会话不可恢复",
        _ => "the original Agent session could not be restored",
    };
    handle_session_load_failure(
        db,
        manager,
        conn_mgr,
        emitter,
        bridge,
        data_dir,
        connection_id,
        session,
        message,
        expected_external_id.as_deref(),
        lang,
    )
    .await;
    true
}

async fn send_pending_prompt(
    bridge: &Arc<Mutex<SessionBridge>>,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    db: &DatabaseConnection,
    connection_id: &str,
) {
    let pending = {
        let mut guard = bridge.lock().await;
        guard.get_mut(connection_id).and_then(|session| {
            session
                .pending_prompt
                .take()
                .map(|prompt| (prompt, session.target.clone()))
        })
    };
    let Some((prompt, target)) = pending else {
        return;
    };
    let blocks = vec![PromptInputBlock::Text {
        text: prompt.clone(),
    }];
    match conn_mgr.send_prompt(connection_id, blocks).await {
        Ok(()) => {}
        Err(crate::acp::error::AcpError::TurnInProgress) => {
            restore_deferred_prompt(bridge, manager, db, connection_id, prompt, target).await;
        }
        Err(error) => {
            tracing::error!(
                connection_id,
                error = %error,
                "[SessionEventSub] failed to send pending prompt"
            );
        }
    }
}

async fn restore_deferred_prompt(
    bridge: &Arc<Mutex<SessionBridge>>,
    manager: &ChatChannelManager,
    db: &DatabaseConnection,
    connection_id: &str,
    prompt: String,
    target: crate::chat_channel::types::ChannelMessageTarget,
) {
    let exhausted = {
        let mut guard = bridge.lock().await;
        let Some(session) = guard.get_mut(connection_id) else {
            return;
        };
        session.pending_prompt_attempts = session.pending_prompt_attempts.saturating_add(1);
        if session.pending_prompt_attempts >= MAX_KICKOFF_RETRIES {
            true
        } else {
            session.pending_prompt = Some(prompt);
            false
        }
    };
    if exhausted {
        let lang = get_lang(db).await;
        let body = match lang {
            Lang::ZhCn | Lang::ZhTw => "任务启动提示连续被占用，已放弃自动重试；请重新发送。",
            _ => "The kickoff prompt was repeatedly blocked; auto-retry stopped. Please resend.",
        };
        let _ = manager
            .send_to_target(&target, &RichMessage::error(body.to_string()))
            .await;
    } else {
        tracing::warn!(
            connection_id,
            "[SessionEventSub] kickoff deferred until TurnComplete"
        );
    }
}

#[allow(clippy::too_many_arguments)]
async fn retry_failed_session_load(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    emitter: &EventEmitter,
    bridge: &Arc<Mutex<SessionBridge>>,
    data_dir: &Path,
    session: ActiveSession,
    prompt: String,
    lang: Lang,
) {
    let route = conversation_binding_service::ConversationRoute {
        route_key: session.route_key,
        target_id: session.target_id,
    };
    let result = Box::pin(session_commands::resume_conversation_for_followup(
        db,
        session.channel_id,
        &session.sender_id,
        &session.target,
        &route,
        session.conversation_id,
        &prompt,
        manager,
        conn_mgr,
        emitter,
        bridge,
        data_dir,
        lang,
        session.trace_id.as_deref(),
    ))
    .await;
    if !result.body.trim().is_empty() {
        let _ = manager.send_to_target(&session.target, &result).await;
    }
}

async fn session_route(
    bridge: &Arc<Mutex<SessionBridge>>,
    connection_id: &str,
) -> Option<(i32, String)> {
    let guard = bridge.lock().await;
    guard
        .get(connection_id)
        .map(|session| (session.channel_id, session.route_key.clone()))
}

async fn session_route_target(
    bridge: &Arc<Mutex<SessionBridge>>,
    connection_id: &str,
) -> Option<(
    i32,
    String,
    crate::chat_channel::types::ChannelMessageTarget,
)> {
    let guard = bridge.lock().await;
    guard.get(connection_id).map(|session| {
        (
            session.channel_id,
            session.route_key.clone(),
            session.target.clone(),
        )
    })
}

async fn promote_fallback_session(
    bridge: &Arc<Mutex<SessionBridge>>,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    db: &DatabaseConnection,
    failed: &ActiveSession,
) -> bool {
    let candidate = {
        let mut guard = bridge.lock().await;
        let Some(candidate) = guard.fallback_candidate(failed) else {
            return false;
        };
        if !guard.is_latest_route_registration(&candidate.connection_id) {
            return false;
        }
        let route = conversation_binding_service::ConversationRoute {
            route_key: candidate.route_key.clone(),
            target_id: candidate.target_id.clone(),
        };
        let persisted = conversation_binding_service::persist_session_start(
            db,
            candidate.conversation_id,
            Some(&candidate.session_id),
            &candidate.session_id,
            Some((candidate.channel_id, &route)),
        )
        .await;
        if persisted.as_ref().ok().copied() != Some(true) {
            tracing::warn!(
                connection_id = %candidate.connection_id,
                conversation_id = candidate.conversation_id,
                result = ?persisted,
                "[ChatChannel] fallback generation persistence rejected"
            );
            return false;
        }
        let route_restored = if candidate.target.is_telegram_forum_topic() {
            let title = conversation_service::get_by_id(db, candidate.conversation_id)
                .await
                .ok()
                .and_then(|conversation| conversation.title);
            session_topic::bind_target(
                db,
                &candidate.target,
                candidate.conversation_id,
                Some(candidate.connection_id.clone()),
                &candidate.sender_id,
                title,
            )
            .await
            .map(|_| ())
        } else {
            sender_context_service::update_session(
                db,
                candidate.channel_id,
                &candidate.sender_id,
                Some(candidate.conversation_id),
                Some(candidate.connection_id.clone()),
            )
            .await
            .map(|_| ())
        };
        if let Err(error) = route_restored {
            tracing::error!(
                connection_id = %candidate.connection_id,
                conversation_id = candidate.conversation_id,
                error = %error,
                "[ChatChannel] fallback generation route restore failed"
            );
            return false;
        }
        guard.activate_fallback(&candidate).then_some(candidate)
    };
    let Some(candidate) = candidate else {
        return false;
    };
    tracing::info!(
        connection_id = %candidate.connection_id,
        conversation_id = candidate.conversation_id,
        "[ChatChannel] promoted prior started route generation"
    );
    send_pending_prompt(bridge, manager, conn_mgr, db, &candidate.connection_id).await;
    true
}

async fn complete_session_start(request: SessionStartRequest<'_>) -> bool {
    match persist_session_start(request).await {
        SessionStartPersistence::Missing | SessionStartPersistence::Superseded => false,
        SessionStartPersistence::Complete(replaced) => {
            disconnect_replaced_sessions(request, replaced).await;
            true
        }
        SessionStartPersistence::Rejected { session, error } => {
            tracing::warn!(
                connection_id = request.connection_id,
                conversation_id = session.conversation_id,
                error,
                "[ChatChannel] rejected stale SessionStarted persistence"
            );
            disconnect_rejected_session(request, session).await;
            false
        }
        SessionStartPersistence::BindFailed { session, error } => {
            fail_session_bind(request, session, &error).await;
            false
        }
    }
}

async fn persist_session_start(request: SessionStartRequest<'_>) -> SessionStartPersistence {
    let expected_external_id = session_started_expected_id(request).await;
    let mut guard = request.bridge.lock().await;
    let Some(session) = guard.get(request.connection_id) else {
        return SessionStartPersistence::Missing;
    };
    let context = session_start_context(session, expected_external_id);
    let updated = conversation_binding_service::persist_session_start(
        request.db,
        context.conversation_id,
        context.expected_external_id.as_deref(),
        request.session_id,
        context
            .route
            .as_ref()
            .map(|route| (context.channel_id, route)),
    )
    .await;
    match updated {
        Ok(true) => {}
        Ok(false) => return rejected_start(&mut guard, request.connection_id, "CAS rejected"),
        Err(error) => return rejected_start(&mut guard, request.connection_id, &error.to_string()),
    }
    if let Some(session) = guard.get_mut(request.connection_id) {
        session.expected_external_id = Some(request.session_id.to_string());
        session.observed_session_id = Some(request.session_id.to_string());
    }
    if !guard.is_latest_route_registration(request.connection_id) {
        return SessionStartPersistence::Superseded;
    }
    if !context.activate_route {
        return SessionStartPersistence::Complete(Vec::new());
    }
    match guard.activate_route(request.connection_id) {
        RouteActivation::Missing => SessionStartPersistence::Missing,
        RouteActivation::Superseded => SessionStartPersistence::Superseded,
        RouteActivation::Activated(replaced) => SessionStartPersistence::Complete(replaced),
    }
}

fn session_start_context(
    session: &ActiveSession,
    expected_external_id: Option<String>,
) -> SessionStartContext {
    SessionStartContext {
        conversation_id: session.conversation_id,
        channel_id: session.channel_id,
        expected_external_id: expected_external_id.or_else(|| session.expected_external_id.clone()),
        activate_route: session.bind_on_start,
        route: session
            .bind_on_start
            .then(|| conversation_binding_service::ConversationRoute {
                route_key: session.route_key.clone(),
                target_id: session.target_id.clone(),
            }),
    }
}

async fn session_started_expected_id(request: SessionStartRequest<'_>) -> Option<String> {
    let state = request.conn_mgr.get_state(request.connection_id).await?;
    let state = state.read().await;
    let transition = request
        .event_seq
        .and_then(|seq| state.session_started_transition(seq))
        .or_else(|| state.latest_session_started_transition())
        .filter(|transition| transition.session_id == request.session_id);
    transition
        .map(|transition| transition.expected_external_id.clone())
        .unwrap_or_else(|| state.requested_external_id.clone())
}

fn rejected_start(
    guard: &mut SessionBridge,
    connection_id: &str,
    error: &str,
) -> SessionStartPersistence {
    guard
        .remove(connection_id)
        .map(|session| SessionStartPersistence::Rejected {
            session,
            error: error.to_string(),
        })
        .unwrap_or(SessionStartPersistence::Missing)
}

fn bind_failed(
    guard: &mut SessionBridge,
    connection_id: &str,
    error: &str,
) -> SessionStartPersistence {
    guard
        .remove(connection_id)
        .map(|session| SessionStartPersistence::BindFailed {
            session,
            error: error.to_string(),
        })
        .unwrap_or(SessionStartPersistence::Missing)
}

async fn disconnect_replaced_sessions(
    request: SessionStartRequest<'_>,
    sessions: Vec<ActiveSession>,
) {
    for session in sessions {
        request
            .manager
            .typing_controller()
            .stop(
                request.manager,
                session.channel_id,
                &session.route_key,
                &session.connection_id,
            )
            .await;
        let _ = request.conn_mgr.disconnect(&session.connection_id).await;
    }
}

async fn disconnect_rejected_session(request: SessionStartRequest<'_>, session: ActiveSession) {
    request
        .manager
        .typing_controller()
        .stop(
            request.manager,
            session.channel_id,
            &session.route_key,
            &session.connection_id,
        )
        .await;
    let _ = request.conn_mgr.disconnect(&session.connection_id).await;
    session_topic::clear_route_if_connection(
        request.db,
        session.channel_id,
        &session.sender_id,
        &session.target,
        &session.connection_id,
    )
    .await;
    promote_fallback_session(
        request.bridge,
        request.manager,
        request.conn_mgr,
        request.db,
        &session,
    )
    .await;
}

async fn fail_session_bind(request: SessionStartRequest<'_>, session: ActiveSession, error: &str) {
    let target = session.target.clone();
    tracing::error!(
        channel_id = session.channel_id,
        conversation_id = session.conversation_id,
        error,
        "[ChatChannel] conversation bind failed"
    );
    let _ = request
        .conn_mgr
        .cancel(request.db, request.connection_id)
        .await;
    let _ = conversation_service::update_status(
        request.db,
        session.conversation_id,
        crate::db::entities::conversation::ConversationStatus::Cancelled,
    )
    .await;
    disconnect_rejected_session(request, session).await;
    let _ = request
        .manager
        .send_to_target(
            &target,
            &RichMessage::error("新对话绑定失败，原对话绑定已保留。"),
        )
        .await;
}

pub(super) async fn catch_up_session_start(
    bridge: &Arc<Mutex<SessionBridge>>,
    manager: &ChatChannelManager,
    conn_mgr: &ConnectionManager,
    db: &DatabaseConnection,
    connection_id: &str,
) {
    let external_id = match conn_mgr.get_state(connection_id).await {
        Some(state) => state.read().await.external_id.clone(),
        None => None,
    };
    let Some(external_id) = external_id else {
        return;
    };
    let request = SessionStartRequest {
        bridge,
        manager,
        conn_mgr,
        db,
        connection_id,
        session_id: &external_id,
        event_seq: None,
    };
    if complete_session_start(request).await {
        send_pending_prompt(bridge, manager, conn_mgr, db, connection_id).await;
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
