use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::acp::capability_policy::{monitor_file_upload, require_file_upload};
use crate::app_error::AppCommandError;
use crate::commands::chat_image::CHAT_IMAGE_SOURCE_MAX_BYTES;
use crate::commands::remote_proxy::{self, RemoteChatImageFile, RemoteProxyState};
use crate::db::AppDatabase;

const CHUNK_MAX_BYTES: usize = 512 * 1024;
const FILE_NAME_MAX_CHARS: usize = 180;

mod upload_state;
pub use upload_state::{cleanup_stale_uploads, RemoteChatImageUploadState};
use upload_state::{
    create_upload_staging_file, ensure_upload_root, image_mime_for_name, monitor_upload_revocation,
    remove_upload_file, validate_upload_file, BeginUploadStaging, UploadEntry, UploadRevocation,
    UPLOAD_DIR,
};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginUploadResult {
    upload_id: String,
}

fn parse_upload_id(raw: &str) -> Result<Uuid, AppCommandError> {
    Uuid::parse_str(raw).map_err(|_| AppCommandError::invalid_input("Invalid image upload id"))
}

fn sanitize_file_name(raw: &str) -> String {
    let base = Path::new(raw)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let cleaned: String = base
        .chars()
        .filter(|ch| !ch.is_control())
        .map(|ch| if matches!(ch, '/' | '\\') { '_' } else { ch })
        .take(FILE_NAME_MAX_CHARS)
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']);
    if cleaned.is_empty() {
        "image".to_string()
    } else {
        cleaned.to_string()
    }
}

fn validate_descriptor(
    file_name: &str,
    mime_type: &str,
    expected_bytes: u64,
) -> Result<(String, String), AppCommandError> {
    if expected_bytes == 0 || expected_bytes > CHAT_IMAGE_SOURCE_MAX_BYTES {
        return Err(AppCommandError::invalid_input(
            "Image size must be between 1 byte and 100 MB",
        ));
    }
    let safe_name = sanitize_file_name(file_name);
    let expected_mime = image_mime_for_name(&safe_name)?;
    let supplied_mime = mime_type.trim().to_ascii_lowercase();
    if !supplied_mime.is_empty() && supplied_mime != expected_mime {
        return Err(AppCommandError::invalid_input(
            "Image MIME type does not match its extension",
        ));
    }
    Ok((safe_name, expected_mime.to_string()))
}

fn upload_root(app: &AppHandle) -> Result<PathBuf, AppCommandError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map(|path| crate::paths::resolve_effective_data_dir(&path))
        .map_err(|error| {
            AppCommandError::io_error("App data directory unavailable")
                .with_detail(error.to_string())
        })?;
    Ok(data_dir.join(UPLOAD_DIR))
}

#[tauri::command]
pub async fn remote_chat_image_upload_begin(
    app: AppHandle,
    state: State<'_, RemoteChatImageUploadState>,
    file_name: String,
    mime_type: String,
    expected_bytes: u64,
) -> Result<BeginUploadResult, AppCommandError> {
    let monitor = monitor_file_upload(None).await?;
    let (file_name, mime_type) = validate_descriptor(&file_name, &mime_type, expected_bytes)?;
    let mut uploads = state.uploads.lock().await;
    let root = upload_root(&app)?;
    let root_was_present = ensure_upload_root(&monitor, &root).await?;
    let upload_id = Uuid::new_v4();
    let path = root.join(format!("{}.part", upload_id.simple()));
    let staging = BeginUploadStaging {
        upload_id,
        root: &root,
        path: &path,
        root_was_present,
    };
    create_upload_staging_file(&monitor, &staging).await?;
    let revoked = monitor.cancellation();
    let finished = CancellationToken::new();
    uploads.insert(
        upload_id,
        UploadEntry {
            path: path.clone(),
            file_name: file_name.clone(),
            mime_type: mime_type.clone(),
            expected_bytes,
            received_bytes: 0,
            monitor,
            finished: finished.clone(),
        },
    );
    drop(uploads);
    monitor_upload_revocation(UploadRevocation {
        uploads: Arc::clone(&state.uploads),
        upload_id,
        path,
        revoked,
        finished,
    });
    tracing::info!(target: "chat.image", %upload_id, %file_name, %mime_type, expected_bytes, "began chunked image upload");
    Ok(BeginUploadResult {
        upload_id: upload_id.to_string(),
    })
}

