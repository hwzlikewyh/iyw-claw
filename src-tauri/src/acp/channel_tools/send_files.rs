use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;

use super::service::ChannelToolService;
use super::types::SendMessagesInput;
use crate::chat_channel::attachments::{AttachmentCapability, ChannelAttachment};
use crate::chat_channel::types::ChannelMessageTarget;
use crate::db::service::chat_channel_message_log_service;

#[derive(Serialize)]
pub(super) struct FileSendResult {
    pub(super) name: String,
    pub(super) bytes: Option<u64>,
    pub(super) mime_type: String,
    pub(super) status: &'static str,
    pub(super) message_id: Option<String>,
    pub(super) error: Option<&'static str>,
    pub(super) log_error: Option<&'static str>,
}

pub(super) struct InspectedFile {
    result: FileSendResult,
    path: Option<PathBuf>,
    max_file_bytes: Option<u64>,
}

impl InspectedFile {
    fn failed(path: &Path, bytes: Option<u64>, mime_type: String, error: &'static str) -> Self {
        Self {
            result: FileSendResult {
                name: safe_name(path),
                bytes,
                mime_type,
                status: "failed",
                message_id: None,
                error: Some(error),
                log_error: None,
            },
            path: None,
            max_file_bytes: None,
        }
    }
}

pub(super) async fn inspect_files(
    paths: &[String],
    working_dir: &Path,
    capability: AttachmentCapability,
) -> Vec<InspectedFile> {
    let mut files = Vec::with_capacity(paths.len());
    for value in paths {
        files.push(inspect_file(value, working_dir, capability).await);
    }
    files
}

async fn inspect_file(
    value: &str,
    working_dir: &Path,
    capability: AttachmentCapability,
) -> InspectedFile {
    let path = resolved_path(value, working_dir);
    let mime_type = mime_type(&path);
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return InspectedFile::failed(&path, None, mime_type, "FILE_NOT_READABLE"),
        Err(error) => {
            let code = if error.kind() == std::io::ErrorKind::NotFound {
                "FILE_NOT_FOUND"
            } else {
                "FILE_NOT_READABLE"
            };
            return InspectedFile::failed(&path, None, mime_type, code);
        }
    };
    let bytes = metadata.len();
    if !capability.supported {
        return InspectedFile::failed(&path, Some(bytes), mime_type, "ATTACHMENT_UNSUPPORTED");
    }
    if capability.max_file_bytes.is_some_and(|limit| bytes > limit) {
        return InspectedFile::failed(&path, Some(bytes), mime_type, "FILE_TOO_LARGE");
    }
    let name = safe_name(&path);
    InspectedFile {
        result: FileSendResult {
            name: name.clone(),
            bytes: Some(bytes),
            mime_type: mime_type.clone(),
            status: "ready",
            message_id: None,
            error: None,
            log_error: None,
        },
        path: Some(path),
        max_file_bytes: capability.max_file_bytes,
    }
}

impl ChannelToolService {
    pub(super) async fn send_files_to_target(
        &self,
        channel_id: i32,
        target_id: &str,
        target: &ChannelMessageTarget,
        files: Vec<InspectedFile>,
    ) -> Vec<FileSendResult> {
        let mut results = Vec::with_capacity(files.len());
        for mut file in files {
            let Some(path) = file.path.take() else {
                results.push(file.result);
                continue;
            };
            let content = match read_checked(&path, file.max_file_bytes).await {
                Ok(content) => content,
                Err(code) => {
                    file.result.status = "failed";
                    file.result.error = Some(code);
                    results.push(file.result);
                    continue;
                }
            };
            file.result.bytes = Some(content.len() as u64);
            let attachment = ChannelAttachment {
                name: file.result.name.clone(),
                mime_type: file.result.mime_type.clone(),
                bytes: Arc::from(content),
            };
            match self
                .manager
                .send_attachment_to_target(target, &attachment)
                .await
            {
                Ok(_) => {
                    file.result.status = "sent";
                    match log_attachment(&self.db.conn, channel_id, target_id, &attachment).await {
                        Ok(id) => file.result.message_id = Some(format!("cm_{id}")),
                        Err(_) => file.result.log_error = Some("MESSAGE_LOG_FAILED"),
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        channel_id,
                        target_id,
                        file_name = %file.result.name,
                        mime_type = %file.result.mime_type,
                        file_bytes = ?file.result.bytes,
                        error_category = error.category(),
                        error = %error,
                        "[ChatChannel] attachment delivery failed"
                    );
                    file.result.status = "failed";
                    file.result.error = Some("ATTACHMENT_SEND_FAILED");
                }
            }
            results.push(file.result);
        }
        results
    }
}

async fn read_checked(path: &Path, limit: Option<u64>) -> Result<Vec<u8>, &'static str> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| "FILE_NOT_READABLE")?;
    let metadata = file.metadata().await.map_err(|_| "FILE_NOT_READABLE")?;
    if !metadata.is_file() {
        return Err("FILE_NOT_READABLE");
    }
    if limit.is_some_and(|value| metadata.len() > value) {
        return Err("FILE_TOO_LARGE");
    }
    let mut content = Vec::with_capacity(read_capacity(metadata.len(), limit));
    match limit {
        Some(value) => file
            .take(value.saturating_add(1))
            .read_to_end(&mut content)
            .await
            .map_err(|_| "FILE_NOT_READABLE")?,
        None => file
            .read_to_end(&mut content)
            .await
            .map_err(|_| "FILE_NOT_READABLE")?,
    };
    if limit.is_some_and(|value| content.len() as u64 > value) {
        return Err("FILE_TOO_LARGE");
    }
    Ok(content)
}

fn read_capacity(file_bytes: u64, limit: Option<u64>) -> usize {
    let bounded = limit.map_or(file_bytes, |value| file_bytes.min(value));
    usize::try_from(bounded)
        .unwrap_or(usize::MAX)
        .min(1024 * 1024)
}

async fn log_attachment(
    db: &sea_orm::DatabaseConnection,
    channel_id: i32,
    target_id: &str,
    attachment: &ChannelAttachment,
) -> Result<i32, crate::db::error::DbError> {
    chat_channel_message_log_service::create_log_for_target_returning(
        db,
        channel_id,
        "outbound",
        "attachment",
        &format!("{} ({} bytes)", attachment.name, attachment.byte_len()),
        "sent",
        None,
        None,
        None,
        Some(target_id.to_string()),
    )
    .await
    .map(|log| log.id)
}

pub(super) fn safe_send_digest(input: &SendMessagesInput) -> Value {
    serde_json::to_value(input).unwrap_or_else(|_| json!({ "invalid": true }))
}

fn resolved_path(value: &str, working_dir: &Path) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        working_dir.join(path)
    }
}

fn safe_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string()
}

fn mime_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "pdf" => "application/pdf",
        "txt" | "md" | "log" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}
