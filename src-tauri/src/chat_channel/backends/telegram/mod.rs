mod api;
mod format;
mod poll;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex, OnceCell};

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::traits::ChatChannelBackend;
use crate::chat_channel::types::*;

use api::TelegramApi;
use format::{format_markdown, inline_keyboard};

pub struct TelegramBackend {
    bot_token: String,
    chat_id: String,
    topic_mode: bool,
    client: reqwest::Client,
    status: Arc<Mutex<ChannelConnectionStatus>>,
    channel_id: i32,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::watch::Sender<bool>>>>,
    resolved_chat_id: OnceCell<String>,
}

impl TelegramBackend {
    pub fn new(channel_id: i32, bot_token: String, chat_id: String, topic_mode: bool) -> Self {
        Self {
            bot_token,
            chat_id,
            topic_mode,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            status: Arc::new(Mutex::new(ChannelConnectionStatus::Disconnected)),
            channel_id,
            shutdown_tx: Arc::new(Mutex::new(None)),
            resolved_chat_id: OnceCell::new(),
        }
    }

    fn api(&self) -> TelegramApi<'_> {
        TelegramApi::new(&self.client, &self.bot_token, &self.chat_id)
    }

    async fn canonical_chat_id(&self) -> Result<String, ChatChannelError> {
        if !self.chat_id.trim_start().starts_with('@') {
            return Ok(self.chat_id.clone());
        }
        self.resolved_chat_id
            .get_or_try_init(|| async {
                let api = self.api();
                api.resolve_chat_id().await
            })
            .await
            .cloned()
    }

    async fn send_rich(
        &self,
        message: &RichMessage,
        target: Option<&ChannelMessageTarget>,
    ) -> Result<SentMessageId, ChatChannelError> {
        let markdown = format_markdown(message);
        match self
            .api()
            .send_text(&markdown, Some("MarkdownV2"), target, None)
            .await
        {
            Ok(message_id) => Ok(message_id),
            Err(error) => {
                tracing::warn!(
                    "[Telegram] MarkdownV2 send failed: {error}, retrying as plain text"
                );
                self.api()
                    .send_text(&message.to_plain_text(), None, target, None)
                    .await
            }
        }
    }

    async fn send_interactive(
        &self,
        message: &InteractiveMessage,
        target: Option<&ChannelMessageTarget>,
    ) -> Result<SentMessageId, ChatChannelError> {
        let keyboard = inline_keyboard(message);
        let markdown = format_markdown(&message.base);
        match self
            .api()
            .send_text(&markdown, Some("MarkdownV2"), target, keyboard.clone())
            .await
        {
            Ok(message_id) => Ok(message_id),
            Err(error) => {
                tracing::warn!(
                    "[Telegram] MarkdownV2 interactive send failed: {error}, retrying as plain text"
                );
                self.api()
                    .send_text(&message.base.to_plain_text(), None, target, keyboard)
                    .await
            }
        }
    }
}

#[async_trait]
impl ChatChannelBackend for TelegramBackend {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Telegram
    }

    async fn start(
        &self,
        command_tx: mpsc::Sender<IncomingCommand>,
    ) -> Result<(), ChatChannelError> {
        *self.status.lock().await = ChannelConnectionStatus::Connecting;
        let bot = match self.api().get_me().await {
            Ok(bot) => bot,
            Err(error) => {
                *self.status.lock().await = ChannelConnectionStatus::Error;
                return Err(error);
            }
        };
        let bot_username = bot
            .pointer("/result/username")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let configured_chat_id = self
            .canonical_chat_id()
            .await
            .unwrap_or_else(|_| self.chat_id.clone());
        *self.status.lock().await = ChannelConnectionStatus::Connected;
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);
        let context = poll::PollContext {
            client: self.client.clone(),
            bot_token: self.bot_token.clone(),
            configured_chat_id,
            bot_username,
            channel_id: self.channel_id,
            topic_mode: self.topic_mode,
            status: self.status.clone(),
        };
        tokio::spawn(poll::run(context, command_tx, shutdown_rx));
        Ok(())
    }

    async fn stop(&self) -> Result<(), ChatChannelError> {
        if let Some(sender) = self.shutdown_tx.lock().await.take() {
            let _ = sender.send(true);
        }
        *self.status.lock().await = ChannelConnectionStatus::Disconnected;
        Ok(())
    }

    async fn status(&self) -> ChannelConnectionStatus {
        *self.status.lock().await
    }

    async fn send_message(&self, text: &str) -> Result<SentMessageId, ChatChannelError> {
        self.api().send_text(text, None, None, None).await
    }

    async fn send_rich_message(
        &self,
        message: &RichMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_rich(message, None).await
    }

    async fn send_rich_message_to(
        &self,
        message: &RichMessage,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_rich(message, Some(target)).await
    }

    async fn send_interactive_message(
        &self,
        message: &InteractiveMessage,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_interactive(message, None).await
    }

    async fn send_interactive_message_to(
        &self,
        message: &InteractiveMessage,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        self.send_interactive(message, Some(target)).await
    }

    async fn create_thread(&self, title: &str) -> Result<ChannelMessageTarget, ChatChannelError> {
        if !self.topic_mode {
            return Err(ChatChannelError::Unsupported(
                "Telegram topic mode is not enabled".into(),
            ));
        }
        let chat_id = self.canonical_chat_id().await?;
        self.api()
            .create_topic(self.channel_id, chat_id, title)
            .await
    }

    async fn edit_thread_title(
        &self,
        target: &ChannelMessageTarget,
        title: &str,
    ) -> Result<(), ChatChannelError> {
        if !target.is_telegram_forum_topic() {
            return Err(ChatChannelError::Unsupported(
                "target is not a Telegram forum topic".into(),
            ));
        }
        self.api().edit_topic(target, title).await
    }

    async fn test_connection(&self) -> Result<(), ChatChannelError> {
        self.api().get_me().await.map(|_| ())
    }
}
