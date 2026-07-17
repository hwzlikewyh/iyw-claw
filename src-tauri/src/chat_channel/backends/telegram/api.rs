use reqwest::{Client, RequestBuilder};

use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::types::{ChannelMessageTarget, SentMessageId};

use super::format::{send_message_body, topic_title};

pub(super) struct TelegramApi<'a> {
    client: &'a Client,
    bot_token: &'a str,
    default_chat_id: &'a str,
}

impl<'a> TelegramApi<'a> {
    pub(super) fn new(client: &'a Client, bot_token: &'a str, default_chat_id: &'a str) -> Self {
        Self {
            client,
            bot_token,
            default_chat_id,
        }
    }

    pub(super) async fn get_me(&self) -> Result<serde_json::Value, ChatChannelError> {
        let body = self
            .request_json(self.client.get(self.url("getMe")), FailureKind::Connection)
            .await?;
        if body.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            Ok(body)
        } else {
            Err(ChatChannelError::AuthenticationFailed(
                description(&body, "Invalid bot token").to_string(),
            ))
        }
    }

    pub(super) async fn resolve_chat_id(&self) -> Result<String, ChatChannelError> {
        if !self.default_chat_id.trim_start().starts_with('@') {
            return Ok(self.default_chat_id.to_string());
        }
        let body = serde_json::json!({ "chat_id": self.default_chat_id });
        let result = self
            .request_json(
                self.client.post(self.url("getChat")).json(&body),
                FailureKind::Send,
            )
            .await?;
        if result.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
            return Err(ChatChannelError::SendFailed(
                description(&result, "Telegram getChat failed").to_string(),
            ));
        }
        result
            .pointer("/result/id")
            .and_then(json_integer_to_string)
            .ok_or_else(|| {
                ChatChannelError::SendFailed(
                    "Telegram getChat returned no numeric chat id".to_string(),
                )
            })
    }

    pub(super) async fn send_text(
        &self,
        text: &str,
        parse_mode: Option<&str>,
        target: Option<&ChannelMessageTarget>,
        reply_markup: Option<serde_json::Value>,
    ) -> Result<SentMessageId, ChatChannelError> {
        let body = send_message_body(self.default_chat_id, text, parse_mode, target, reply_markup)?;
        let result = self
            .request_json(
                self.client.post(self.url("sendMessage")).json(&body),
                FailureKind::Send,
            )
            .await?;
        require_ok(&result, "Telegram sendMessage failed")?;
        let message_id = result
            .pointer("/result/message_id")
            .and_then(json_integer_to_string)
            .unwrap_or_default();
        Ok(SentMessageId(message_id))
    }

    pub(super) async fn create_topic(
        &self,
        channel_id: i32,
        canonical_chat_id: String,
        title: &str,
    ) -> Result<ChannelMessageTarget, ChatChannelError> {
        let body = serde_json::json!({
            "chat_id": canonical_chat_id,
            "name": topic_title(title),
        });
        let result = self
            .request_json(
                self.client.post(self.url("createForumTopic")).json(&body),
                FailureKind::Send,
            )
            .await?;
        require_ok(&result, "failed to create Telegram topic")?;
        let thread_id = result
            .pointer("/result/message_thread_id")
            .and_then(json_integer_to_string)
            .ok_or_else(|| {
                ChatChannelError::SendFailed(
                    "Telegram createForumTopic returned no message_thread_id".to_string(),
                )
            })?;
        Ok(ChannelMessageTarget::telegram_forum_topic(
            channel_id,
            canonical_chat_id,
            thread_id,
        ))
    }

    pub(super) async fn edit_topic(
        &self,
        target: &ChannelMessageTarget,
        title: &str,
    ) -> Result<(), ChatChannelError> {
        let thread_id = target
            .thread_key
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or_else(|| ChatChannelError::SendFailed("invalid Telegram topic id".into()))?;
        let chat_id = target.chat_id.as_deref().unwrap_or(self.default_chat_id);
        let body = serde_json::json!({
            "chat_id": chat_id,
            "message_thread_id": thread_id,
            "name": topic_title(title),
        });
        let result = self
            .request_json(
                self.client.post(self.url("editForumTopic")).json(&body),
                FailureKind::Send,
            )
            .await?;
        require_ok(&result, "failed to edit Telegram topic")
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{method}", self.bot_token)
    }

    async fn request_json(
        &self,
        request: RequestBuilder,
        failure: FailureKind,
    ) -> Result<serde_json::Value, ChatChannelError> {
        let response = request
            .send()
            .await
            .map_err(|error| failure.error(redact_token(error.to_string(), self.bot_token)))?;
        response
            .json()
            .await
            .map_err(|error| failure.error(redact_token(error.to_string(), self.bot_token)))
    }
}

#[derive(Clone, Copy)]
enum FailureKind {
    Connection,
    Send,
}

impl FailureKind {
    fn error(self, message: String) -> ChatChannelError {
        match self {
            Self::Connection => ChatChannelError::ConnectionFailed(message),
            Self::Send => ChatChannelError::SendFailed(message),
        }
    }
}

fn require_ok(result: &serde_json::Value, fallback: &str) -> Result<(), ChatChannelError> {
    if result.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(ChatChannelError::SendFailed(
            description(result, fallback).to_string(),
        ))
    }
}

fn description<'a>(result: &'a serde_json::Value, fallback: &'a str) -> &'a str {
    result
        .get("description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
}

fn json_integer_to_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_i64()
        .map(|number| number.to_string())
        .or_else(|| value.as_u64().map(|number| number.to_string()))
}

pub(super) fn redact_token(message: String, token: &str) -> String {
    if token.is_empty() {
        message
    } else {
        message.replace(token, "***")
    }
}
