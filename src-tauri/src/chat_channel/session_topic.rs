use std::sync::Arc;
use std::time::Instant;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use super::i18n;
use super::session_bridge::{ActiveSession, SessionBridge};
use super::session_commands::FollowupRequest;
use super::session_runtime;
use super::session_topic_messages;
use super::types::{ChannelMessageTarget, RichMessage};
use crate::db::entities::chat_channel_thread_binding;
use crate::db::service::{conversation_service, sender_context_service, thread_binding_service};
use crate::models::agent::AgentType;

pub(super) struct CommandSessionRef {
    pub connection_id: String,
    pub conversation_id: Option<i32>,
}

pub(super) async fn has_active_session(
    _db: &DatabaseConnection,
    bridge: &Arc<Mutex<SessionBridge>>,
    channel_id: i32,
    route_key: &str,
) -> bool {
    bridge
        .lock()
        .await
        .find_by_route(channel_id, route_key)
        .is_some()
}

pub(super) async fn command_session_ref(
    _db: &DatabaseConnection,
    bridge: &Arc<Mutex<SessionBridge>>,
    channel_id: i32,
    _sender_id: &str,
    route_key: &str,
) -> Result<Option<CommandSessionRef>, crate::db::error::DbError> {
    let guard = bridge.lock().await;
    Ok(guard
        .find_by_route(channel_id, route_key)
        .map(|session| CommandSessionRef {
            connection_id: session.connection_id.clone(),
            conversation_id: Some(session.conversation_id),
        }))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn bind_target(
    db: &DatabaseConnection,
    target: &ChannelMessageTarget,
    conversation_id: i32,
    connection_id: Option<String>,
    sender_id: &str,
    display_title: Option<String>,
) -> Result<chat_channel_thread_binding::Model, crate::db::error::DbError> {
    thread_binding_service::upsert_for_target(
        db,
        thread_binding_service::ThreadBindingUpsert {
            target,
            channel_type: "telegram",
            conversation_id,
            connection_id,
            created_by_sender_id: sender_id,
            display_title,
        },
    )
    .await
}

/// Clear only the route state owned by `connection_id`. This protects a newer
/// session from a late disconnect/error event emitted by an older connection.
pub(super) async fn clear_route_if_connection(
    db: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    connection_id: &str,
) {
    if target.is_telegram_forum_topic() {
        if let Ok(Some(binding)) = thread_binding_service::get_by_target(db, target).await {
            let _ =
                thread_binding_service::clear_connection_if_matches(db, binding.id, connection_id)
                    .await;
        }
    } else {
        let _ = sender_context_service::clear_connection_if_matches(
            db,
            channel_id,
            sender_id,
            connection_id,
        )
        .await;
    }
}

pub(super) async fn handle_followup(req: FollowupRequest<'_>) -> RichMessage {
    let binding = match thread_binding_service::get_by_target(req.db, req.target).await {
        Ok(Some(binding)) => binding,
        Ok(None) => {
            return RichMessage::info(session_topic_messages::no_session(req.lang, req.prefix))
        }
        Err(error) => {
            return RichMessage::error(format!(
                "{}{error}",
                i18n::failed_to_load_context_label(req.lang)
            ))
        }
    };
    if let Some(reference) = active_binding_ref(req.bridge, req.target, &binding).await {
        return send_to_session(req, reference).await;
    }
    if binding.connection_id.is_some() {
        if let Some(connection_id) = binding.connection_id.as_deref() {
            let _ = thread_binding_service::clear_connection_if_matches(
                req.db,
                binding.id,
                connection_id,
            )
            .await;
        }
    }
    resume_binding(req, binding).await
}

async fn active_binding_ref(
    bridge: &Arc<Mutex<SessionBridge>>,
    target: &ChannelMessageTarget,
    binding: &chat_channel_thread_binding::Model,
) -> Option<CommandSessionRef> {
    let guard = bridge.lock().await;
    if let Some(session) = guard.find_by_target(target) {
        return Some(CommandSessionRef {
            connection_id: session.connection_id.clone(),
            conversation_id: Some(session.conversation_id),
        });
    }
    binding.connection_id.as_ref().and_then(|connection_id| {
        guard.get(connection_id).map(|_| CommandSessionRef {
            connection_id: connection_id.clone(),
            conversation_id: Some(binding.conversation_id),
        })
    })
}

async fn send_to_session(req: FollowupRequest<'_>, reference: CommandSessionRef) -> RichMessage {
    // Stamp the session with this follow-up's trace so the outbound reply
    // links back to the message that triggered it.
    if let Some(trace) = req.trace_id {
        let mut guard = req.bridge.lock().await;
        if let Some(session) = guard.get_mut(&reference.connection_id) {
            session.trace_id = Some(trace.to_string());
        }
    }
    match session_runtime::send_prompt(req.conn_mgr, &reference.connection_id, req.text).await {
        Ok(()) | Err(crate::acp::error::AcpError::TurnInProgress) => return RichMessage::info(""),
        Err(error) => {
            tracing::warn!(
                "[ChatChannel] failed to send Telegram topic follow-up on {}: {error}",
                reference.connection_id
            );
        }
    }
    req.bridge.lock().await.remove(&reference.connection_id);
    clear_route_if_connection(
        req.db,
        req.channel_id,
        req.sender_id,
        req.target,
        &reference.connection_id,
    )
    .await;
    RichMessage::info("")
}

async fn resume_binding(
    req: FollowupRequest<'_>,
    binding: chat_channel_thread_binding::Model,
) -> RichMessage {
    let conversation = match conversation_service::get_by_id(req.db, binding.conversation_id).await
    {
        Ok(conversation) => conversation,
        Err(_) => return RichMessage::info(i18n::conversation_not_found(req.lang)),
    };
    let (connection_id, folder) = match session_runtime::spawn_for_conversation(
        req.db,
        &conversation,
        req.channel_id,
        req.sender_id,
        req.target,
        req.conn_mgr,
        req.emitter,
        req.data_dir,
    )
    .await
    {
        Ok(started) => started,
        Err(error) => {
            return RichMessage::error(session_topic_messages::resume_failed(
                req.lang,
                conversation.id,
                &error,
            ))
        }
    };
    register_session(req.bridge, req, &conversation, &connection_id).await;
    if bind_target(
        req.db,
        req.target,
        conversation.id,
        Some(connection_id.clone()),
        req.sender_id,
        conversation.title.clone(),
    )
    .await
    .is_err()
    {
        cleanup_resume(req, &connection_id, binding.id).await;
        return RichMessage::error("Failed to bind Telegram topic");
    }
    remember_topic_preferences(req, conversation.folder_id, conversation.agent_type).await;
    if session_runtime::send_prompt_linked(
        req.db,
        req.conn_mgr,
        &connection_id,
        folder.id,
        conversation.id,
        req.text,
    )
    .await
    .is_err()
    {
        cleanup_resume(req, &connection_id, binding.id).await;
    }
    RichMessage::info("")
}

async fn register_session(
    bridge: &Arc<Mutex<SessionBridge>>,
    req: FollowupRequest<'_>,
    conversation: &crate::models::conversation::DbConversationSummary,
    connection_id: &str,
) {
    bridge.lock().await.register(
        connection_id.to_string(),
        ActiveSession {
            channel_id: req.channel_id,
            sender_id: req.sender_id.to_string(),
            target: req.target.clone(),
            route_key: req.route.route_key.clone(),
            target_id: req.route.target_id.clone(),
            bind_on_start: false,
            conversation_id: conversation.id,
            connection_id: connection_id.to_string(),
            agent_type: conversation.agent_type,
            content_buffer: String::new(),
            tool_calls: Vec::new(),
            tool_call_inputs: Default::default(),
            delegation_rendered: Default::default(),
            last_flushed: Instant::now(),
            pending_prompt: None,
            recovery_prompt: None,
            pending_prompt_attempts: 0,
            trace_id: req.trace_id.map(|s| s.to_string()),
            permission_pending: None,
        },
    );
}

async fn remember_topic_preferences(
    req: FollowupRequest<'_>,
    folder_id: i32,
    agent_type: AgentType,
) {
    let _ = sender_context_service::update_folder(
        req.db,
        req.channel_id,
        req.sender_id,
        Some(folder_id),
    )
    .await;
    let agent = serde_json::to_value(agent_type)
        .ok()
        .and_then(|value| value.as_str().map(ToString::to_string));
    let _ =
        sender_context_service::update_agent(req.db, req.channel_id, req.sender_id, agent).await;
}

async fn cleanup_resume(req: FollowupRequest<'_>, connection_id: &str, binding_id: i32) {
    req.bridge.lock().await.remove(connection_id);
    let _ = thread_binding_service::clear_connection_if_matches(req.db, binding_id, connection_id)
        .await;
    let _ = req.conn_mgr.cancel(req.db, connection_id).await;
}
