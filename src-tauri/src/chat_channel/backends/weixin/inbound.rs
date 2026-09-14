use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

use super::media_crypto;
use crate::chat_channel::attachments::{AttachmentSource, ChannelAttachment, IncomingAttachment};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http::{self, failure};

pub(super) const CDN_BASE: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

pub(super) fn content(message: &Value) -> (String, Vec<IncomingAttachment>) {
    let mut text = Vec::new();
    let mut files = Vec::new();
    for item in message["item_list"].as_array().into_iter().flatten() {
        match item["type"].as_i64() {
            Some(1) => {
                if let Some(value) = item.pointer("/text_item/text").and_then(Value::as_str) {
                    text.push(value.to_string());
                }
            }
            Some(3)
                if item
                    .pointer("/voice_item/text")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty()) =>
            {
                text.push(
                    item["voice_item"]["text"]
                        .as_str()
                        .unwrap_or_default()
                        .into(),
                );
            }
            Some(2..=5) => {
                if let Some(file) = attachment(item) {
                    files.push(file);
                }
            }
            _ => {}
        }
    }
    let text = text.join("\n");
    (
        if text.trim().is_empty() && !files.is_empty() {
            "请查看附件。".into()
        } else {
            text
        },
        files,
    )
}

fn attachment(item: &Value) -> Option<IncomingAttachment> {
    let (key, name, mime) = match item["type"].as_i64()? {
        2 => ("image_item", "image.jpg", "application/octet-stream"),
        3 => ("voice_item", "voice.silk", "audio/silk"),
        4 => ("file_item", "file", "application/octet-stream"),
        5 => ("video_item", "video.mp4", "video/mp4"),
        _ => return None,
    };
    let resource = &item[key];
    let media = &resource["media"];
    let url = media["full_url"]
        .as_str()
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .or_else(|| {
            media["encrypt_query_param"].as_str().map(|value| {
                format!(
                    "{CDN_BASE}/download?encrypted_query_param={}",
                    urlencoding::encode(value)
                )
            })
        })?;
    let aes_key = resource["aeskey"]
        .as_str()
        .map(|key| STANDARD.encode(key))
        .or_else(|| media["aes_key"].as_str().map(str::to_string))
        .or_else(|| (key != "image_item").then(String::new));
    Some(IncomingAttachment {
        name: resource["file_name"].as_str().unwrap_or(name).into(),
        mime_type: mime.into(),
        source: AttachmentSource::Weixin { url, aes_key },
    })
}

pub(super) async fn download(
    source: &IncomingAttachment,
) -> Result<ChannelAttachment, ChatChannelError> {
    let AttachmentSource::Weixin { url, aes_key } = &source.source else {
        return Err(failure("Invalid Weixin media source"));
    };
    let bytes = media_http::download(url).await?;
    let bytes = match aes_key {
        Some(key) => media_crypto::decrypt(bytes, key)?,
        None => bytes,
    };
    Ok(media_http::attachment(source, bytes))
}
