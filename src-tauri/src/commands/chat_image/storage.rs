use std::path::Path;

use crate::app_error::AppCommandError;
use crate::commands::chat_attachments::{
    ensure_managed_chat_dir, new_attachment_dir, sanitize_file_name, sanitize_session_bucket,
    user_facing_path, StagedChatAttachment,
};

use super::CHAT_IMAGE_DERIVED_MAX_BYTES;

const MAX_IMAGE_FILE_STEM_CHARS: usize = 170;
const CONVERSATION_ATTACHMENTS_DIR: &str = "conversation-attachments";

pub(crate) struct StageChatImageBytes<'a> {
    pub chat_dir: Option<&'a Path>,
    pub session_id: Option<&'a str>,
    pub file_name: &'a str,
    pub mime_type: &'a str,
    pub bytes: &'a [u8],
}

fn prepared_image_file_name(raw: &str, mime_type: &str) -> Result<String, AppCommandError> {
    let extension = match mime_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => {
            return Err(AppCommandError::invalid_input(
                "Prepared image MIME type is not supported",
            ))
        }
    };
    let sanitized = sanitize_file_name(raw);
    let stem = Path::new(&sanitized)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let stem: String = stem.chars().take(MAX_IMAGE_FILE_STEM_CHARS).collect();
    Ok(format!(
        "{}.{}",
        stem.trim_end_matches(['.', ' ']),
        extension
    ))
}

fn validate_image_bytes(bytes: &[u8]) -> Result<(), AppCommandError> {
    if bytes.is_empty() {
        return Err(AppCommandError::invalid_input("Prepared image is empty"));
    }
    if bytes.len() > CHAT_IMAGE_DERIVED_MAX_BYTES {
        return Err(AppCommandError::invalid_input(
            "Prepared image exceeds the size limit",
        ));
    }
    Ok(())
}

pub(crate) async fn stage_chat_image_bytes_core(
    data_dir: &Path,
    request: StageChatImageBytes<'_>,
) -> Result<StagedChatAttachment, AppCommandError> {
    validate_image_bytes(request.bytes)?;
    let base = if let Some(chat_dir) = request.chat_dir {
        ensure_managed_chat_dir(data_dir, chat_dir)
            .await?
            .join("attachments")
    } else {
        data_dir
            .join(CONVERSATION_ATTACHMENTS_DIR)
            .join(sanitize_session_bucket(request.session_id))
    };
    let attachment_dir = new_attachment_dir(&base).await?;
    let file_name = prepared_image_file_name(request.file_name, request.mime_type)?;
    let destination = attachment_dir.join(file_name);
    if let Err(error) = tokio::fs::write(&destination, request.bytes).await {
        let _ = tokio::fs::remove_dir_all(&attachment_dir).await;
        return Err(AppCommandError::io(error));
    }
    Ok(StagedChatAttachment {
        path: user_facing_path(&destination),
    })
}
