use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

use super::{protocol, WecomAiBotBackend};
use crate::chat_channel::attachments::{
    AttachmentSource, ChannelAttachment, IncomingAttachment, MIB,
};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http::{self, failure, required};
use crate::chat_channel::types::{ChannelMessageTarget, SentMessageId};

const CHUNK_BYTES: usize = 512 * 1024;
const MAX_FILE_BYTES: u64 = 20 * MIB;

impl WecomAiBotBackend {
    pub(super) async fn send_media(
        &self,
        attachment: &ChannelAttachment,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        let chat_id = target
            .chat_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| failure("WeCom media target is missing"))?;
        let kind = media_kind(attachment)?;
        let media_id = self.upload_media(attachment, kind).await?;
        let mut body = json!({ "chatid": chat_id, "chat_type": protocol::target_chat_type(target).unwrap_or(self.config.default_chat_type), "msgtype": kind });
        body[kind] = json!({ "media_id": media_id });
        self.send_frame(frame("aibot_send_msg", body)).await
    }

    async fn upload_media(
        &self,
        attachment: &ChannelAttachment,
        kind: &str,
    ) -> Result<String, ChatChannelError> {
        let chunks = attachment.bytes.chunks(CHUNK_BYTES);
        let init = self
            .request_frame(frame(
                "aibot_upload_media_init",
                json!({
                    "type": kind, "filename": attachment.name,
                    "total_size": attachment.byte_len(), "total_chunks": chunks.len(),
                }),
            ))
            .await?;
        let upload_id = required(&init, "upload_id")?;
        for (index, chunk) in chunks.enumerate() {
            self.request_frame(frame(
                "aibot_upload_media_chunk",
                json!({
                    "upload_id": upload_id, "chunk_index": index,
                    "base64_data": STANDARD.encode(chunk),
                }),
            ))
            .await?;
        }
        let result = self
            .request_frame(frame(
                "aibot_upload_media_finish",
                json!({ "upload_id": upload_id }),
            ))
            .await?;
        Ok(required(&result, "media_id")?.to_string())
    }
}

fn frame(command: &str, body: Value) -> Value {
    json!({ "cmd": command, "headers": { "req_id": uuid::Uuid::new_v4().to_string() }, "body": body })
}

fn media_kind(attachment: &ChannelAttachment) -> Result<&'static str, ChatChannelError> {
    let image = crate::chat_channel::media_capabilities::native_image(
        "wecom_ai_bot",
        &attachment.mime_type,
        attachment.byte_len(),
    );
    attachment.validate(MAX_FILE_BYTES)?;
    if attachment.byte_len() < 5 || attachment.name.len() > 256 {
        return Err(failure(
            "WeCom media requires at least 5 bytes and a filename within 256 bytes",
        ));
    }
    Ok(if image { "image" } else { "file" })
}

pub(super) async fn download(
    source: &IncomingAttachment,
) -> Result<ChannelAttachment, ChatChannelError> {
    let AttachmentSource::WecomAiBot { url, aes_key } = &source.source else {
        return Err(failure("Invalid WeCom resource source"));
    };
    let (mut bytes, name) = media_http::download_named(url).await?;
    let key = STANDARD
        .decode(aes_key.trim_end_matches('=').to_string() + "=")
        .map_err(|_| failure("Invalid WeCom media key"))?;
    if key.len() != 32 || bytes.is_empty() || bytes.len() % 16 != 0 {
        return Err(failure("Invalid WeCom encrypted media"));
    }
    let plaintext = cbc::Decryptor::<aes::Aes256>::new_from_slices(&key, &key[..16])
        .map_err(|_| failure("Invalid WeCom cipher key"))?
        .decrypt_padded_mut::<NoPadding>(&mut bytes)
        .map_err(|_| failure("WeCom media decryption failed"))?;
    let padding = *plaintext
        .last()
        .ok_or_else(|| failure("Empty WeCom media"))? as usize;
    if padding == 0
        || padding > 32
        || plaintext.len() < padding
        || !plaintext[plaintext.len() - padding..]
            .iter()
            .all(|b| *b as usize == padding)
    {
        return Err(failure("Invalid WeCom media padding"));
    }
    let length = plaintext.len() - padding;
    bytes.truncate(length);
    let mut attachment = media_http::attachment(source, bytes);
    if let Some(name) = name {
        attachment.name = name;
    }
    Ok(attachment)
}
