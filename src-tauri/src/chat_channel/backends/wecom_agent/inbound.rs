use super::{crypto::WecomInboundMessage, WecomAgentBackend};
use crate::chat_channel::attachments::{AttachmentSource, ChannelAttachment, IncomingAttachment};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http::{self, failure, transport};

pub(crate) fn inbound_attachments(message: &WecomInboundMessage) -> Vec<IncomingAttachment> {
    if message.media_id.is_empty() {
        return Vec::new();
    }
    let name = match message.msg_type.as_str() {
        "image" => "image.jpg",
        "voice" => "voice.amr",
        "video" => "video.mp4",
        _ => "file",
    };
    vec![IncomingAttachment {
        name: if message.file_name.is_empty() {
            name.into()
        } else {
            message.file_name.clone()
        },
        mime_type: "application/octet-stream".into(),
        source: AttachmentSource::WecomAgent {
            media_id: message.media_id.clone(),
        },
    }]
}

impl WecomAgentBackend {
    pub(super) async fn download_media(
        &self,
        source: &IncomingAttachment,
    ) -> Result<ChannelAttachment, ChatChannelError> {
        let AttachmentSource::WecomAgent { media_id } = &source.source else {
            return Err(failure("Invalid WeCom application resource"));
        };
        let token = self.access_token(false, None).await?;
        let response = self
            .client
            .client
            .get("https://qyapi.weixin.qq.com/cgi-bin/media/get")
            .query(&[("access_token", &token), ("media_id", media_id)])
            .send()
            .await
            .map_err(transport)?;
        let name = media_http::response_filename(&response);
        let bytes = media_http::binary(response).await?;
        let mut file = media_http::attachment(source, bytes);
        if let Some(name) = name {
            file.name = name;
        }
        Ok(file)
    }
}
