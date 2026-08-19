use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::acp::capability_policy::CapabilityRevocationMonitor;
use crate::app_error::AppCommandError;

pub(super) const UPLOAD_DIR: &str = ".remote-chat-image-upload";
const STALE_UPLOAD_AGE: Duration = Duration::from_secs(24 * 60 * 60);

pub(super) struct UploadEntry {
    pub(super) path: PathBuf,
    pub(super) file_name: String,
    pub(super) mime_type: String,
    pub(super) expected_bytes: u64,
    pub(super) received_bytes: u64,
    pub(super) monitor: CapabilityRevocationMonitor,
    pub(super) finished: CancellationToken,
}

#[derive(Default)]
pub struct RemoteChatImageUploadState {
    pub(super) uploads: Arc<Mutex<HashMap<Uuid, UploadEntry>>>,
}

pub(super) struct UploadRevocation {
    pub(super) uploads: Arc<Mutex<HashMap<Uuid, UploadEntry>>>,
    pub(super) upload_id: Uuid,
    pub(super) path: PathBuf,
    pub(super) revoked: CancellationToken,
    pub(super) finished: CancellationToken,
}

pub(super) struct BeginUploadStaging<'a> {
    pub(super) upload_id: Uuid,
    pub(super) root: &'a Path,
    pub(super) path: &'a Path,
    pub(super) root_was_present: bool,
}

pub(super) fn image_mime_for_name(file_name: &str) -> Result<&'static str, AppCommandError> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        "gif" => Ok("image/gif"),
        _ => Err(AppCommandError::invalid_input(
            "Image file extension is not supported",
        )),
    }
}

pub fn cleanup_stale_uploads(data_dir: &Path) {
    let root = data_dir.join(UPLOAD_DIR);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if path.extension().and_then(|value| value.to_str()) != Some("part")
            || Uuid::parse_str(stem).is_err()
        {
            continue;
        }
        let is_stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
            .is_ok_and(|age| age >= STALE_UPLOAD_AGE);
        if is_stale {
            if let Err(error) = std::fs::remove_file(path) {
                tracing::warn!(target: "chat.image", %error, "failed to clean stale image upload");
            }
        }
    }
}

pub(super) async fn remove_upload_file(upload_id: Uuid, path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(target: "chat.image", %upload_id, %error, "failed to clean image upload file");
        }
    }
}

async fn remove_upload_root_if_empty(root: &Path) {
    if let Err(error) = tokio::fs::remove_dir(root).await {
        if !matches!(
            error.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
        ) {
            tracing::warn!(target: "chat.image", %error, "failed to clean empty image upload root");
        }
    }
}

async fn remove_upload_root_if_new(root: &Path, root_was_present: bool) {
    if !root_was_present {
        remove_upload_root_if_empty(root).await;
    }
}

async fn cleanup_failed_upload_begin(staging: &BeginUploadStaging<'_>) {
    remove_upload_file(staging.upload_id, staging.path).await;
    remove_upload_root_if_new(staging.root, staging.root_was_present).await;
}

pub(super) async fn ensure_upload_root(
    monitor: &CapabilityRevocationMonitor,
    root: &Path,
) -> Result<bool, AppCommandError> {
    let root_was_present = tokio::fs::try_exists(root)
        .await
        .map_err(AppCommandError::io)?;
    monitor.require_current().await?;
    if let Err(error) = tokio::fs::create_dir_all(root).await {
        remove_upload_root_if_new(root, root_was_present).await;
        return Err(AppCommandError::io(error));
    }
    if let Err(error) = monitor.require_current().await {
        remove_upload_root_if_new(root, root_was_present).await;
        return Err(error);
    }
    Ok(root_was_present)
}

pub(super) async fn create_upload_staging_file(
    monitor: &CapabilityRevocationMonitor,
    staging: &BeginUploadStaging<'_>,
) -> Result<(), AppCommandError> {
    if let Err(error) = monitor.require_current().await {
        remove_upload_root_if_new(staging.root, staging.root_was_present).await;
        return Err(error);
    }
    let file_result = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(staging.path)
        .await;
    if let Err(error) = file_result {
        cleanup_failed_upload_begin(staging).await;
        return Err(AppCommandError::io(error));
    }
    if let Err(error) = monitor.require_current().await {
        cleanup_failed_upload_begin(staging).await;
        return Err(error);
    }
    Ok(())
}

pub(super) async fn validate_upload_file(entry: &UploadEntry) -> Result<(), AppCommandError> {
    image_mime_for_name(&entry.file_name)?;
    let metadata = tokio::fs::symlink_metadata(&entry.path)
        .await
        .map_err(AppCommandError::io)?;
    if !metadata.file_type().is_file()
        || metadata.len() != entry.expected_bytes
        || entry.received_bytes != entry.expected_bytes
    {
        return Err(AppCommandError::invalid_input("Image upload is incomplete"));
    }
    Ok(())
}

pub(super) fn monitor_upload_revocation(watch: UploadRevocation) {
    tokio::spawn(async move {
        tokio::select! {
            _ = watch.finished.cancelled() => {}
            _ = watch.revoked.cancelled() => {
                let removed = watch.uploads.lock().await.remove(&watch.upload_id);
                if let Some(entry) = removed {
                    entry.finished.cancel();
                    remove_upload_file(watch.upload_id, &watch.path).await;
                    tracing::warn!(
                        target: "chat.image",
                        upload_id = %watch.upload_id,
                        "revoked chunked image upload and removed staging file"
                    );
                }
            }
        }
    });
}
