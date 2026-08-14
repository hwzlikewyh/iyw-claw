use std::path::{Component, Path, PathBuf};

use crate::acp::error::AcpError;
use crate::acp::types::PromptInputBlock;
use crate::commands::chat_attachments::{sanitize_session_bucket, user_facing_path};
use crate::commands::chat_image::{
    stage_chat_image_bytes_core, StageChatImageBytes, CHAT_IMAGE_DERIVED_MAX_BYTES,
};

const CHAT_SESSIONS_DIR: &str = "chat-sessions";
const CONVERSATION_ATTACHMENTS_DIR: &str = "conversation-attachments";
const ATTACHMENTS_DIR: &str = "attachments";

#[derive(Debug, Clone)]
pub(crate) struct CodexImageScope {
    pub connection_id: String,
    pub conversation_id: Option<i32>,
    pub working_dir: Option<PathBuf>,
}

struct PreparedScope {
    data_dir: PathBuf,
    destination_bucket: String,
    allowed_buckets: Vec<String>,
    chat_dir: Option<PathBuf>,
}

enum ManagedLocation {
    Chat(PathBuf),
    Conversation(String),
}

struct InspectedImage {
    path: PathBuf,
    bytes: Vec<u8>,
    mime_type: &'static str,
    location: ManagedLocation,
}

pub(crate) async fn prepare_codex_image_inputs(
    data_dir: &Path,
    scope: CodexImageScope,
    mut blocks: Vec<PromptInputBlock>,
) -> Result<Vec<PromptInputBlock>, AcpError> {
    if !has_images(&blocks) {
        return Ok(blocks);
    }
    let scope = prepare_scope(data_dir, scope).await?;
    for block in &mut blocks {
        let PromptInputBlock::Image {
            mime_type,
            local_path,
            ..
        } = block
        else {
            continue;
        };
        let inspected = inspect_image(&scope, mime_type, local_path.as_deref()).await?;
        let path = if belongs_to_scope(&scope, &inspected.location) {
            inspected.path
        } else {
            restage_for_scope(&scope, inspected).await?
        };
        *local_path = Some(user_facing_path(&path));
    }
    Ok(blocks)
}

pub(crate) async fn validate_codex_image_inputs(
    data_dir: &Path,
    scope: CodexImageScope,
    blocks: &[PromptInputBlock],
) -> Result<(), AcpError> {
    if !has_images(blocks) {
        return Ok(());
    }
    let scope = prepare_scope(data_dir, scope).await?;
    for block in blocks {
        let PromptInputBlock::Image {
            mime_type,
            local_path,
            ..
        } = block
        else {
            continue;
        };
        let inspected = inspect_image(&scope, mime_type, local_path.as_deref()).await?;
        if !belongs_to_scope(&scope, &inspected.location) {
            return Err(image_error(
                "Codex image attachment is not bound to the current session. Retry the upload.",
            ));
        }
    }
    Ok(())
}

fn has_images(blocks: &[PromptInputBlock]) -> bool {
    blocks
        .iter()
        .any(|block| matches!(block, PromptInputBlock::Image { .. }))
}

async fn prepare_scope(data_dir: &Path, scope: CodexImageScope) -> Result<PreparedScope, AcpError> {
    let data_dir = tokio::fs::canonicalize(data_dir)
        .await
        .map_err(|error| io_error("data directory", error))?;
    let chat_dir = match scope.working_dir {
        Some(path) if is_managed_chat_dir(&data_dir, &path) => tokio::fs::canonicalize(path)
            .await
            .ok()
            .filter(|path| is_managed_chat_dir(&data_dir, path)),
        _ => None,
    };
    let connection_bucket =
        sanitize_session_bucket(Some(&format!("connection-{}", scope.connection_id)));
    let destination_bucket = scope
        .conversation_id
        .map(|id| format!("conversation-{id}"))
        .map(|value| sanitize_session_bucket(Some(&value)))
        .unwrap_or_else(|| connection_bucket.clone());
    let mut allowed_buckets = vec![destination_bucket.clone()];
    if connection_bucket != destination_bucket {
        allowed_buckets.push(connection_bucket);
    }
    Ok(PreparedScope {
        data_dir,
        destination_bucket,
        allowed_buckets,
        chat_dir,
    })
}

