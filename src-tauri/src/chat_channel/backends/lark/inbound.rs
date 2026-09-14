use serde_json::Value;

use super::LarkBackend;
use crate::chat_channel::attachments::{AttachmentSource, ChannelAttachment, IncomingAttachment};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http::{self, failure, transport};

pub(super) fn content(event: &Value) -> (String, Vec<IncomingAttachment>) {
    let message = &event["event"]["message"];
    let content: Value =
        serde_json::from_str(message["content"].as_str().unwrap_or("{}")).unwrap_or(Value::Null);
    let mut files = Vec::new();
    let mut texts = Vec::new();
    match message["message_type"].as_str().unwrap_or("") {
        "text" => texts.push(content["text"].as_str().unwrap_or_default().to_string()),
        "image" | "file" | "audio" | "media" => add_file(&content, message, &mut files),
        "post" => (texts, files) = read_post(&content, message),
        _ => {}
    }
    let text = texts.join("\n");
    (
        if text.trim().is_empty() && !files.is_empty() {
            "请查看附件。".into()
        } else {
            text
        },
        files,
    )
}

fn read_post(content: &Value, message: &Value) -> (Vec<String>, Vec<IncomingAttachment>) {
    let mut text = Vec::new();
    let mut files = Vec::new();
    let post = if content.get("content").is_some() {
        content
    } else {
        content
            .as_object()
            .and_then(|o| o.values().find(|v| v.get("content").is_some()))
            .unwrap_or(content)
    };
    if let Some(title) = post["title"].as_str() {
        text.push(title.into());
    }
    let Some(rows) = post["content"].as_array() else {
        return (text, files);
    };
    for item in rows.iter().filter_map(Value::as_array).flatten() {
        if let Some(value) = item["text"].as_str() {
            text.push(value.into());
        }
        if matches!(item["tag"].as_str(), Some("img" | "media")) {
            add_file(item, message, &mut files);
        }
    }
    (text, files)
}

fn add_file(content: &Value, message: &Value, files: &mut Vec<IncomingAttachment>) {
    let image = content.get("image_key").is_some() && content.get("file_key").is_none();
    let key_name = if image { "image_key" } else { "file_key" };
    let (Some(key), Some(message_id)) =
        (content[key_name].as_str(), message["message_id"].as_str())
    else {
        return;
    };
    files.push(IncomingAttachment {
        name: content["file_name"]
            .as_str()
            .unwrap_or(if image { "image.jpg" } else { "file" })
            .into(),
        mime_type: "application/octet-stream".into(),
        source: AttachmentSource::Lark {
            message_id: message_id.into(),
            key: key.into(),
            kind: if image { "image" } else { "file" }.into(),
        },
    });
}

impl LarkBackend {
    pub(super) async fn download_media(
        &self,
        source: &IncomingAttachment,
    ) -> Result<ChannelAttachment, ChatChannelError> {
        let AttachmentSource::Lark {
            message_id,
            key,
            kind,
        } = &source.source
        else {
            return Err(failure("Invalid Lark resource source"));
        };
        let token = self.get_tenant_access_token().await?;
        let response = self
            .client
            .get(format!(
                "{}/open-apis/im/v1/messages/{}/resources/{}",
                self.api_base_url,
                urlencoding::encode(message_id),
                urlencoding::encode(key)
            ))
            .bearer_auth(token)
            .query(&[("type", kind)])
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
