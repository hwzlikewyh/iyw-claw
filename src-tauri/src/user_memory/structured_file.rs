use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::app_error::AppCommandError;

use super::fs;
use super::helpers::reject_symlink;
use super::platform::{open_read_no_follow, replace_file, sync_directory};

#[derive(Debug, thiserror::Error)]
pub(super) enum StructuredReadError {
    #[error("source is missing")]
    Missing,
    #[error("source is invalid: {0}")]
    Invalid(String),
    #[error("source I/O failed: {0}")]
    Io(#[source] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstallNewResult {
    Installed,
    AlreadyExists,
}

pub(super) fn read_bounded_utf8(
    path: &Path,
    max_chars: usize,
) -> Result<String, StructuredReadError> {
    validate_source_metadata(path)?;
    let file = open_read_no_follow(path).map_err(classify_open_error)?;
    if !file.metadata().map_err(StructuredReadError::Io)?.is_file() {
        return Err(StructuredReadError::Invalid("not a regular file".into()));
    }
    let byte_limit = max_chars.saturating_mul(4).saturating_add(1);
    let mut bytes = Vec::new();
    file.take(byte_limit as u64)
        .read_to_end(&mut bytes)
        .map_err(StructuredReadError::Io)?;
    let content = String::from_utf8(bytes)
        .map_err(|_| StructuredReadError::Invalid("not valid UTF-8".into()))?;
    if content.chars().count() > max_chars {
        return Err(StructuredReadError::Invalid("exceeds size limit".into()));
    }
    Ok(content)
}

pub(super) fn read_json_optional<T: DeserializeOwned>(
    root: &Path,
    file_name: &str,
    max_chars: usize,
) -> Result<Option<T>, AppCommandError> {
    let path = safe_child(root, file_name)?;
    match read_bounded_utf8(&path, max_chars) {
        Ok(content) => serde_json::from_str(&content).map(Some).map_err(|error| {
            AppCommandError::configuration_invalid("User memory structured state is invalid")
                .with_detail(error.to_string())
        }),
        Err(StructuredReadError::Missing) => Ok(None),
        Err(error) => Err(AppCommandError::configuration_invalid(
            "User memory structured state cannot be read",
        )
        .with_detail(error.to_string())),
    }
}

pub(super) fn write_json_atomic<T: Serialize>(
    root: &Path,
    file_name: &str,
    value: &T,
) -> Result<(), AppCommandError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        AppCommandError::configuration_invalid("Serialize user memory structured state")
            .with_detail(error.to_string())
    })?;
    let target = safe_child(root, file_name)?;
    let temp = write_private_temp(&target, &bytes)?;
    let result = replace_file(&temp, &target).and_then(|_| sync_directory(root));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

pub(super) fn ensure_writable_optional(
    root: &Path,
    file_name: &str,
) -> Result<(), AppCommandError> {
    let path = safe_child(root, file_name)?;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppCommandError::io(error)),
    };
    if !metadata.is_file() {
        return Err(AppCommandError::configuration_invalid(
            "User memory structured state is not a regular file",
        ));
    }
    if metadata.permissions().readonly() {
        return Err(AppCommandError::permission_denied(
            "User memory structured state is read-only",
        ));
    }
    Ok(())
}

pub(super) fn install_new_private(
    root: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<InstallNewResult, AppCommandError> {
    let target = safe_child(root, file_name)?;
    let temp = write_private_temp(&target, bytes)?;
    let result = match std::fs::hard_link(&temp, &target) {
        Ok(()) => sync_directory(root).map(|_| InstallNewResult::Installed),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(InstallNewResult::AlreadyExists)
        }
        Err(error) => Err(AppCommandError::io(error)),
    };
    let _ = std::fs::remove_file(&temp);
    result
}

fn validate_source_metadata(path: &Path) -> Result<(), StructuredReadError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(StructuredReadError::Missing)
        }
        Err(error) => return Err(StructuredReadError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(StructuredReadError::Invalid(
            "symlink or non-regular file".into(),
        ));
    }
    Ok(())
}

fn classify_open_error(error: std::io::Error) -> StructuredReadError {
    if error.kind() == std::io::ErrorKind::NotFound {
        StructuredReadError::Missing
    } else {
        StructuredReadError::Io(error)
    }
}

fn safe_child(root: &Path, file_name: &str) -> Result<PathBuf, AppCommandError> {
    fs::ensure_safe_root(root)?;
    let candidate = Path::new(file_name);
    if candidate.file_name() != Some(candidate.as_os_str()) {
        return Err(AppCommandError::invalid_input(
            "User memory structured filename is invalid",
        ));
    }
    let path = root.join(candidate);
    reject_symlink(&path)?;
    Ok(path)
}

fn write_private_temp(target: &Path, bytes: &[u8]) -> Result<PathBuf, AppCommandError> {
    let temp = temporary_path(target);
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp).map_err(AppCommandError::io)?;
        file.write_all(bytes).map_err(AppCommandError::io)?;
        file.flush().map_err(AppCommandError::io)?;
        file.sync_all().map_err(AppCommandError::io)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    Ok(temp)
}

fn temporary_path(target: &Path) -> PathBuf {
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    target.with_file_name(format!(
        "{name}.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ))
}