async fn inspect_image(
    scope: &PreparedScope,
    declared_mime: &str,
    local_path: Option<&str>,
) -> Result<InspectedImage, AcpError> {
    let raw = local_path
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            image_error(
                "Codex image attachment is missing its prepared local file. Retry the upload.",
            )
        })?;
    let source = Path::new(raw);
    if !source.is_absolute() {
        return Err(image_error("Codex image attachment local file is invalid."));
    }
    reject_symlink(source).await?;
    let path = tokio::fs::canonicalize(source)
        .await
        .map_err(|error| io_error("local image", error))?;
    let location = managed_location(&scope.data_dir, &path)?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|error| io_error("local image", error))?;
    if !metadata.is_file() || metadata.len() > CHAT_IMAGE_DERIVED_MAX_BYTES as u64 {
        return Err(image_error("Codex image attachment local file is invalid."));
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| io_error("local image", error))?;
    if bytes.is_empty() || bytes.len() > CHAT_IMAGE_DERIVED_MAX_BYTES {
        return Err(image_error("Codex image attachment local file is invalid."));
    }
    let mime_type = inspect_mime(&bytes)?;
    if mime_type != declared_mime {
        return Err(image_error(
            "Codex image attachment MIME type does not match its file.",
        ));
    }
    Ok(InspectedImage {
        path,
        bytes,
        mime_type,
        location,
    })
}

async fn reject_symlink(path: &Path) -> Result<(), AcpError> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|error| io_error("local image", error))?;
    if metadata.file_type().is_symlink() {
        return Err(image_error("Codex image attachment local file is invalid."));
    }
    Ok(())
}

fn managed_location(data_dir: &Path, path: &Path) -> Result<ManagedLocation, AcpError> {
    let relative = path
        .strip_prefix(data_dir)
        .map_err(|_| image_error("Codex image attachment is outside managed storage."))?;
    let parts = normal_components(relative)?;
    match parts.as_slice() {
        [root, date, chat_id, attachments, attachment_id, _]
            if root == CHAT_SESSIONS_DIR
                && attachments == ATTACHMENTS_DIR
                && valid_date(date)
                && valid_uuid(chat_id)
                && valid_uuid(attachment_id) =>
        {
            Ok(ManagedLocation::Chat(
                data_dir.join(root).join(date).join(chat_id),
            ))
        }
        [root, bucket, attachment_id, _]
            if root == CONVERSATION_ATTACHMENTS_DIR && valid_uuid(attachment_id) =>
        {
            Ok(ManagedLocation::Conversation(bucket.clone()))
        }
        _ => Err(image_error(
            "Codex image attachment is outside managed image storage.",
        )),
    }
}

fn normal_components(path: &Path) -> Result<Vec<String>, AcpError> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value.to_string_lossy().to_string()),
            _ => Err(image_error("Codex image attachment local file is invalid.")),
        })
        .collect()
}

fn valid_date(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

fn valid_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}

fn is_managed_chat_dir(data_dir: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(data_dir.join(CHAT_SESSIONS_DIR)) else {
        return false;
    };
    let Ok(parts) = normal_components(relative) else {
        return false;
    };
    matches!(parts.as_slice(), [date, id] if valid_date(date) && valid_uuid(id))
}

fn inspect_mime(bytes: &[u8]) -> Result<&'static str, AcpError> {
    crate::commands::chat_image::inspect_derived_image_mime(bytes)
        .map_err(|_| image_error("Codex image attachment format is not supported."))
}

fn belongs_to_scope(scope: &PreparedScope, location: &ManagedLocation) -> bool {
    match location {
        ManagedLocation::Chat(chat_dir) => scope.chat_dir.as_ref() == Some(chat_dir),
        ManagedLocation::Conversation(bucket) => scope.allowed_buckets.contains(bucket),
    }
}

async fn restage_for_scope(
    scope: &PreparedScope,
    image: InspectedImage,
) -> Result<PathBuf, AcpError> {
    let file_name = image
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let staged = stage_chat_image_bytes_core(
        &scope.data_dir,
        StageChatImageBytes {
            chat_dir: None,
            session_id: Some(&scope.destination_bucket),
            file_name,
            mime_type: image.mime_type,
            bytes: &image.bytes,
        },
    )
    .await
    .map_err(|error| image_error(error.to_string()))?;
    tracing::info!(
        target: "acp.image",
        attachment_bytes = image.bytes.len(),
        destination_scope = %scope.destination_bucket,
        "re-staged Codex image for current session"
    );
    Ok(PathBuf::from(staged.path))
}

fn io_error(stage: &str, error: std::io::Error) -> AcpError {
    tracing::warn!(target: "acp.image", stage, error = %error, "Codex image validation failed");
    image_error("Codex image attachment local file is unavailable. Retry the upload.")
}

fn image_error(message: impl Into<String>) -> AcpError {
    AcpError::protocol(message.into())
}
