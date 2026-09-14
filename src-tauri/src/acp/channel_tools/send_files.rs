use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::send_file_io::{mime_type, read_checked, resolved_path, safe_name};
use serde::Serialize;
use serde_json::{json, Value};

use super::service::ChannelToolService;
use super::types::SendMessagesInput;
use crate::chat_channel::attachments::{AttachmentCapability, ChannelAttachment};
use crate::chat_channel::types::ChannelMessageTarget;
use crate::db::service::chat_channel_message_log_service;

static MEDIA_SENDS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
const MEDIA_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Serialize)]
pub(super) struct FileSendResult {
    pub(super) name: String,
    pub(super) bytes: Option<u64>,
    pub(super) mime_type: String,
    pub(super) status: &'static str,
    pub(super) message_id: Option<String>,
    pub(super) delivery_receipt: Option<String>,
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
                delivery_receipt: None,
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
            delivery_receipt: None,
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
            if let Err(code) = self.deliver_file(target, &mut file).await {
                file.result.status = "failed";
                file.result.error = Some(code);
            }
            match log_attachment(&self.db.conn, channel_id, target_id, &file.result).await {
                Ok(id) => file.result.message_id = Some(format!("cm_{id}")),
                Err(_) => file.result.log_error = Some("MESSAGE_LOG_FAILED"),
            }
            results.push(file.result);
        }
        results
    }

    async fn deliver_file(
        &self,
        target: &ChannelMessageTarget,
        file: &mut InspectedFile,
    ) -> Result<(), &'static str> {
        if file.path.is_none() {
            return Ok(());
        }
        let _permit = MEDIA_SENDS
            .acquire()
            .await
            .map_err(|_| "ATTACHMENT_SEND_UNAVAILABLE")?;
        let attachment = prepare_attachment(file).await?;
        let result = tokio::time::timeout(
            MEDIA_SEND_TIMEOUT,
            self.manager.send_attachment_to_target(target, &attachment),
        )
        .await
        .unwrap_or_else(|_| {
            Err(crate::chat_channel::error::ChatChannelError::SendFailed(
                "Attachment delivery timed out".into(),
            ))
        });
        let receipt = result.map_err(|error| {
            tracing::warn!(channel_id = target.channel_id, mime_type = %file.result.mime_type,
                bytes = ?file.result.bytes, error_category = error.category(), error = %error,
                "[ChatChannel] attachment delivery failed");
            attachment_error(&error)
        })?;
        file.result.status = "sent";
        file.result.delivery_receipt = (!receipt.0.is_empty()).then_some(receipt.0);
        Ok(())
    }
}

async fn prepare_attachment(file: &mut InspectedFile) -> Result<ChannelAttachment, &'static str> {
    let path = file.path.take().ok_or("FILE_NOT_READABLE")?;
    let content = read_checked(&path, file.max_file_bytes).await?;
    file.result.bytes = Some(content.len() as u64);
    if let Ok(format) = image::guess_format(&content) {
        file.result.mime_type = format.to_mime_type().to_string();
    } else if file.result.mime_type.starts_with("image/") {
        file.result.mime_type = "application/octet-stream".into();
    }
    Ok(ChannelAttachment {
        name: file.result.name.clone(),
        mime_type: file.result.mime_type.clone(),
        bytes: Arc::from(content),
    })
}

fn attachment_error(error: &crate::chat_channel::error::ChatChannelError) -> &'static str {
    use crate::chat_channel::error::ChatChannelError;
    let message = error.to_string();
    match error {
        ChatChannelError::Unsupported(_) => "ATTACHMENT_UNSUPPORTED",
        ChatChannelError::NotConnected => "CHANNEL_NOT_CONNECTED",
        _ if message.contains("TARGET_CONTEXT_EXPIRED") => "TARGET_CONTEXT_EXPIRED",
        _ if message.contains("TARGET_NOT_SENDABLE") => "TARGET_NOT_SENDABLE",
        _ if message.contains("timed out") => "ATTACHMENT_DELIVERY_UNKNOWN",
        _ => "ATTACHMENT_SEND_FAILED",
    }
}

async fn log_attachment(
    db: &sea_orm::DatabaseConnection,
    channel_id: i32,
    target_id: &str,
    attachment: &FileSendResult,
) -> Result<i32, crate::db::error::DbError> {
    chat_channel_message_log_service::create_log_for_target_returning(
        db,
        channel_id,
        "outbound",
        "attachment",
        &format!(
            "{} ({} bytes)",
            attachment.name,
            attachment.bytes.unwrap_or(0)
        ),
        attachment.status,
        attachment.error.map(str::to_string),
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
