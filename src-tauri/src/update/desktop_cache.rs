//! 桌面更新包的持久缓存；只发布完整包，读取时重新验证签名。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tauri::Manager;
use tauri_plugin_updater::Update;
use tokio::io::AsyncWriteExt;

use super::{release, verify};

fn cache_path(app: &tauri::AppHandle, update: &Update) -> Result<PathBuf, String> {
    let extensions = release::extensions(update).map_err(|error| error.to_string())?;
    let identity = serde_json::to_vec(&(
        &update.version,
        &update.target,
        &update.signature,
        extensions.channel,
        extensions.release_id,
    ))
    .map_err(|error| error.to_string())?;
    let key = format!("{:x}", Sha256::digest(identity));
    Ok(app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("app-updates")
        .join(format!("{key}.package")))
}

pub async fn load(app: &tauri::AppHandle, update: &Update) -> Result<Option<Vec<u8>>, String> {
    let path = cache_path(app, update)?;
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Failed to read update download cache: {error}")),
    };
    let update = update.clone();
    tokio::task::spawn_blocking(move || match validate(&update, &bytes) {
        Ok(()) => {
            tracing::info!(version = %update.version, "[app-update] reusing verified download cache");
            Some(bytes)
        }
        Err(error) => {
            tracing::warn!(version = %update.version, %error, "[app-update] invalid download cache; downloading again");
            None
        }
    })
    .await
    .map_err(|error| error.to_string())
}

pub async fn store(app: &tauri::AppHandle, update: &Update, bytes: &[u8]) -> Result<(), String> {
    validate(update, bytes)?;
    let path = cache_path(app, update)?;
    let directory = path.parent().ok_or("Invalid update cache directory")?;
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|error| format!("Failed to create update download cache: {error}"))?;
    persist(directory, &path, bytes)
        .await
        .map_err(|error| format!("Failed to save update download cache: {error}"))?;
    tracing::info!(version = %update.version, size = bytes.len(), "[app-update] saved verified download cache");
    Ok(())
}

async fn persist(directory: &Path, path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let temporary = tempfile::NamedTempFile::new_in(directory)?;
    let mut file = tokio::fs::File::from_std(temporary.reopen()?);
    file.write_all(bytes).await?;
    file.sync_all().await?;
    drop(file);
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn validate(update: &Update, bytes: &[u8]) -> Result<(), String> {
    verify::verify_release_signature(bytes, &update.signature)?;
    let extensions = release::extensions(update).map_err(|error| error.to_string())?;
    if extensions
        .size
        .is_some_and(|size| size != bytes.len() as u64)
    {
        return Err("Update download size does not match the release".to_string());
    }
    if let Some(expected) = extensions.sha256 {
        let actual = format!("{:x}", Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            return Err("Update download checksum does not match the release".to_string());
        }
    }
    Ok(())
}
