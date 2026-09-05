use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};

use zip::ZipArchive;

use super::super::error::{BrowserError, BrowserErrorCode};

const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;

pub(super) fn extract(archive_path: &Path, destination: &Path) -> Result<(), String> {
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

pub(super) fn replace_cache(root: &Path, staging: &Path) -> Result<(), BrowserError> {
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

fn unavailable(message: impl Into<String>) -> BrowserError {
    BrowserError::new(BrowserErrorCode::BrowserRuntimeUnavailable, message).retryable(true)
}
