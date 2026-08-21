use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use super::i18n::{self, Lang};
use super::session_bridge::SessionBridge;
use super::session_runtime;
use super::types::{ChannelMessageTarget, InteractiveMessage, RichMessage};
use crate::acp::manager::ConnectionManager;
use crate::db::entities::conversation;
use crate::db::service::{
    conversation_binding_service, conversation_service, sender_context_service,
    thread_binding_service,
};

pub struct CommandMessageResult {
    pub message: RichMessage,
    pub response_target: ChannelMessageTarget,
    pub extra_responses: Vec<(RichMessage, ChannelMessageTarget)>,
    pub post_action: Option<CommandPostAction>,
}

impl CommandMessageResult {
    pub(super) fn current(message: RichMessage, target: &ChannelMessageTarget) -> Self {
        Self {
            message,
            response_target: target.clone(),
            extra_responses: Vec::new(),
            post_action: None,
        }
    }
}

pub enum CommandPostAction {
    SendLinkedPrompt {
        connection_id: String,
        folder_id: i32,
        conversation_id: i32,
        text: String,
        channel_id: i32,
        sender_id: String,
        response_target: ChannelMessageTarget,
        lang: Lang,
        trace_id: Option<String>,
        route_key: String,
        registration_generation: u64,
        previous_binding: Option<(String, i32)>,
    },
}

pub enum SessionCommandMessage {
    Rich(RichMessage),
    Interactive(InteractiveMessage),
}

impl From<RichMessage> for SessionCommandMessage {
    fn from(message: RichMessage) -> Self {
        Self::Rich(message)
    }
}

pub async fn handle_post_action(
    action: CommandPostAction,
    db: &DatabaseConnection,
    connection_manager: &ConnectionManager,
    bridge: &Arc<Mutex<SessionBridge>>,
) -> Option<(RichMessage, ChannelMessageTarget)> {
    let CommandPostAction::SendLinkedPrompt {
        connection_id,
        folder_id,
        conversation_id,
        text,
        channel_id,
        sender_id,
        response_target,
        lang,
        trace_id,
        route_key,
        registration_generation,
        previous_binding,
    } = action;
    // Keep registration validation and prompt enqueue atomic with respect to
    // route replacement. Otherwise a newer generation could register between
    // the check and `send_prompt_linked`, receiving a kickoff for stale work.
    let send_result = {
        let _route_guard = SessionBridge::acquire_route_gate(bridge, channel_id, &route_key).await;
        let guard = bridge.lock().await;
        if !guard.is_latest_route_generation(&connection_id, registration_generation) {
            tracing::debug!(
                connection_id = %connection_id,
                generation = registration_generation,
                "[ChatChannel] skipped post-action after route generation changed"
            );
            return None;
        }
        drop(guard);
        // This second check is intentionally adjacent to the enqueue call.
        // The route gate held above prevents a concurrent registration from
        // changing the generation between this check and the send.
        let guard = bridge.lock().await;
        if !guard.is_latest_route_generation(&connection_id, registration_generation) {
            tracing::debug!(
                connection_id = %connection_id,
                generation = registration_generation,
                "[ChatChannel] skipped post-action at send boundary"
            );
            return None;
        }
        drop(guard);
        session_runtime::send_prompt_linked(
            db,
            connection_manager,
            &connection_id,
            folder_id,
            conversation_id,
            &text,
        )
        .await
    };

    // Record the kickoff in the message log so the end-to-end trace shows the
    // full chain even when the eventual AI reply fails later.
    let target_id =
        crate::db::service::chat_channel_target_service::find_by_target(db, &response_target)
            .await
            .ok()
            .flatten()
            .map(|registered| registered.target_id);
    let _ = crate::db::service::chat_channel_message_log_service::create_log_for_target(
        db,
        channel_id,
        "outbound",
        "session_kickoff",
        &text,
        if send_result.is_ok() {
            "sent"
        } else {
            "failed"
        },
        send_result.as_ref().err().map(|e| e.to_string()),
        trace_id,
        None,
        target_id,
    )
    .await;

    if send_result.is_ok() {
        return None;
    }
    cleanup_failed_prompt(
        db,
        connection_manager,
        bridge,
        &connection_id,
        conversation_id,
        channel_id,
        &sender_id,
        &response_target,
        &route_key,
        previous_binding.as_ref(),
    )
    .await;
    Some((
        RichMessage::error(i18n::failed_to_send_message_label(lang)),
        response_target,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn cleanup_failed_prompt(
    db: &DatabaseConnection,
    connection_manager: &ConnectionManager,
    bridge: &Arc<Mutex<SessionBridge>>,
    connection_id: &str,
    conversation_id: i32,
    channel_id: i32,
    sender_id: &str,
    target: &ChannelMessageTarget,
    route_key: &str,
    previous_binding: Option<&(String, i32)>,
) {
    bridge.lock().await.remove(connection_id);
    if target.is_telegram_forum_topic() {
        if let Ok(Some(binding)) = thread_binding_service::get_by_target(db, target).await {
            let _ =
                thread_binding_service::clear_connection_if_matches(db, binding.id, connection_id)
                    .await;
        }
    } else {
        let _ = sender_context_service::clear_session_if_connection_matches(
            db,
            channel_id,
            sender_id,
            connection_id,
        )
        .await;
    }
    let _ = connection_manager.cancel(db, connection_id).await;
    let _ = conversation_service::update_status(
        db,
        conversation_id,
        conversation::ConversationStatus::Cancelled,
    )
    .await;

    // Roll back only if this failed candidate still owns the route.
    let previous =
        previous_binding.map(|(target_id, conversation_id)| (target_id.as_str(), *conversation_id));
    let _ = conversation_binding_service::rollback_if_current(
        db,
        conversation_binding_service::BindingRollback {
            channel_id,
            route_key,
            failed_conversation_id: conversation_id,
            previous,
        },
    )
    .await;
}
