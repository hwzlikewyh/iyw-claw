use sea_orm::DatabaseConnection;

use super::error::ChatChannelError;
use super::manager::ChatChannelManager;
use super::types::{
    ChannelMessageTarget, InteractiveMessage, RichMessage, SentMessageId,
    TELEGRAM_FORUM_THREAD_KIND,
};
use crate::db::entities::chat_channel_thread_binding;
use crate::db::service::thread_binding_service;

impl ChatChannelManager {
    pub async fn send_to_target(
        &self,
        target: &ChannelMessageTarget,
        message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.backend_for(target.channel_id)
            .await?
            .send_rich_message_to(message, target)
            .await
    }

    pub async fn send_interactive_to_target(
        &self,
        target: &ChannelMessageTarget,
        message: &InteractiveMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.backend_for(target.channel_id)
            .await?
            .send_interactive_message_to(message, target)
            .await
    }

    pub async fn create_thread(
        &self,
        channel_id: i32,
        title: &str,
    ) -> Result<ChannelMessageTarget, ChatChannelError> {
        self.backend_for(channel_id)
            .await?
            .create_thread(title)
            .await
    }

    pub async fn edit_thread_title(
        &self,
        target: &ChannelMessageTarget,
        title: &str,
    ) -> Result<(), ChatChannelError> {
        self.backend_for(target.channel_id)
            .await?
            .edit_thread_title(target, title)
            .await
    }

    pub async fn sync_conversation_title(
        &self,
        db: &DatabaseConnection,
        conversation_id: i32,
        title: &str,
    ) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        let bindings = match thread_binding_service::list_by_conversation(db, conversation_id).await
        {
            Ok(bindings) => bindings,
            Err(error) => {
                tracing::warn!(
                    "[ChatChannel] failed to load thread bindings for conversation {conversation_id}: {error}"
                );
                return;
            }
        };
        let topic_title = topic_title_for_conversation(conversation_id, title);
        for binding in bindings {
            self.sync_binding_title(db, conversation_id, binding, &topic_title)
                .await;
        }
    }

    async fn sync_binding_title(
        &self,
        db: &DatabaseConnection,
        conversation_id: i32,
        binding: chat_channel_thread_binding::Model,
        topic_title: &str,
    ) {
        if binding.thread_kind != TELEGRAM_FORUM_THREAD_KIND || !binding.title_sync_enabled {
            return;
        }
        let target = target_from_binding(&binding);
        match self.edit_thread_title(&target, topic_title).await {
            Ok(()) => {
                let _ = thread_binding_service::update_display_title(
                    db,
                    binding.id,
                    topic_title.to_string(),
                )
                .await;
            }
            Err(error) => tracing::warn!(
                "[ChatChannel] failed to sync Telegram topic title for conversation {conversation_id}: {error}"
            ),
        }
    }
}

fn target_from_binding(binding: &chat_channel_thread_binding::Model) -> ChannelMessageTarget {
    ChannelMessageTarget {
        channel_id: binding.channel_id,
        chat_id: Some(binding.chat_id.clone()),
        thread_key: Some(binding.thread_key.clone()),
        thread_kind: Some(binding.thread_kind.clone()),
        provider_payload: binding
            .provider_payload_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok()),
    }
}

fn topic_title_for_conversation(conversation_id: i32, title: &str) -> String {
    format!("#{conversation_id} {title}")
        .chars()
        .take(128)
        .collect()
}
