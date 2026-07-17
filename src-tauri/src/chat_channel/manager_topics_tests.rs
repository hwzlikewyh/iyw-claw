use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use super::error::ChatChannelError;
use super::manager::ChatChannelManager;
use super::traits::ChatChannelBackend;
use super::types::*;
use crate::db::service::{chat_channel_service, thread_binding_service};
use crate::db::test_helpers::{fresh_in_memory_db, seed_conversation, seed_folder};
use crate::models::AgentType;

#[derive(Default)]
struct Recorder {
    sent_targets: Arc<Mutex<Vec<ChannelMessageTarget>>>,
    edited_titles: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ChatChannelBackend for Recorder {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Telegram
    }

    async fn start(&self, _tx: mpsc::Sender<IncomingCommand>) -> Result<(), ChatChannelError> {
        Ok(())
    }

    async fn stop(&self) -> Result<(), ChatChannelError> {
        Ok(())
    }

    async fn status(&self) -> ChannelConnectionStatus {
        ChannelConnectionStatus::Connected
    }

    async fn send_message(&self, _text: &str) -> Result<SentMessageId, ChatChannelError> {
        Ok(SentMessageId("sent".into()))
    }

    async fn send_rich_message(
        &self,
        _message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        Ok(SentMessageId("sent".into()))
    }

    async fn send_rich_message_to(
        &self,
        _message: &RichMessage,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.sent_targets.lock().await.push(target.clone());
        Ok(SentMessageId("sent".into()))
    }

    async fn edit_thread_title(
        &self,
        _target: &ChannelMessageTarget,
        title: &str,
    ) -> Result<(), ChatChannelError> {
        self.edited_titles.lock().await.push(title.to_string());
        Ok(())
    }

    async fn test_connection(&self) -> Result<(), ChatChannelError> {
        Ok(())
    }
}

#[tokio::test]
async fn target_send_preserves_topic_identity() {
    let manager = ChatChannelManager::new();
    let recorder = Recorder::default();
    let targets = recorder.sent_targets.clone();
    manager
        .add_channel(
            7,
            "Telegram".into(),
            ChannelType::Telegram,
            Box::new(recorder),
        )
        .await
        .expect("add channel");
    let target = ChannelMessageTarget::telegram_forum_topic(7, "-100123", "42");

    manager
        .send_to_target(&target, &RichMessage::info("hello"))
        .await
        .expect("send");

    assert_eq!(targets.lock().await.as_slice(), &[target]);
}

#[tokio::test]
async fn conversation_title_sync_updates_bound_topic() {
    let db = fresh_in_memory_db().await;
    let channel = chat_channel_service::create(
        &db.conn,
        "Telegram".into(),
        "telegram".into(),
        "{}".into(),
        true,
        false,
        None,
    )
    .await
    .expect("channel");
    let folder = seed_folder(&db, "/tmp/iyw-claw-topic-title").await;
    let conversation = seed_conversation(&db, folder, AgentType::Codex).await;
    let target = ChannelMessageTarget::telegram_forum_topic(channel.id, "-100123", "42");
    thread_binding_service::upsert_for_target(
        &db.conn,
        thread_binding_service::ThreadBindingUpsert {
            target: &target,
            channel_type: "telegram",
            conversation_id: conversation,
            connection_id: None,
            created_by_sender_id: "sender",
            display_title: None,
        },
    )
    .await
    .expect("binding");
    let manager = ChatChannelManager::new();
    let recorder = Recorder::default();
    let titles = recorder.edited_titles.clone();
    manager
        .add_channel(
            channel.id,
            "Telegram".into(),
            ChannelType::Telegram,
            Box::new(recorder),
        )
        .await
        .expect("add channel");

    manager
        .sync_conversation_title(&db.conn, conversation, "Renamed")
        .await;

    assert_eq!(
        titles.lock().await.as_slice(),
        &[format!("#{conversation} Renamed")]
    );
}
