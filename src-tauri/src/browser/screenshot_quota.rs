use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::error::{BrowserError, BrowserErrorCode};

const MAX_SCREENSHOT_FILES: usize = 20;
const MAX_SCREENSHOT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_SINGLE_SCREENSHOT_BYTES: u64 = 25 * 1024 * 1024;

struct ScreenshotFile {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

pub(super) async fn enforce_screenshot_quota(path: &Path) -> Result<(), BrowserError> {
    let mut files = read_screenshots(path).await?;
    files.sort_by_key(|file| file.modified);
    let oversized = files
        .iter()
        .filter(|file| file.bytes > MAX_SINGLE_SCREENSHOT_BYTES)
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    for path in &oversized {
        let _ = tokio::fs::remove_file(path).await;
    }
    if !oversized.is_empty() {
        return Err(quota_error());
    }
    let mut total = files.iter().map(|file| file.bytes).sum::<u64>();
    let mut count = files.len();
    for file in files {
        if count <= MAX_SCREENSHOT_FILES && total <= MAX_SCREENSHOT_BYTES {
            break;
        }
        tokio::fs::remove_file(&file.path)
            .await
            .map_err(|_| quota_error())?;
        count = count.saturating_sub(1);
        total = total.saturating_sub(file.bytes);
    }
    Ok(())
}

async fn read_screenshots(path: &Path) -> Result<Vec<ScreenshotFile>, BrowserError> {
    let mut entries = tokio::fs::read_dir(path).await.map_err(|_| quota_error())?;
    let mut files = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|_| quota_error())? {
        let metadata = entry.metadata().await.map_err(|_| quota_error())?;
        if metadata.is_file() {
            files.push(ScreenshotFile {
                path: entry.path(),
                bytes: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
    }
    Ok(files)
}

fn quota_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The browser screenshot storage limit could not be enforced",
    )
}
