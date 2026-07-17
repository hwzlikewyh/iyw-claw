use sea_orm::DatabaseConnection;

use super::manager::ChatChannelManager;
use super::session_dispatch::{CommandMessageResult, CommandPostAction, SessionCommandMessage};
use super::types::{ChannelMessageTarget, InteractiveMessage, RichMessage};

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
) {
    if message.is_silent() {
        return;
    }
    tracing::info!(
        "[ChatChannel] dispatch result: title={:?}, body_len={}",
        message.title(),
        message.body_len()
    );
    let result = match &message {
        DispatchMessage::Rich(message) => manager.send_to_target(&target, message).await,
        DispatchMessage::Interactive(message) => {
            manager.send_interactive_to_target(&target, message).await
        }
    };
    let (status, error) = match result {
        Ok(_) => ("sent", None),
        Err(error) => {
            tracing::error!(
                "[ChatChannel] failed to send response for {command_text:?} to channel {channel_id}: {error}"
            );
            ("failed", Some(error.to_string()))
        }
    };
    let _ = crate::db::service::chat_channel_message_log_service::create_log(
        db,
        channel_id,
        "outbound",
        "command_response",
        &message.to_plain_text(),
        status,
        error,
    )
    .await;
}
