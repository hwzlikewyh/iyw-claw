use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use super::i18n::{self, Lang};
use super::session_bridge::SessionBridge;
use super::session_runtime;
use super::types::{ChannelMessageTarget, InteractiveMessage, RichMessage};
use crate::acp::manager::ConnectionManager;
use crate::db::entities::conversation;
use crate::db::service::{conversation_service, sender_context_service, thread_binding_service};

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
    } = action;
    let send_result = session_runtime::send_prompt_linked(
        db,
        connection_manager,
        &connection_id,
        folder_id,
        conversation_id,
        &text,
    )
    .await;

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
) {
    bridge.lock().await.remove(connection_id);
    if target.is_telegram_forum_topic() {
        if let Ok(Some(binding)) = thread_binding_service::get_by_target(db, target).await {
            let _ = thread_binding_service::clear_connection(db, binding.id).await;
        }
    } else {
        let _ = sender_context_service::clear_session(db, channel_id, sender_id).await;
    }
    let _ = connection_manager.cancel(db, connection_id).await;
    let _ = conversation_service::update_status(
        db,
        conversation_id,
        conversation::ConversationStatus::Cancelled,
    )
    .await;
}
