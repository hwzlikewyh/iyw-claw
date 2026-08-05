use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::path::Path;

use crate::app_error::AppCommandError;

/// Open an existing absolute path with its system-default application.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn open_local_path(path: String) -> Result<(), AppCommandError> {
    let normalized = path.trim();
    if normalized.is_empty() || normalized.contains('\0') {
        return Err(AppCommandError::invalid_input("Invalid local path"));
    }
    let target = Path::new(normalized);
    if !target.is_absolute() {
        return Err(AppCommandError::invalid_input(
            "Local path must be absolute",
        ));
    }
    let metadata = std::fs::metadata(target).map_err(|error| {
        AppCommandError::invalid_input("Local path is unavailable").with_detail(error.to_string())
    })?;
    tauri_plugin_opener::open_path(target, None::<&str>).map_err(|error| {
        AppCommandError::external_command("Could not open the local path", error.to_string())
    })?;
    tracing::info!(
        target_kind = if metadata.is_dir() {
            "directory"
        } else {
            "file"
        },
        "[local-path] opened with default application"
    );
    Ok(())
}

/// Write a base64-encoded binary blob to a user-chosen path on disk.
///
/// Used by the frontend's "download generated image" flow on desktop:
/// the renderer first invokes `tauri-plugin-dialog`'s `save()` to obtain
/// a destination path from the system save dialog, then calls this command
/// with the base64 payload. Web mode bypasses this command entirely and
/// uses an `<a download>` Blob link.
///
/// `path` must be an absolute filesystem path (the dialog returns one).
/// Parent directory must already exist (it does, since the OS dialog only
/// lets the user pick an existing folder + filename).
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn save_binary_file(path: String, data_base64: String) -> Result<(), AppCommandError> {
    let bytes = STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| AppCommandError::invalid_input(format!("invalid base64 payload: {e}")))?;
    std::fs::write(&path, bytes).map_err(AppCommandError::io)?;
    Ok(())
}

/// Write a UTF-8 text payload to a user-chosen path on disk.
///
/// Used by the frontend's "export conversation as Markdown / HTML" flow on
/// desktop. The renderer first invokes `tauri-plugin-dialog`'s `save()`
/// to obtain a destination path from the system save dialog, then calls
/// this command with the text contents. Web mode bypasses this command
/// entirely and uses an `<a download>` Blob link.
///
/// Mirrors `save_binary_file`'s contract: `path` must be an absolute
/// filesystem path returned by the OS dialog, and the parent directory
/// is guaranteed to exist by the dialog. I/O failures (including macOS
/// TCC denials at write time) surface through `AppCommandError::io`,
/// which maps `PermissionDenied` so the caller can disambiguate.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn save_text_file(path: String, contents: String) -> Result<(), AppCommandError> {
    std::fs::write(&path, contents).map_err(AppCommandError::io)?;
    Ok(())
}
