use std::path::Path;
use std::sync::Arc;

use axum::extract::{Extension, Multipart};
use axum::Json;
use tokio::io::AsyncWriteExt;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::chat_image::{
    prepare_chat_image_named_core, PreparedChatImage, CHAT_IMAGE_SOURCE_MAX_BYTES,
};
use crate::paths::iyw_claw_uploads_root;

use super::files::{ensure_path_inside, reserve_upload_bytes, sanitize_upload_filename};
use super::upload_jail;

const IMAGE_UPLOAD_TMP_DIR: &str = ".image-tmp";

fn is_supported_mime(mime_type: &str) -> bool {
    matches!(
        mime_type.to_ascii_lowercase().as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    )
}

async fn prepare_upload_dirs(uploads_root: &Path, tmp_dir: &Path) -> Result<(), AppCommandError> {
    tokio::fs::create_dir_all(uploads_root)
        .await
        .map_err(AppCommandError::io)?;
    tokio::fs::create_dir_all(tmp_dir)
        .await
        .map_err(AppCommandError::io)?;
    let metadata = tokio::fs::symlink_metadata(tmp_dir)
        .await
        .map_err(AppCommandError::io)?;
    if metadata.file_type().is_symlink() {
        return Err(AppCommandError::invalid_input(
            "Refusing to use a symlinked image upload directory",
        ));
    }
    ensure_path_inside(tmp_dir, uploads_root).await?;
    Ok(())
}

async fn write_image_field(
    field: &mut axum::extract::multipart::Field<'_>,
    tmp_dir: &Path,
    staging_name: &str,
) -> Result<u64, AppCommandError> {
    let mut output = upload_jail::create_staging_file(tmp_dir, staging_name)
        .await
        .map_err(AppCommandError::io)?;
    let mut written = 0_u64;
    while let Some(chunk) = field.chunk().await.map_err(|error| {
        AppCommandError::io_error("Unable to read image upload").with_detail(error.to_string())
    })? {
        written = written.saturating_add(chunk.len() as u64);
        if written > CHAT_IMAGE_SOURCE_MAX_BYTES {
            return Err(AppCommandError::invalid_input(
                "Image exceeds the 100 MB source limit",
            ));
        }
        output
            .write_all(&chunk)
            .await
            .map_err(AppCommandError::io)?;
    }
    output.flush().await.map_err(AppCommandError::io)?;
    if written == 0 {
        return Err(AppCommandError::invalid_input("Image upload is empty"));
    }
    Ok(written)
}

async fn stream_image(
    multipart: &mut Multipart,
    tmp_dir: &Path,
    staging_name: &str,
) -> Result<(String, u64), AppCommandError> {
    let mut file_name: Option<String> = None;
    let mut size = 0;
    while let Some(mut field) = multipart.next_field().await.map_err(|error| {
        AppCommandError::invalid_input("Invalid image upload").with_detail(error.to_string())
    })? {
        match field.name().unwrap_or("") {
            "session_id" | "sessionId" => {
                field.text().await.map_err(|error| {
                    AppCommandError::invalid_input("Invalid session id")
                        .with_detail(error.to_string())
                })?;
            }
            "file" if file_name.is_none() => {
                let declared_mime = field.content_type().unwrap_or("").to_string();
                if !is_supported_mime(&declared_mime) {
                    return Err(AppCommandError::invalid_input(
                        "Image MIME type is not supported",
                    ));
                }
                file_name = Some(field.file_name().unwrap_or("image").to_string());
                size = write_image_field(&mut field, tmp_dir, staging_name).await?;
            }
            "file" => {
                return Err(AppCommandError::invalid_input(
                    "Only one image can be uploaded per request",
                ));
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    let name = file_name.ok_or_else(|| AppCommandError::invalid_input("Image file is missing"))?;
    Ok((name, size))
}

pub async fn upload_chat_image(
    Extension(state): Extension<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<PreparedChatImage>, AppCommandError> {
    let uploads_root = iyw_claw_uploads_root();
    let _quota_guard = reserve_upload_bytes(&uploads_root, CHAT_IMAGE_SOURCE_MAX_BYTES).await?;
    let tmp_dir = uploads_root.join(IMAGE_UPLOAD_TMP_DIR);
    prepare_upload_dirs(&uploads_root, &tmp_dir).await?;
    let staging_name = format!("{}.part", uuid::Uuid::new_v4().simple());
    let result = async {
        let (raw_name, size) = stream_image(&mut multipart, &tmp_dir, &staging_name).await?;
        let staged_path = tmp_dir.join(&staging_name);
        let display_name = sanitize_upload_filename(&raw_name);
        let prepared =
            prepare_chat_image_named_core(&state.db.conn, staged_path.clone(), display_name).await;
        upload_jail::remove_staging_best_effort(&tmp_dir, &staging_name).await;
        tracing::debug!(target: "chat.image", source_bytes = size, "processed temporary chat image upload");
        prepared
    }
    .await;
    if result.is_err() {
        upload_jail::remove_staging_best_effort(&tmp_dir, &staging_name).await;
    }
    result.map(Json)
}
