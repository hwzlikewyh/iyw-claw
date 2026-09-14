use crate::chat_channel::attachments::{AttachmentSource, IncomingAttachment};
use serde_json::Value;

pub(super) fn content(data: &Value) -> (String, Vec<IncomingAttachment>) {
    let mut text = data
        .pointer("/text/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut files = Vec::new();
    let content = &data["content"];
    match data["msgtype"].as_str().unwrap_or("text") {
        "picture" | "file" | "video" | "audio" => add_file(content, &mut files),
        "richText" => {
            for part in content["richText"].as_array().into_iter().flatten() {
                if let Some(value) = part["text"].as_str() {
                    text.push_str(value);
                }
                add_file(part, &mut files);
            }
        }
        _ => {}
    }
    if text.trim().is_empty() && !files.is_empty() {
        text = "请查看附件。".into();
    }
    (text, files)
}

fn add_file(value: &Value, files: &mut Vec<IncomingAttachment>) {
    let Some(code) = value
        .get("downloadCode")
        .or_else(|| value.get("pictureDownloadCode"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    files.push(IncomingAttachment {
        name: value["fileName"].as_str().unwrap_or("attachment").into(),
        mime_type: "application/octet-stream".into(),
        source: AttachmentSource::Dingtalk {
            download_code: code.into(),
        },
    });
}