#[tauri::command]
pub async fn remote_chat_image_upload_append(
    state: State<'_, RemoteChatImageUploadState>,
    upload_id: String,
    offset: u64,
    chunk: Vec<u8>,
) -> Result<u64, AppCommandError> {
    require_file_upload().await?;
    if chunk.is_empty() || chunk.len() > CHUNK_MAX_BYTES {
        return Err(AppCommandError::invalid_input(
            "Invalid image upload chunk size",
        ));
    }
    let upload_id = parse_upload_id(&upload_id)?;
    let mut uploads = state.uploads.lock().await;
    let entry = uploads
        .get_mut(&upload_id)
        .ok_or_else(|| AppCommandError::not_found("Image upload was not found"))?;
    entry.monitor.error_if_revoked()?;
    let next = offset
        .checked_add(chunk.len() as u64)
        .filter(|value| *value <= entry.expected_bytes && *value <= CHAT_IMAGE_SOURCE_MAX_BYTES)
        .ok_or_else(|| AppCommandError::invalid_input("Image upload exceeds its size limit"))?;
    if entry.received_bytes != offset {
        return Err(AppCommandError::invalid_input(
            "Image upload offset mismatch",
        ));
    }
    let metadata = entry
        .monitor
        .run_until_revoked(tokio::fs::symlink_metadata(&entry.path))
        .await?
        .map_err(AppCommandError::io)?;
    if !metadata.file_type().is_file() || metadata.len() != offset {
        return Err(AppCommandError::invalid_input(
            "Image upload file state mismatch",
        ));
    }
    let mut file = entry
        .monitor
        .run_until_revoked(tokio::fs::OpenOptions::new().append(true).open(&entry.path))
        .await?
        .map_err(AppCommandError::io)?;
    entry
        .monitor
        .run_until_revoked(file.write_all(&chunk))
        .await?
        .map_err(AppCommandError::io)?;
    entry.monitor.require_current().await?;
    entry.received_bytes = next;
    tracing::debug!(target: "chat.image", %upload_id, offset, chunk_bytes = chunk.len(), received_bytes = next, "appended image upload chunk");
    Ok(next)
}

#[tauri::command]
pub async fn remote_chat_image_upload_finish(
    app: AppHandle,
    db: State<'_, AppDatabase>,
    proxy: State<'_, Arc<RemoteProxyState>>,
    state: State<'_, RemoteChatImageUploadState>,
    connection_id: Option<i32>,
    upload_id: String,
    session_id: Option<String>,
    chat_dir: Option<String>,
) -> Result<Value, AppCommandError> {
    require_file_upload().await?;
    let upload_id = parse_upload_id(&upload_id)?;
    let entry = state
        .uploads
        .lock()
        .await
        .remove(&upload_id)
        .ok_or_else(|| AppCommandError::not_found("Image upload was not found"))?;
    if let Err(error) = validate_upload_file(&entry).await {
        entry.finished.cancel();
        remove_upload_file(upload_id, &entry.path).await;
        return Err(error);
    }
    let UploadEntry {
        path,
        file_name,
        mime_type,
        monitor,
        finished,
        ..
    } = entry;
    let operation = async {
        if let Some(connection_id) = connection_id {
            remote_proxy::upload_chat_image_file_to_remote(
                db.inner(),
                proxy.inner().as_ref(),
                RemoteChatImageFile {
                    connection_id,
                    path: path.clone(),
                    file_name: file_name.clone(),
                    mime_type: mime_type.clone(),
                    session_id,
                    chat_dir,
                },
            )
            .await
        } else {
            let prepared = crate::commands::chat_image::prepare_chat_image_core(
                &db.conn,
                crate::commands::chat_image::PrepareChatImageRequest {
                    path: path.clone(),
                    data_dir: crate::commands::chat_image::effective_app_data_dir(&app)?,
                    chat_dir: chat_dir.map(PathBuf::from),
                    session_id,
                    display_name: Some(file_name.clone()),
                },
                &monitor,
            )
            .await?;
            serde_json::to_value(prepared).map_err(|error| {
                AppCommandError::task_execution_failed("Unable to serialize prepared image")
                    .with_detail(error.to_string())
            })
        }
    };
    let result = match monitor.run_until_revoked(operation).await {
        Ok(result) => result,
        Err(error) => Err(error),
    };
    finished.cancel();
    remove_upload_file(upload_id, &path).await;
    result
}

#[tauri::command]
pub async fn remote_chat_image_upload_abort(
    state: State<'_, RemoteChatImageUploadState>,
    upload_id: String,
) -> Result<(), AppCommandError> {
    let upload_id = parse_upload_id(&upload_id)?;
    if let Some(entry) = state.uploads.lock().await.remove(&upload_id) {
        entry.finished.cancel();
        remove_upload_file(upload_id, &entry.path).await;
        tracing::info!(target: "chat.image", %upload_id, "aborted chunked image upload");
    }
    Ok(())
}
