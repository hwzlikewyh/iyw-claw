#[cfg(feature = "tauri-runtime")]
pub mod desktop;
#[cfg(feature = "tauri-runtime")]
pub mod remote;
mod serving;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::app_error::AppCommandError;
pub use serving::serve;

const MAX_PREVIEWS: usize = 32;
const LEASE_TIMEOUT: Duration = Duration::from_secs(120);
const LEASE_CHECK_INTERVAL: Duration = Duration::from_secs(30);
const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MODEL_BYTES: u64 = 128 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

fn byte_limit(path: &Path) -> u64 {
    match extension(path).as_str() {
        "pdf" => MAX_DOCUMENT_BYTES,
        "glb" | "gltf" | "bin" => MAX_MODEL_BYTES,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg" | "ktx2" => MAX_IMAGE_BYTES,
        _ => MAX_PREVIEW_BYTES,
    }
}

pub(super) struct Resource {
    root: PathBuf,
    target: PathBuf,
    assets: bool,
    version: (u64, Option<std::time::SystemTime>),
    touched: Mutex<Instant>,
    pub(super) cancelled: CancellationToken,
}

static RESOURCES: LazyLock<Mutex<HashMap<String, Arc<Resource>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPreviewParams {
    pub root_path: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct PreviewIdParams {
    pub id: String,
}

#[derive(Serialize, Deserialize)]
pub struct PreviewResource {
    pub id: String,
    pub url: String,
    pub size: u64,
}

fn resources() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Resource>>> {
    RESOURCES.lock().unwrap_or_else(|error| error.into_inner())
}

fn is_asset(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "bin" | "png" | "jpg" | "jpeg" | "webp" | "ktx2"
    )
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_preview(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "pdf"
            | "mp4"
            | "webm"
            | "mov"
            | "m4v"
            | "ogv"
            | "mkv"
            | "mp3"
            | "wav"
            | "ogg"
            | "m4a"
            | "aac"
            | "flac"
            | "opus"
            | "glb"
            | "gltf"
            | "png"
            | "jpg"
            | "jpeg"
            | "webp"
            | "gif"
            | "bmp"
            | "svg"
    )
}

async fn validate(
    params: OpenPreviewParams,
) -> Result<(PathBuf, PathBuf, std::fs::Metadata), AppCommandError> {
    let root = tokio::fs::canonicalize(params.root_path)
        .await
        .map_err(AppCommandError::io)?;
    let target = tokio::fs::canonicalize(root.join(params.path))
        .await
        .map_err(AppCommandError::io)?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(AppCommandError::io)?;
    if !target.starts_with(&root) || !metadata.is_file() || !is_preview(&target) {
        return Err(AppCommandError::invalid_input(
            "Unsupported preview or path outside workspace",
        ));
    }
    if metadata.len() > byte_limit(&target) {
        return Err(AppCommandError::invalid_input(
            "File exceeds preview size limit",
        ));
    }
    Ok((root, target, metadata))
}

pub async fn open(params: OpenPreviewParams) -> Result<PreviewResource, AppCommandError> {
    let (root, target, metadata) = validate(params).await?;
    let size = metadata.len();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let resource = Arc::new(Resource {
        root,
        assets: matches!(extension(&target).as_str(), "glb" | "gltf"),
        version: (size, metadata.modified().ok()),
        target,
        touched: Mutex::new(Instant::now()),
        cancelled: CancellationToken::new(),
    });
    {
        let mut entries = resources();
        if entries.len() >= MAX_PREVIEWS {
            return Err(AppCommandError::invalid_input("Too many open previews"));
        }
        entries.insert(id.clone(), resource.clone());
    }
    expire(id.clone(), resource);
    tracing::debug!(size, "[preview] resource opened");
    Ok(PreviewResource {
        url: format!("/api/preview_resource/{id}/file"),
        id,
        size,
    })
}

pub fn renew(id: &str) -> Result<(), AppCommandError> {
    let resource = resources()
        .get(id)
        .cloned()
        .ok_or_else(|| AppCommandError::not_found("Preview expired"))?;
    *resource
        .touched
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Instant::now();
    Ok(())
}

pub fn close(id: &str) {
    if let Some(resource) = resources().remove(id) {
        resource.cancelled.cancel();
        tracing::debug!("[preview] resource closed; active streams cancelled");
    }
}

fn expire(id: String, resource: Arc<Resource>) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = resource.cancelled.cancelled() => break,
                _ = tokio::time::sleep(LEASE_CHECK_INTERVAL) => {
                    let elapsed = resource.touched.lock().unwrap_or_else(|error| error.into_inner()).elapsed();
                    if elapsed >= LEASE_TIMEOUT {
                        close(&id);
                        break;
                    }
                }
            }
        }
    });
}

pub async fn open_handler(
    axum::Json(params): axum::Json<OpenPreviewParams>,
) -> Result<axum::Json<PreviewResource>, AppCommandError> {
    open(params).await.map(axum::Json)
}

pub async fn close_handler(axum::Json(params): axum::Json<PreviewIdParams>) -> axum::Json<()> {
    close(&params.id);
    axum::Json(())
}

pub async fn renew_handler(
    axum::Json(params): axum::Json<PreviewIdParams>,
) -> Result<axum::Json<()>, AppCommandError> {
    renew(&params.id).map(axum::Json)
}
