use std::path::PathBuf;
use std::time::Instant;

use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::acp::capability_policy::CapabilityRevocationMonitor;
use crate::app_error::AppCommandError;
use crate::commands::chat_attachments::StagedChatAttachment;
use crate::commands::chat_image_upload;

use super::{
    encode_chat_image_path, stage_chat_image_bytes_core, EncodedChatImage, PreparedChatImage,
    StageChatImageBytes,
};

pub(crate) struct PrepareChatImageRequest {
    pub path: PathBuf,
    pub data_dir: PathBuf,
    pub chat_dir: Option<PathBuf>,
    pub session_id: Option<String>,
    pub display_name: Option<String>,
}

struct CompletedImage {
    image: EncodedChatImage,
    result: PreparedChatImage,
    delivery_mode: &'static str,
}

fn display_name(request: &PrepareChatImageRequest) -> String {
    request
        .display_name
        .clone()
        .or_else(|| {
            request
                .path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "image".to_string())
}

fn log_prepare_failure(stage: &str, image: &EncodedChatImage, error: &AppCommandError) {
    tracing::error!(
        target: "chat.image",
        stage,
        file_name = %image.name,
        mime_type = %image.mime_type,
        source_bytes = image.source_bytes,
        derived_bytes = image.bytes.len(),
        error = %error,
        "chat image preparation failed"
    );
}

async fn stage_prepared_image(
    request: &PrepareChatImageRequest,
    image: &EncodedChatImage,
) -> Result<StagedChatAttachment, AppCommandError> {
    stage_chat_image_bytes_core(
        &request.data_dir,
        StageChatImageBytes {
            chat_dir: request.chat_dir.as_deref(),
            session_id: request.session_id.as_deref(),
            file_name: &image.name,
            mime_type: image.mime_type,
            bytes: &image.bytes,
        },
    )
    .await
    .map_err(|error| {
        log_prepare_failure("local_storage", image, &error);
        error
    })
}

fn complete_image(
    image: EncodedChatImage,
    local_path: String,
    upload: chat_image_upload::ImageUploadOutcome,
) -> CompletedImage {
    let (url, data, delivery_mode) = match upload {
        chat_image_upload::ImageUploadOutcome::Uploaded(url) => (Some(url), None, "tos"),
        chat_image_upload::ImageUploadOutcome::Unavailable(error) => {
            tracing::warn!(
                target: "chat.image",
                file_name = %image.name,
                mime_type = %image.mime_type,
                source_bytes = image.source_bytes,
                derived_bytes = image.bytes.len(),
                error_code = ?error.code,
                error = %error,
                "TOS image upload unavailable; using inline image fallback"
            );
            (None, Some(STANDARD.encode(&image.bytes)), "inline")
        }
    };
    let result = PreparedChatImage {
        url,
        data,
        local_path: Some(local_path),
        mime_type: image.mime_type.to_string(),
        name: image.name.clone(),
        source_bytes: image.source_bytes,
        derived_bytes: image.bytes.len(),
        width: image.width,
        height: image.height,
    };
    CompletedImage {
        image,
        result,
        delivery_mode,
    }
}

fn log_prepared(completed: &CompletedImage, started: &Instant) {
    tracing::info!(
        target: "chat.image",
        file_name = %completed.image.name,
        mime_type = %completed.image.mime_type,
        source_bytes = completed.image.source_bytes,
        derived_bytes = completed.image.bytes.len(),
        width = completed.image.width,
        height = completed.image.height,
        delivery_mode = completed.delivery_mode,
        has_local_path = completed.result.local_path.is_some(),
        elapsed_ms = started.elapsed().as_millis(),
        "prepared and stored chat image"
    );
}

pub(crate) async fn prepare_chat_image_core(
    conn: &sea_orm::DatabaseConnection,
    request: PrepareChatImageRequest,
    monitor: &CapabilityRevocationMonitor,
) -> Result<PreparedChatImage, AppCommandError> {
    let started = Instant::now();
    let name = display_name(&request);
    monitor.require_current().await?;
    let mut prepared = monitor
        .run_until_revoked(encode_chat_image_path(request.path.clone()))
        .await??;
    prepared.name = name;
    monitor.require_current().await?;
    let staged = stage_prepared_image(&request, &prepared).await?;
    if let Err(error) = monitor.require_current().await {
        remove_staged_image(&staged.path).await;
        return Err(error);
    }
    let upload = match chat_image_upload::upload_prepared(conn, &prepared).await {
        Ok(result) => result,
        Err(error) => {
            log_prepare_failure("tos_upload", &prepared, &error);
            remove_staged_image(&staged.path).await;
            return Err(error);
        }
    };
    if let Err(error) = monitor.require_current().await {
        remove_staged_image(&staged.path).await;
        return Err(error);
    }
    let completed = complete_image(prepared, staged.path, upload);
    log_prepared(&completed, &started);
    Ok(completed.result)
}

async fn remove_staged_image(path: &str) {
    let path = PathBuf::from(path);
    let _ = tokio::fs::remove_file(&path).await;
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::remove_dir(parent).await;
    }
}

#[cfg(feature = "tauri-runtime")]
pub(crate) fn effective_app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppCommandError> {
    use tauri::Manager;

    app.path()
        .app_data_dir()
        .map(|path| crate::paths::resolve_effective_data_dir(&path))
        .map_err(|error| {
            AppCommandError::io_error("App data directory unavailable")
                .with_detail(error.to_string())
        })
}
