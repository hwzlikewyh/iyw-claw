use super::LarkBackend;
use crate::chat_channel::attachments::ChannelAttachment;
use crate::chat_channel::error::ChatChannelError;

pub(super) fn should_send_as_image(attachment: &ChannelAttachment) -> bool {
    crate::chat_channel::media_capabilities::native_image(
        "lark",
        &attachment.mime_type,
        attachment.byte_len(),
    )
}

pub(super) async fn upload_image(
    backend: &LarkBackend,
    image: &ChannelAttachment,
) -> Result<String, ChatChannelError> {
    backend.upload_attachment(image, true).await
}
