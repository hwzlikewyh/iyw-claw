use std::path::Path;

use axum::extract::Multipart;
use axum::Json;
use image::{ImageFormat, ImageReader};
use tokio::io::AsyncWriteExt;

use crate::app_error::AppCommandError;
use crate::commands::chat_image::CHAT_IMAGE_SOURCE_MAX_BYTES;
use crate::paths::iyw_claw_uploads_root;

use super::files::{
    ensure_path_inside, finalize_with_available_upload_name, reserve_upload_bytes,
    sanitize_session_bucket, sanitize_upload_filename, UploadAttachmentResult,
};
use super::upload_jail;

const IMAGE_UPLOAD_TMP_DIR: &str = ".image-tmp";

fn is_supported_mime(mime_type: &str) -> bool {
    matches!(
        mime_type.to_ascii_lowercase().as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    )
}

async fn validate_image_format(path: &Path) -> Result<String, AppCommandError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let reader = ImageReader::open(path)
            .map_err(|error| {
                AppCommandError::invalid_input("Unable to inspect uploaded image")
                    .with_detail(error.to_string())
            })?
            .with_guessed_format()
            .map_err(|error| {
                AppCommandError::invalid_input("Unable to inspect uploaded image")
                    .with_detail(error.to_string())
            })?;
        match reader.format() {
            Some(ImageFormat::Png) => Ok("image/png".to_string()),
            Some(ImageFormat::Jpeg) => Ok("image/jpeg".to_string()),
            Some(ImageFormat::WebP) => Ok("image/webp".to_string()),
            Some(ImageFormat::Gif) => Ok("image/gif".to_string()),
            _ => Err(AppCommandError::invalid_input(
                "Uploaded file is not a supported image",
            )),
        }
    })
    .await
    .map_err(|error| {
        AppCommandError::task_execution_failed("Image validation task failed")
            .with_detail(error.to_string())
    })?
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
) -> Result<(String, Option<String>, u64), AppCommandError> {
    let mut file_name: Option<String> = None;
    let mut session_id: Option<String> = None;
    let mut size = 0;
    while let Some(mut field) = multipart.next_field().await.map_err(|error| {
        AppCommandError::invalid_input("Invalid image upload").with_detail(error.to_string())
    })? {
        match field.name().unwrap_or("") {
            "session_id" | "sessionId" => {
                let value = field.text().await.map_err(|error| {
                    AppCommandError::invalid_input("Invalid session id")
                        .with_detail(error.to_string())
                })?;
                session_id = Some(sanitize_session_bucket(value.trim()));
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
    Ok((name, session_id, size))
}

async fn finalize_image_upload(
    uploads_root: &Path,
    tmp_dir: &Path,
    staging_name: &str,
    raw_name: &str,
    session_id: Option<&str>,
) -> Result<(String, String), AppCommandError> {
    let bucket = sanitize_session_bucket(session_id.unwrap_or("conversation"));
    let bucket_dir = uploads_root.join(bucket);
    tokio::fs::create_dir_all(&bucket_dir)
        .await
        .map_err(AppCommandError::io)?;
    ensure_path_inside(&bucket_dir, uploads_root).await?;
    let safe_name = sanitize_upload_filename(raw_name);
    let final_name =
        finalize_with_available_upload_name(tmp_dir, staging_name, &bucket_dir, &safe_name).await?;
    let final_path = ensure_path_inside(&bucket_dir.join(&final_name), uploads_root).await?;
    Ok((final_path.to_string_lossy().to_string(), final_name))
}

pub async fn upload_chat_image(
    mut multipart: Multipart,
) -> Result<Json<UploadAttachmentResult>, AppCommandError> {
    let uploads_root = iyw_claw_uploads_root();
    let _quota_guard = reserve_upload_bytes(&uploads_root, CHAT_IMAGE_SOURCE_MAX_BYTES).await?;
    let tmp_dir = uploads_root.join(IMAGE_UPLOAD_TMP_DIR);
    prepare_upload_dirs(&uploads_root, &tmp_dir).await?;
    let staging_name = format!("{}.part", uuid::Uuid::new_v4().simple());
    let result = async {
        let (raw_name, session_id, size) =
            stream_image(&mut multipart, &tmp_dir, &staging_name).await?;
        let staged_path = tmp_dir.join(&staging_name);
        let mime_type = validate_image_format(&staged_path).await?;
        let (path, name) = finalize_image_upload(
            &uploads_root,
            &tmp_dir,
            &staging_name,
            &raw_name,
            session_id.as_deref(),
        )
        .await?;
        Ok(UploadAttachmentResult {
            path,
            name,
            size,
            mime_type: Some(mime_type),
        })
    }
    .await;
    if result.is_err() {
        upload_jail::remove_staging_best_effort(&tmp_dir, &staging_name).await;
    }
    result.map(Json)
}
