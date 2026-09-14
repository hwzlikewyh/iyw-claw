use serde_json::Value;

use crate::chat_channel::attachments::{AttachmentSource, IncomingAttachment};

pub(super) fn content(body: &Value) -> Option<(String, Vec<IncomingAttachment>)> {
    let mut text = Vec::new();
    let mut attachments = Vec::new();
    if body["msgtype"] == "mixed" {
        for item in body.pointer("/mixed/msg_item")?.as_array()? {
            read_item(item, &mut text, &mut attachments);
        }
    } else {
        read_item(body, &mut text, &mut attachments);
    }
    let text = text.join("\n");
    if text.trim().is_empty() && attachments.is_empty() {
        return None;
    }
    Some((
        if text.trim().is_empty() {
            "请查看附件。".into()
        } else {
            text
        },
        attachments,
    ))
}

fn read_item(body: &Value, text: &mut Vec<String>, files: &mut Vec<IncomingAttachment>) {
    let kind = body["msgtype"].as_str().unwrap_or("text");
    if matches!(kind, "text" | "voice") {
        if let Some(value) = body
            .get(kind)
            .and_then(|part| part["content"].as_str())
            .or_else(|| body["content"].as_str())
        {
            text.push(value.to_string());
        }
        return;
    }
    if !matches!(kind, "image" | "file" | "video") {
        return;
    }
    let Some(resource) = body.get(kind) else {
        return;
    };
    let (Some(url), Some(key)) = (resource["url"].as_str(), resource["aeskey"].as_str()) else {
        return;
    };
    let default = match kind {
        "image" => "image.jpg",
        "video" => "video.mp4",
        _ => "file",
    };
    files.push(IncomingAttachment {
        name: resource["filename"].as_str().unwrap_or(default).into(),
        mime_type: if kind == "video" {
            "video/mp4"
        } else {
            "application/octet-stream"
        }
        .into(),
        source: AttachmentSource::WecomAiBot {
            url: url.into(),
            aes_key: key.into(),
        },
    });
}
