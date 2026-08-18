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
                channel_id,
                error = %error,
                trace_id = trace_id.unwrap_or(""),
                "[ChatChannel] failed to send response"
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
    // Providers cap UTF-8 bytes (WeCom 2048 bytes, Weixin similar). Keep room
    // for provider envelopes and split the fully rendered fallback only once.
    let chunks = split_utf8_chunks(&text, 1500);
    let mut last_id: Option<SentMessageId> = None;
    for chunk in chunks {
        let partial = RichMessage {
            title: None,
            body: chunk.to_string(),
            fields: Vec::new(),
            level: message.level,
        };
        let id = manager.send_to_target(target, &partial).await?;
        last_id = Some(id);
    }
    last_id.ok_or_else(|| ChatChannelError::SendFailed("empty message".to_string()))
}

/// Split by UTF-8 byte size without tearing a code point.
fn split_utf8_chunks(text: &str, max_bytes: usize) -> Vec<&str> {
    assert!(max_bytes >= 4, "chunk size must fit any UTF-8 code point");
    let mut chunks = Vec::new();
    let mut rest = text;
    while rest.len() > max_bytes {
        let mut cut = max_bytes;
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

#[cfg(test)]
mod tests {
    use super::split_utf8_chunks;

    #[test]
    fn splits_ascii_by_byte_budget() {
        let text = "a".repeat(3_001);
        let chunks = split_utf8_chunks(&text, 1_500);
        assert_eq!(
            chunks.iter().map(|chunk| chunk.len()).collect::<Vec<_>>(),
            [1_500, 1_500, 1]
        );
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn splits_multibyte_text_by_byte_budget() {
        let text = "消息渠道".repeat(600);
        let chunks = split_utf8_chunks(&text, 1_500);
        assert!(chunks.iter().all(|chunk| chunk.len() <= 1_500));
        assert_eq!(chunks.concat(), text);
    }
}
