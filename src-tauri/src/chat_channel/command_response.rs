use sea_orm::DatabaseConnection;

use super::error::ChatChannelError;
use super::manager::ChatChannelManager;
use super::session_dispatch::{CommandMessageResult, CommandPostAction, SessionCommandMessage};
use super::types::{ChannelMessageTarget, InteractiveMessage, RichMessage, SentMessageId};

pub(super) struct DispatchResponse {
    pub message: Option<DispatchMessage>,
    pub target: ChannelMessageTarget,
    pub extra_messages: Vec<(DispatchMessage, ChannelMessageTarget)>,
    pub post_action: Option<CommandPostAction>,
}

impl DispatchResponse {
    pub(super) fn current(message: RichMessage, target: &ChannelMessageTarget) -> Self {
        Self {
            message: Some(DispatchMessage::Rich(message)),
            target: target.clone(),
            extra_messages: Vec::new(),
            post_action: None,
        }
    }

    pub(super) fn from_session_message(
        message: SessionCommandMessage,
        target: &ChannelMessageTarget,
    ) -> Self {
        let message = match message {
            SessionCommandMessage::Rich(message) => DispatchMessage::Rich(message),
            SessionCommandMessage::Interactive(message) => DispatchMessage::Interactive(message),
        };
        Self {
            message: Some(message),
            target: target.clone(),
            extra_messages: Vec::new(),
            post_action: None,
        }
    }

    pub(super) fn from_command_result(result: CommandMessageResult) -> Self {
        Self {
            message: Some(DispatchMessage::Rich(result.message)),
            target: result.response_target,
            extra_messages: result
                .extra_responses
                .into_iter()
                .map(|(message, target)| (DispatchMessage::Rich(message), target))
                .collect(),
            post_action: result.post_action,
        }
    }

    pub(super) fn none(target: &ChannelMessageTarget) -> Self {
        Self {
            message: None,
            target: target.clone(),
            extra_messages: Vec::new(),
            post_action: None,
        }
    }

    pub(super) fn take_messages(&mut self) -> Vec<(DispatchMessage, ChannelMessageTarget)> {
        let mut messages = Vec::new();
        if let Some(message) = self.message.take() {
            messages.push((message, self.target.clone()));
        }
        messages.append(&mut self.extra_messages);
        messages
    }
}

pub(super) enum DispatchMessage {
    Rich(RichMessage),
    Interactive(InteractiveMessage),
}

impl DispatchMessage {
    fn title(&self) -> Option<&String> {
        match self {
            Self::Rich(message) => message.title.as_ref(),
            Self::Interactive(message) => message.base.title.as_ref(),
        }
    }

    fn body_len(&self) -> usize {
        match self {
            Self::Rich(message) => message.body.len(),
            Self::Interactive(message) => message.base.body.len(),
        }
    }

    fn is_silent(&self) -> bool {
        match self {
            Self::Rich(message) => message.is_silent(),
            Self::Interactive(message) => message.base.is_silent() && message.buttons.is_empty(),
        }
    }

    fn to_plain_text(&self) -> String {
        match self {
            Self::Rich(message) => message.to_plain_text(),
            Self::Interactive(message) => message.to_rich_fallback().to_plain_text(),
        }
    }
}

pub(super) async fn send_dispatch_message(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    channel_id: i32,
    command_text: &str,
    message: DispatchMessage,
    target: ChannelMessageTarget,
    trace_id: Option<&str>,
) {
    if message.is_silent() {
        return;
    }
    tracing::info!(
        "[ChatChannel] dispatch result: title={:?}, body_len={} trace={}",
        message.title(),
        message.body_len(),
        trace_id.unwrap_or("")
    );
    // Long replies are split before sending so the provider never truncates
    // mid-turn; chunks keep the same trace id and go out in order.
    let result = match &message {
        DispatchMessage::Rich(message) => send_long_message(manager, &target, message).await,
        DispatchMessage::Interactive(message) => {
            manager.send_interactive_to_target(&target, message).await
        }
    };
    let (status, error_code, provider_message_id) = match result {
        Ok(sent_id) => ("sent", None, Some(sent_id.0)),
        Err(error) => {
            tracing::error!(
                "[ChatChannel] failed to send response for {command_text:?} to channel {channel_id}: {error}"
            );
            ("failed", Some("CHANNEL_SEND_FAILED".to_string()), None)
        }
    };
    let target_id = crate::db::service::chat_channel_target_service::find_by_target(db, &target)
        .await
        .ok()
        .flatten()
        .map(|registered| registered.target_id);
    let _ = crate::db::service::chat_channel_message_log_service::create_log_for_target(
        db,
        channel_id,
        "outbound",
        "command_response",
        &message.to_plain_text(),
        status,
        error_code,
        trace_id.map(|s| s.to_string()),
        provider_message_id,
        target_id,
    )
    .await;
}

/// Split long text replies into provider-safe chunks (order preserved, same
/// trace id), returning the last provider message id.
async fn send_long_message(
    manager: &ChatChannelManager,
    target: &ChannelMessageTarget,
    message: &RichMessage,
) -> Result<SentMessageId, ChatChannelError> {
    let text = message.to_plain_text();
    // Providers cap message length (WeCom 2048 bytes, Weixin similar); 1500
    // chars is a conservative safe size.
    let chunks = split_utf8_chunks(&text, 1500);
    let mut last_id: Option<SentMessageId> = None;
    for chunk in chunks {
        let mut partial = message.clone();
        partial.body = chunk.to_string();
        partial.fields = Vec::new();
        let id = manager.send_to_target(target, &partial).await?;
        last_id = Some(id);
    }
    last_id.ok_or_else(|| ChatChannelError::SendFailed("empty message".to_string()))
}

/// Split on char boundaries so multibyte text is never torn.
fn split_utf8_chunks(text: &str, max_chars: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut rest = text;
    while rest.chars().count() > max_chars {
        let mut cut = max_chars;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let (head, tail) = rest.split_at(cut);
        chunks.push(head);
        rest = tail;
    }
    chunks.push(rest);
    chunks
}
