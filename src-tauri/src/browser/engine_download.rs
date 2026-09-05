use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
#[path = "engine_download_archive.rs"]
mod archive;
use super::engine::{probe_engine, BrowserEngine};
use super::error::{BrowserError, BrowserErrorCode};
use super::types::BrowserEngineKind;

const CHROMIUM_VERSION: &str = "152.0.7977.64";
const CHROMIUM_ARCHIVE_URL: &str =
    "https://storage.googleapis.com/chrome-for-testing-public/152.0.7977.64/win64/chrome-win64.zip";
const CHROMIUM_ARCHIVE_SIZE: u64 = 202_713_690;
const CHROMIUM_ARCHIVE_SHA256: &str =
    "b0db25dea445822429d8ebd36d53344cadcd63127308759456964029bbe18004";
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) fn managed_engine_root(data_root: &Path) -> PathBuf {
    data_root.join("browser").join("chromium")
}

pub(super) fn managed_engine_path(data_root: &Path) -> PathBuf {
    managed_engine_root(data_root).join("chrome.exe")
}

pub(super) async fn ensure_managed_engine(
    data_root: &Path,
    cancellation: CancellationToken,
) -> Result<BrowserEngine, BrowserError> {
    let root = managed_engine_root(data_root);
    let parent = root
        .parent()
        .ok_or_else(|| unavailable("managed Chromium path has no parent"))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| unavailable(format!("failed to create Chromium directory: {error}")))?;
    let lock = acquire_lock(&parent.join("chromium.lock"))?;

    if let Some(engine) = probe_engine(
        BrowserEngineKind::Chromium,
        managed_engine_path(data_root),
        None,
    )
    .await
    {
        drop(lock);
        return Ok(engine);
    }
    if cancellation.is_cancelled() {
        return Err(BrowserError::shutting_down());
    }

    let staging = parent.join(format!(".chromium-staging-{}", Uuid::new_v4().simple()));
    let archive = parent.join("chromium.zip.part");
    let result = install_managed_engine(&root, &staging, &archive, &cancellation).await;
    cleanup_path(&staging, &archive).await;
    drop(lock);
    result
}

async fn install_managed_engine(
    root: &Path,
    staging: &Path,
    archive: &Path,
    cancellation: &CancellationToken,
) -> Result<BrowserEngine, BrowserError> {
    tokio::fs::create_dir_all(staging).await.map_err(|error| {
        unavailable(format!(
            "failed to create Chromium staging directory: {error}"
        ))
    })?;
    download_archive(archive, cancellation).await?;
    let archive_for_extract = archive.to_path_buf();
    let staging_for_extract = staging.to_path_buf();
    tokio::task::spawn_blocking(move || {
        archive::extract(&archive_for_extract, &staging_for_extract)
    })
    .await
    .map_err(|error| unavailable(format!("Chromium extraction worker failed: {error}")))?
    .map_err(|error| unavailable(format!("Chromium archive extraction failed: {error}")))?;

    let staged_engine = staging.join("chrome.exe");
    let staged_engine = probe_engine(BrowserEngineKind::Chromium, staged_engine, None)
        .await
        .ok_or_else(|| unavailable("downloaded Chromium failed its startup probe"))?;
    if !staged_engine.version.contains(CHROMIUM_VERSION) {
        return Err(unavailable(
            "downloaded Chromium version does not match the pinned release",
        ));
    }
    archive::replace_cache(root, staging)?;
    probe_engine(BrowserEngineKind::Chromium, root.join("chrome.exe"), None)
        .await
        .ok_or_else(|| unavailable("installed Chromium failed its startup probe"))
}

async fn download_archive(
    destination: &Path,
    cancellation: &CancellationToken,
) -> Result<(), BrowserError> {
    let client = reqwest::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .read_timeout(DOWNLOAD_READ_TIMEOUT)
        .user_agent(concat!("iyw-claw/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| unavailable(format!("failed to create Chromium downloader: {error}")))?;
    let response = client
        .get(CHROMIUM_ARCHIVE_URL)
        .send()
        .await
        .map_err(|error| {
            unavailable(format!(
                "Chromium download request failed: {}",
                error.without_url()
            ))
        })?;
    if !response.status().is_success() {
        return Err(unavailable(format!(
            "Chromium download returned HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length != CHROMIUM_ARCHIVE_SIZE)
    {
        return Err(unavailable(
            "Chromium download size does not match the pinned package",
        ));
    }

    stream_archive(response, destination, cancellation).await
}

async fn stream_archive(
    response: reqwest::Response,
    destination: &Path,
    cancellation: &CancellationToken,
) -> Result<(), BrowserError> {
    let mut file = tokio::fs::File::create(destination)
        .await
        .map_err(|error| unavailable(format!("failed to create Chromium archive: {error}")))?;
    let (downloaded, archive_sha256) =
        stream_archive_chunks(response, &mut file, cancellation).await?;
    file.flush()
        .await
        .map_err(|error| unavailable(format!("failed to flush Chromium archive: {error}")))?;
    file.sync_all()
        .await
        .map_err(|error| unavailable(format!("failed to sync Chromium archive: {error}")))?;
    validate_archive_digest(downloaded, &archive_sha256)?;
    tracing::info!(
        target: "iyw_claw_browser",
        archive_bytes = downloaded,
        archive_sha256 = %archive_sha256,
        "managed Chromium archive downloaded"
    );
    Ok(())
}

async fn stream_archive_chunks(
    response: reqwest::Response,
    file: &mut tokio::fs::File,
    cancellation: &CancellationToken,
) -> Result<(u64, String), BrowserError> {
    let mut stream = response.bytes_stream();
    let mut downloaded = 0_u64;
    let mut hasher = Sha256::new();
    while let Some(chunk) = tokio::select! {
        _ = cancellation.cancelled() => return Err(BrowserError::shutting_down()),
        chunk = stream.next() => chunk,
    } {
        let chunk = chunk.map_err(|error| {
            unavailable(format!(
                "Chromium download stream failed: {}",
                error.without_url()
            ))
        })?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > CHROMIUM_ARCHIVE_SIZE {
            return Err(unavailable(
                "Chromium download exceeded the pinned package size",
            ));
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|error| unavailable(format!("failed to write Chromium archive: {error}")))?;
    }
    Ok((downloaded, format!("{:x}", hasher.finalize())))
}

fn validate_archive_digest(downloaded: u64, archive_sha256: &str) -> Result<(), BrowserError> {
    if downloaded != CHROMIUM_ARCHIVE_SIZE {
        return Err(unavailable(
            "Chromium download ended before the pinned size",
        ));
    }
    if archive_sha256 != CHROMIUM_ARCHIVE_SHA256 {
        return Err(unavailable(
            "Chromium download digest does not match the pinned package",
        ));
    }
    Ok(())
}

fn acquire_lock(path: &Path) -> Result<File, BrowserError> {
    let parent = path
        .parent()
        .ok_or_else(|| unavailable("Chromium lock path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| {
        unavailable(format!("failed to create Chromium lock directory: {error}"))
    })?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| unavailable(format!("failed to open Chromium lock: {error}")))?;
    file.lock()
        .map_err(|error| unavailable(format!("failed to lock Chromium download: {error}")))?;
    Ok(file)
}

async fn cleanup_path(staging: &Path, archive: &Path) {
    let _ = tokio::fs::remove_dir_all(staging).await;
    let _ = tokio::fs::remove_file(archive).await;
}
fn unavailable(message: impl Into<String>) -> BrowserError {
    BrowserError::new(BrowserErrorCode::BrowserRuntimeUnavailable, message).retryable(true)
}
