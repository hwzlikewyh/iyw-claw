use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};
use tokio::io::AsyncReadExt;

use super::{run_cli_json, State, WecomBackend, SENT_ECHO_TTL};
use crate::chat_channel::attachments::{
    AttachmentSource, ChannelAttachment, IncomingAttachment, MAX_INBOUND_BYTES, MIB,
};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http::{failure, required};
use crate::chat_channel::types::{ChannelMessageTarget, SentMessageId};

const MAX_MEDIA_ECHO_RECORDS: usize = 64;

impl WecomBackend {
    pub(super) async fn send_media(
        &self,
        file: &ChannelAttachment,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        file.validate(20 * MIB)?;
        let chat_id = self.media_target(target).await?;
        let temp = tempfile::Builder::new()
            .prefix("iyw-channel-")
            .tempdir()
            .map_err(|error| failure(format!("Cannot stage WeCom media: {error}")))?;
        let path = temp
            .path()
            .join(crate::commands::chat_attachments::sanitize_file_name(
                &file.name,
            ));
        tokio::fs::write(&path, &file.bytes)
            .await
            .map_err(|error| failure(error.to_string()))?;
        let uploaded = run_cli_json(
            &self.state.data_dir,
            &[
                "media",
                "upload",
                "--json",
                &json!({ "file_path": path }).to_string(),
            ],
        )
        .await?;
        let body = payload(&uploaded);
        self.send_uploaded_media(&chat_id, body).await
    }

    async fn send_uploaded_media(
        &self,
        chat_id: &str,
        body: &Value,
    ) -> Result<SentMessageId, ChatChannelError> {
        let media_id = required(body, "media_id")?;
        // 使用上传回执的真实类型，避免将 file 素材错误标记为 image。
        let kind = body["type"]
            .as_str()
            .filter(|kind| matches!(*kind, "image" | "file" | "voice" | "video"))
            .unwrap_or("file");
        let mut request = json!({ "chat_id": chat_id, "msg_type": kind });
        request[kind] = json!({ "media_id": media_id });
        {
            let mut sent = self.state.recently_sent_media.lock().await;
            sent.push_back((media_id.to_string(), Instant::now()));
            while sent.len() > MAX_MEDIA_ECHO_RECORDS {
                sent.pop_front();
            }
        }
        run_cli_json(
            &self.state.data_dir,
            &["message", "aibot", "send", "--json", &request.to_string()],
        )
        .await?;
        Ok(SentMessageId(String::new()))
    }

    async fn media_target(
        &self,
        target: &ChannelMessageTarget,
    ) -> Result<String, ChatChannelError> {
        let expected = target
            .chat_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| failure("WeCom media target is missing"))?;
        let result = run_cli_json(
            &self.state.data_dir,
            &["message", "aibot", "sessions", "list"],
        )
        .await?;
        payload(&result)["sessions"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|session| session["chat_id"].as_str().filter(|id| *id == expected))
            .map(str::to_string)
            .ok_or_else(|| {
                failure("TARGET_NOT_SENDABLE: WeCom target is not a current robot session")
            })
    }

    pub(super) async fn download_media(
        &self,
        source: &IncomingAttachment,
    ) -> Result<ChannelAttachment, ChatChannelError> {
        let AttachmentSource::WecomCli { media_id } = &source.source else {
            return Err(failure("Invalid WeCom CLI media source"));
        };
        let result = run_cli_json(
            &self.state.data_dir,
            &[
                "media",
                "download",
                "--json",
                &json!({ "media_id": media_id }).to_string(),
            ],
        )
        .await?;
        let path = Path::new(required(payload(&result), "file_path")?);
        read_downloaded(path, source).await
    }
}

async fn read_downloaded(
    path: &Path,
    source: &IncomingAttachment,
) -> Result<ChannelAttachment, ChatChannelError> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| failure(error.to_string()))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|error| failure(error.to_string()))?;
    if !metadata.is_file() || metadata.len() > MAX_INBOUND_BYTES as u64 {
        return Err(failure("WeCom media exceeds the size limit"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_INBOUND_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| failure(error.to_string()))?;
    if bytes.is_empty() || bytes.len() > MAX_INBOUND_BYTES {
        return Err(failure("Invalid WeCom media size"));
    }
    Ok(ChannelAttachment {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&source.name)
            .into(),
        mime_type: image::guess_format(&bytes)
            .ok()
            .map(|format| format.to_mime_type().to_string())
            .unwrap_or_else(|| source.mime_type.clone()),
        bytes: Arc::from(bytes),
    })
}

fn payload(value: &Value) -> &Value {
    value
        .get("data")
        .filter(|data| data.is_object())
        .unwrap_or(value)
}

pub(super) fn inbound(message: &Value) -> Vec<IncomingAttachment> {
    let kind = message["msg_type"].as_str().unwrap_or_default();
    if !matches!(kind, "image" | "file" | "voice" | "video") {
        return Vec::new();
    }
    let resource = &message[kind];
    let Some(media_id) = resource["media_id"].as_str().filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    vec![IncomingAttachment {
        name: resource["file_name"].as_str().unwrap_or(kind).into(),
        mime_type: "application/octet-stream".into(),
        source: AttachmentSource::WecomCli {
            media_id: media_id.into(),
        },
    }]
}

pub(super) async fn is_echo(state: &State, files: &[IncomingAttachment]) -> bool {
    if files.is_empty() {
        return false;
    }
    let mut sent = state.recently_sent_media.lock().await;
    sent.retain(|(_, created)| created.elapsed() < SENT_ECHO_TTL);
    files.iter().any(|file| match &file.source {
        AttachmentSource::WecomCli { media_id } => sent.iter().any(|(id, _)| id == media_id),
        _ => false,
    })
}
