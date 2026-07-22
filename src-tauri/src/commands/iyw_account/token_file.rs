use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::app_error::AppCommandError;

use super::IywAccountToken;

const TOKEN_FILE_NAME: &str = "iyw-account-token.json";

pub(super) fn sync(token: Option<&IywAccountToken>) -> Result<(), AppCommandError> {
    let path = crate::paths::iyw_claw_home_dir().join(TOKEN_FILE_NAME);
    let action = if token.is_some() { "write" } else { "remove" };
    tracing::debug!(action, path = %path.display(), "Syncing iyw account token file");
    match sync_at(&path, token) {
        Ok(()) => {
            tracing::info!(action, path = %path.display(), "Synchronized iyw account token file");
            Ok(())
        }
        Err(error) => {
            tracing::error!(
                action,
                path = %path.display(),
                error = %error,
                detail = ?error.detail,
                "Failed to synchronize iyw account token file"
            );
            Err(error)
        }
    }
}

fn sync_at(path: &Path, token: Option<&IywAccountToken>) -> Result<(), AppCommandError> {
    match token {
        Some(token) => save_to(path, token),
        None => remove_at(path),
    }
}

fn save_to(path: &Path, token: &IywAccountToken) -> Result<(), AppCommandError> {
    let bytes = serde_json::to_vec_pretty(token).map_err(|error| {
        AppCommandError::configuration_invalid("Failed to serialize iyw account token")
            .with_detail(error.to_string())
    })?;
    let parent = path.parent().ok_or_else(|| {
        AppCommandError::invalid_input("iyw account token path has no parent directory")
    })?;
    fs::create_dir_all(parent).map_err(AppCommandError::io)?;

    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("iyw-account-token.json");
    let temp_path = parent.join(format!(
        ".{file_name}.{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let result = write_and_replace(&temp_path, path, &bytes);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn remove_at(path: &Path) -> Result<(), AppCommandError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppCommandError::io(error)),
    }
}

fn write_and_replace(
    temp_path: &Path,
    target_path: &Path,
    bytes: &[u8],
) -> Result<(), AppCommandError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut temp = options.open(temp_path).map_err(AppCommandError::io)?;
    temp.write_all(bytes).map_err(AppCommandError::io)?;
    temp.write_all(b"\n").map_err(AppCommandError::io)?;
    temp.sync_all().map_err(AppCommandError::io)?;
    replace_file(temp_path, target_path)?;
    sync_directory(target_path.parent().unwrap_or_else(|| Path::new("")))
}

#[cfg(unix)]
fn replace_file(temp_path: &Path, target_path: &Path) -> Result<(), AppCommandError> {
    fs::rename(temp_path, target_path).map_err(AppCommandError::io)
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, target_path: &Path) -> Result<(), AppCommandError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let source = wide(temp_path);
    let destination = wide(target_path);
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(AppCommandError::io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn replace_file(temp_path: &Path, target_path: &Path) -> Result<(), AppCommandError> {
    fs::rename(temp_path, target_path).map_err(AppCommandError::io)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), AppCommandError> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(AppCommandError::io)
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), AppCommandError> {
    Ok(())
}
