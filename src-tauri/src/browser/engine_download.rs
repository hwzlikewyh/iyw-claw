use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zip::ZipArchive;

use super::engine::{probe_engine, BrowserEngine};
use super::error::{BrowserError, BrowserErrorCode};
use super::types::BrowserEngineKind;

const CHROMIUM_VERSION: &str = "152.0.7977.64";
const CHROMIUM_ARCHIVE_URL: &str =
    "https://storage.googleapis.com/chrome-for-testing-public/152.0.7977.64/win64/chrome-win64.zip";
const CHROMIUM_ARCHIVE_SIZE: u64 = 202_713_690;
const CHROMIUM_ARCHIVE_SHA256: &str =
    "b0db25dea445822429d8ebd36d53344cadcd63127308759456964029bbe18004";
const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) fn managed_engine_root(data_root: &Path) -> PathBuf {
    data_root.join("browser").join("chromium")
}

pub(super) fn managed_engine_path(data_root: &Path) -> PathBuf {
    managed_engine_root(data_root).join("chrome.exe")
}

pub(super) async fn detect_cached_engine(data_root: &Path) -> Option<BrowserEngine> {
    let engine = probe_engine(
        BrowserEngineKind::Chromium,
        managed_engine_path(data_root),
        None,
    )
    .await?;
    (engine.version == CHROMIUM_VERSION).then_some(engine)
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

    if let Some(engine) = detect_cached_engine(data_root).await {
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
        extract_archive(&archive_for_extract, &staging_for_extract)
    })
    .await
    .map_err(|error| unavailable(format!("Chromium extraction worker failed: {error}")))?
    .map_err(|error| unavailable(format!("Chromium archive extraction failed: {error}")))?;

    let staged_engine = staging.join("chrome.exe");
    let staged_engine = probe_engine(BrowserEngineKind::Chromium, staged_engine, None)
        .await
        .ok_or_else(|| unavailable("downloaded Chromium failed its startup probe"))?;
    if staged_engine.version != CHROMIUM_VERSION {
        return Err(unavailable(
            "downloaded Chromium version does not match the pinned release",
        ));
    }
    replace_cache(root, staging)?;
    detect_cached_engine(
        root.parent()
            .and_then(Path::parent)
            .ok_or_else(|| unavailable("installed Chromium path has no data root"))?,
    )
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
    file.flush()
        .await
        .map_err(|error| unavailable(format!("failed to flush Chromium archive: {error}")))?;
    file.sync_all()
        .await
        .map_err(|error| unavailable(format!("failed to sync Chromium archive: {error}")))?;
    if downloaded != CHROMIUM_ARCHIVE_SIZE {
        return Err(unavailable(
            "Chromium download ended before the pinned size",
        ));
    }
    let archive_sha256 = format!("{:x}", hasher.finalize());
    if archive_sha256 != CHROMIUM_ARCHIVE_SHA256 {
        return Err(unavailable(
            "Chromium download digest does not match the pinned package",
        ));
    }
    tracing::info!(
        target: "iyw_claw_browser",
        archive_bytes = downloaded,
        archive_sha256 = %archive_sha256,
        "managed Chromium archive downloaded"
    );
    Ok(())
}

fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|error| error.to_string())?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("Chromium archive contains too many entries".to_string());
    }
    let mut extracted = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(path) = entry.enclosed_name() else {
            return Err("Chromium archive contains an unsafe path".to_string());
        };
        let mut components = path.components();
        match components.next() {
            Some(Component::Normal(root)) if root == OsStr::new("chrome-win64") => {}
            _ => return Err("Chromium archive has an unexpected root directory".to_string()),
        }
        let relative = components.collect::<PathBuf>();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
            continue;
        }
        if !entry.is_file() {
            return Err("Chromium archive contains an unsupported entry".to_string());
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let remaining = MAX_EXTRACTED_BYTES.saturating_sub(extracted);
        let mut writer = File::create(&output).map_err(|error| error.to_string())?;
        let written = io::copy(
            &mut entry.by_ref().take(remaining.saturating_add(1)),
            &mut writer,
        )
        .map_err(|error| error.to_string())?;
        if written > remaining {
            return Err("Chromium archive exceeds the extraction size limit".to_string());
        }
        extracted = extracted.saturating_add(written);
        writer.flush().map_err(|error| error.to_string())?;
    }
    if !destination.join("chrome.exe").is_file() {
        return Err("Chromium archive does not contain chrome.exe".to_string());
    }
    Ok(())
}

fn replace_cache(root: &Path, staging: &Path) -> Result<(), BrowserError> {
    let parent = root
        .parent()
        .ok_or_else(|| unavailable("managed Chromium path has no parent"))?;
    let backup = parent.join(".chromium-previous");
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(|error| {
            unavailable(format!("failed to remove old Chromium backup: {error}"))
        })?;
    }
    if root.exists() {
        fs::rename(root, &backup)
            .map_err(|error| unavailable(format!("failed to stage existing Chromium: {error}")))?;
    }
    if let Err(error) = fs::rename(staging, root) {
        if backup.exists() {
            let _ = fs::rename(&backup, root);
        }
        return Err(unavailable(format!("failed to activate Chromium: {error}")));
    }
    if backup.exists() {
        if let Err(error) = fs::remove_dir_all(backup) {
            tracing::warn!(
                target: "iyw_claw_browser",
                error = %error,
                "old Chromium backup cleanup failed after activation"
            );
        }
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
