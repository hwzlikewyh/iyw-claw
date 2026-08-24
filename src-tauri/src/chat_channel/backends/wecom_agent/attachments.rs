use super::client::{SendError, SendReceipt};
use super::client_media::MediaKind;
use super::WecomAgentBackend;
use crate::chat_channel::attachments::{AttachmentCapability, ChannelAttachment};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::types::SentMessageId;

const MAX_ATTACHMENT_BYTES: u64 = 10 * 1024 * 1024;

pub(super) fn capability() -> AttachmentCapability {
    AttachmentCapability {
        supported: true,
        max_file_bytes: Some(MAX_ATTACHMENT_BYTES),
    }
}

impl WecomAgentBackend {
    pub(super) async fn send_attachment_to_user(
        &self,
        user_id: &str,
        attachment: &ChannelAttachment,
    ) -> Result<SentMessageId, ChatChannelError> {
        let user_id = user_id.trim();
        validate_attachment(user_id, attachment)?;
        let kind = media_kind(&attachment.mime_type);
        let token = self.access_token(false, None).await?;
        let receipt = match self
            .upload_and_send(&token, user_id, kind, attachment)
            .await
        {
            Ok(receipt) => receipt,
            Err(SendError::TokenInvalid { code, message }) => {
                tracing::warn!(
                    channel_id = self.channel_id,
                    provider_code = code,
                    provider_message = message,
                    "[WeCom Agent] media token expired; refreshing once"
                );
                let refreshed = self.access_token(true, Some(&token)).await?;
                self.upload_and_send(&refreshed, user_id, kind, attachment)
                    .await
                    .map_err(ChatChannelError::from)?
            }
            Err(error) => return Err(ChatChannelError::from(error)),
        };
        Ok(sent_id(receipt))
    }

    async fn upload_and_send(
        &self,
        token: &str,
        user_id: &str,
        kind: MediaKind,
        attachment: &ChannelAttachment,
    ) -> Result<SendReceipt, SendError> {
        let media_id = self.client.upload_media(token, kind, attachment).await?;
        self.client
            .send_media(token, user_id, self.agent_id, kind, &media_id)
            .await
    }
}

fn validate_attachment(
    user_id: &str,
    attachment: &ChannelAttachment,
) -> Result<(), ChatChannelError> {
    if user_id.is_empty() {
        return Err(ChatChannelError::ConfigurationInvalid(
            "WeCom target UserID is missing".to_string(),
        ));
    }
    if attachment.byte_len() == 0 || attachment.byte_len() > MAX_ATTACHMENT_BYTES {
        return Err(ChatChannelError::SendFailed(
            "WeCom attachment must be between 1 byte and 10 MB".to_string(),
        ));
    }
    Ok(())
}

fn media_kind(mime_type: &str) -> MediaKind {
    match mime_type {
        "image/jpeg" | "image/png" => MediaKind::Image,
        _ => MediaKind::File,
    }
}

fn sent_id(receipt: SendReceipt) -> SentMessageId {
    SentMessageId(
        receipt
            .message_id
            .unwrap_or_else(|| format!("wecom-agent-{}", uuid::Uuid::new_v4())),
    )
}
