use std::path::PathBuf;
use std::time::Instant;

use crate::acp::capability_policy::CapabilityRevocationMonitor;
use crate::app_error::AppCommandError;
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

pub(crate) async fn prepare_chat_image_core(
    conn: &sea_orm::DatabaseConnection,
    request: PrepareChatImageRequest,
    monitor: &CapabilityRevocationMonitor,
) -> Result<PreparedChatImage, AppCommandError> {
    let started = Instant::now();
    let name = display_name(&request);
    monitor.require_current().await?;
    let mut prepared = monitor
        .run_until_revoked(encode_chat_image_path(request.path))
        .await??;
    prepared.name = name;
    monitor.require_current().await?;
    let staged = stage_chat_image_bytes_core(
        &request.data_dir,
        StageChatImageBytes {
            chat_dir: request.chat_dir.as_deref(),
            session_id: request.session_id.as_deref(),
            file_name: &prepared.name,
            mime_type: prepared.mime_type,
            bytes: &prepared.bytes,
        },
    )
    .await
    .map_err(|error| {
        log_prepare_failure("local_storage", &prepared, &error);
        error
    })?;
    if let Err(error) = monitor.require_current().await {
        remove_staged_image(&staged.path).await;
        return Err(error);
    }
    let mut result = match chat_image_upload::upload_prepared(conn, &prepared).await {
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
    result.local_path = Some(staged.path);
    tracing::info!(
        target: "chat.image",
        file_name = %prepared.name,
        mime_type = %prepared.mime_type,
        source_bytes = prepared.source_bytes,
        derived_bytes = prepared.bytes.len(),
        width = prepared.width,
        height = prepared.height,
        has_local_path = result.local_path.is_some(),
        elapsed_ms = started.elapsed().as_millis(),
        "prepared, stored, and uploaded chat image"
    );
    Ok(result)
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
