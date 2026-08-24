use serde::Deserialize;

use super::LarkBackend;
use crate::chat_channel::attachments::ChannelAttachment;
use crate::chat_channel::error::ChatChannelError;

const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

pub(super) fn should_send_as_image(attachment: &ChannelAttachment) -> bool {
    attachment.byte_len() <= MAX_IMAGE_BYTES
        && matches!(
            attachment.mime_type.as_str(),
            "image/jpeg"
                | "image/png"
                | "image/webp"
                | "image/gif"
                | "image/bmp"
                | "image/x-icon"
                | "image/tiff"
                | "image/heic"
        )
}

pub(super) async fn upload_image(
    backend: &LarkBackend,
    image: &ChannelAttachment,
) -> Result<String, ChatChannelError> {
    let token = backend.get_tenant_access_token().await?;
    let part = reqwest::multipart::Part::bytes(image.bytes.to_vec())
        .file_name(image.name.clone())
        .mime_str(&image.mime_type)
        .map_err(|_| ChatChannelError::SendFailed("invalid image MIME type".into()))?;
    let result = backend
        .client
        .post(format!("{}/open-apis/im/v1/images", backend.api_base_url))
        .header("Authorization", format!("Bearer {token}"))
        .multipart(
            reqwest::multipart::Form::new()
                .text("image_type", "message")
                .part("image", part),
        )
        .send()
        .await
        .map_err(|error| ChatChannelError::SendFailed(error.to_string()))?
        .json::<UploadImageResponse>()
        .await
        .map_err(|error| ChatChannelError::SendFailed(error.to_string()))?;
    if result.code != 0 {
        return Err(ChatChannelError::SendFailed(format!(
            "provider code {}",
            result.code
        )));
    }
    result
        .data
        .and_then(|data| data.image_key)
        .ok_or_else(|| ChatChannelError::SendFailed("provider image key missing".into()))
}

#[derive(Deserialize)]
struct UploadImageResponse {
    code: i32,
    data: Option<UploadImageData>,
}

#[derive(Deserialize)]
struct UploadImageData {
    image_key: Option<String>,
}
