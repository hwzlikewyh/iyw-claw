use std::fs;
use std::path::Path;

use uuid::Uuid;

use super::error::{BrowserError, BrowserErrorCode};
use super::process::executable_is_running;

const MAX_PROFILE_COPY_BYTES: u64 = 512 * 1024 * 1024;

pub(super) async fn seed(
    target: &Path,
    source: &Path,
    browser_executable: &Path,
) -> Result<(), BrowserError> {
    if !source.is_dir()
        || executable_is_running(browser_executable)
        || source_profile_is_busy(source)
        || !profile_is_empty(target)
    {
        return Ok(());
    }
    let parent = target.parent().ok_or_else(profile_error)?;
    let staging = parent.join(format!(".profile-seed-{}", Uuid::new_v4().simple()));
    let source = source.to_path_buf();
    let staging_for_copy = staging.clone();
    let copy_result = tokio::task::spawn_blocking(move || {
        let mut copied = 0;
        copy_profile_tree(&source, &staging_for_copy, &mut copied)
    })
    .await
    .map_err(|error| profile_error_with_detail(std::io::Error::other(error)));
    if let Err(error) = copy_result.and_then(|result| result.map_err(profile_error_with_detail)) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = install_seeded_profile(target, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(())
}

fn profile_is_empty(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|entries| entries.flatten().next().is_none())
        .unwrap_or(false)
}

fn source_profile_is_busy(path: &Path) -> bool {
    ["SingletonLock", "SingletonCookie", "SingletonSocket"]
        .iter()
        .any(|name| path.join(name).exists())
}

fn copy_profile_tree(source: &Path, destination: &Path, copied: &mut u64) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if should_skip_entry(&name) {
            continue;
        }
        let from = entry.path();
        let to = destination.join(name);
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(std::io::Error::other("profile contains a symbolic link"));
        }
        if file_type.is_dir() {
            copy_profile_tree(&from, &to, copied)?;
        } else if file_type.is_file() {
            let size = fs::metadata(&from)?.len();
            if (*copied).saturating_add(size) > MAX_PROFILE_COPY_BYTES {
                return Err(std::io::Error::other(
                    "browser profile exceeds the seed size limit",
                ));
            }
            fs::copy(from, to)?;
            *copied = (*copied).saturating_add(size);
        }
    }
    fs::File::create(destination.join(".iyw-claw-profile-seeded"))?.sync_all()?;
    Ok(())
}

fn should_skip_entry(name: &std::ffi::OsStr) -> bool {
    let value = name.to_string_lossy();
    value.starts_with("Singleton")
        || matches!(
            value.as_ref(),
            "LOCK"
                | "Lockfile"
                | "DevToolsActivePort"
                | "Crashpad"
                | "Cache"
                | "Code Cache"
                | "GPUCache"
                | "ShaderCache"
                | "DawnCache"
                | "Extensions"
                | "Extension State"
                | "Local Extension Settings"
                | "Sync Extension Settings"
        )
}

fn install_seeded_profile(target: &Path, staging: &Path) -> Result<(), BrowserError> {
    let parent = target.parent().ok_or_else(profile_error)?;
    let backup = parent.join(".profile-v1-before-seed");
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(|_| profile_error())?;
    }
    fs::rename(target, &backup).map_err(|_| profile_error())?;
    if let Err(error) = fs::rename(staging, target) {
        let _ = fs::rename(&backup, target);
        return Err(profile_error_with_detail(error));
    }
    let _ = fs::remove_dir_all(backup);
    Ok(())
}

fn profile_error() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "The browser profile could not be prepared",
    )
    .retryable(true)
}

fn profile_error_with_detail(error: std::io::Error) -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        format!("The browser profile could not be activated: {error}"),
    )
    .retryable(true)
}
