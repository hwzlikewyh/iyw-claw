use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

const EXTENSION_VERSION: &str = "1.0.24";
const ARCHIVE: &[u8] = include_bytes!("../../../resources/opencli/opencli-extension-v1.0.24.zip");
const LICENSE: &[u8] = include_bytes!("../../../resources/opencli/LICENSE");
const ARCHIVE_SHA256: &str = "bad9163f32a66224404e302f52f35391672341d2bd0bf30c8bbaae3c6e6e246c";

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn internet_tools_open_extension_settings(browser: String) -> Result<(), String> {
    let result = crate::browser::open_extension_settings(&browser).await;
    if let Err(error) = &result {
        tracing::warn!(browser = %browser, error = %error, "[internet-tools] Could not open extension settings");
    }
    result
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn internet_tools_prepare_extension() -> Result<String, String> {
    let _guard = super::bootstrap_lock().lock().await;
    let directory = super::commands::active_paths()?
        .root()
        .join("browser-extensions/opencli")
        .join(EXTENSION_VERSION);
    tracing::info!(
        version = EXTENSION_VERSION,
        "[internet-tools] Preparing bundled browser extension"
    );
    let result = tokio::task::spawn_blocking(move || {
        prepare_archive(&directory)?;
        Ok::<_, String>(directory.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?;
    match &result {
        Ok(_) => tracing::info!(
            version = EXTENSION_VERSION,
            "[internet-tools] Browser extension prepared"
        ),
        Err(error) => {
            tracing::warn!(error = %error, "[internet-tools] Browser extension preparation failed")
        }
    }
    result
}

fn prepare_archive(directory: &Path) -> Result<(), String> {
    if format!("{:x}", Sha256::digest(ARCHIVE)) != ARCHIVE_SHA256 {
        return Err("Bundled OpenCLI extension checksum mismatch".into());
    }
    ensure_directory(directory)?;
    let mut archive =
        zip::ZipArchive::new(Cursor::new(ARCHIVE)).map_err(|error| error.to_string())?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or("Invalid bundled extension path")?;
        let path = directory.join(relative);
        if entry.is_dir() {
            ensure_directory(&path)?;
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        write_asset(&path, &bytes)?;
    }
    write_asset(&directory.join("LICENSE"), LICENSE)
}

fn ensure_directory(directory: &Path) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(directory) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Extension directory must be a regular directory".into());
        }
        return Ok(());
    }
    if let Some(parent) = directory.parent() {
        ensure_directory(parent)?;
    }
    fs::create_dir(directory).map_err(|error| error.to_string())
}

fn write_asset(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid extension asset path")?;
    ensure_directory(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("Extension asset must be a regular file".into());
        }
    }
    if fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}
