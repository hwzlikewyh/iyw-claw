use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncWriteExt;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::market_types::{Document, OptionSpec};
use crate::app_error::AppCommandError;
use crate::commands::mcp_install::progress;

pub(super) const MAX_FILES: usize = 2048;

pub(super) async fn install(
    conn: &sea_orm::DatabaseConnection,
    input: (&Document, &OptionSpec, &Value),
) -> Result<PathBuf, AppCommandError> {
    let (doc, option, metadata) = input;
    progress::stage("checking_cache")?;
    let digest = &option.artifact_id;
    if digest.len() != 64
        || !digest.bytes().all(|value| value.is_ascii_hexdigit())
        || metadata["sha256"].as_str() != Some(digest.as_str())
    {
        return Err(AppCommandError::configuration_invalid("连接器制品摘要无效"));
    }
    let root = crate::system_skills::data_dir_from_env()
        .join("connectors")
        .join("artifacts");
    let destination = root.join(digest);
    let cache_path = destination.clone();
    let cache_digest = digest.clone();
    let valid =
        tokio::task::spawn_blocking(move || super::market_cache::valid(&cache_path, &cache_digest))
            .await
            .map_err(|_| AppCommandError::io_error("连接器文件校验任务失败"))?;
    if valid {
        return Ok(destination);
    }
    let file = progress::cancellable(download(conn, (doc, digest, metadata))).await?;
    let filename = metadata["filename"]
        .as_str()
        .unwrap_or("server")
        .to_string();
    let digest = digest.to_string();
    progress::stage("extracting")?;
    tokio::task::spawn_blocking(move || activate(&root, &digest, (&filename, file)))
        .await
        .map_err(|_| AppCommandError::io_error("连接器文件安装任务失败"))?
}

fn activate(
    root: &Path,
    digest: &str,
    source: (&str, tempfile::NamedTempFile),
) -> Result<PathBuf, AppCommandError> {
    std::fs::create_dir_all(root).map_err(storage_error)?;
    let staging = root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging).map_err(storage_error)?;
    let result =
        write_artifact(&staging, source).and_then(|_| super::market_cache::write(&staging, digest));
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    let destination = root.join(digest);
    if let Err(error) = std::fs::rename(&staging, &destination) {
        let _ = std::fs::remove_dir_all(&staging);
        if !super::market_cache::valid(&destination, digest) {
            return Err(storage_error(error));
        }
    }
    Ok(destination)
}

fn write_artifact(
    staging: &Path,
    source: (&str, tempfile::NamedTempFile),
) -> Result<(), AppCommandError> {
    if !source.0.to_ascii_lowercase().ends_with(".zip") {
        let filename = Path::new(source.0)
            .file_name()
            .filter(|name| !name.to_string_lossy().eq_ignore_ascii_case(".iyw-artifact"))
            .ok_or_else(|| AppCommandError::configuration_invalid("连接器文件名无效"))?;
        let path = staging.join(filename);
        std::fs::copy(source.1.path(), &path).map_err(storage_error)?;
        return executable(&path);
    }
    let mut archive = zip::ZipArchive::new(source.1.reopen().map_err(storage_error)?)
        .map_err(|_| AppCommandError::configuration_invalid("连接器 ZIP 格式无效"))?;
    if archive.len() > MAX_FILES {
        return Err(AppCommandError::invalid_input("连接器文件数量超过限制"));
    }
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| AppCommandError::configuration_invalid("无法读取连接器 ZIP"))?;
        let raw = entry.name().to_string();
        if raw.contains('\\')
            || raw.contains(':')
            || raw.contains('\0')
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(AppCommandError::configuration_invalid(
                "连接器 ZIP 含不安全路径或符号链接",
            ));
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| AppCommandError::configuration_invalid("连接器 ZIP 路径越界"))?;
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
            || raw.eq_ignore_ascii_case(".iyw-artifact")
        {
            return Err(AppCommandError::configuration_invalid(
                "连接器 ZIP 路径无效",
            ));
        }
        let target = staging.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(storage_error)?;
            continue;
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or_else(|| AppCommandError::invalid_input("连接器解压大小无效"))?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(storage_error)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(storage_error)?;
        let expected = entry.size();
        let read_limit = expected
            .checked_add(1)
            .ok_or_else(|| AppCommandError::invalid_input("连接器文件大小无效"))?;
        let count = std::io::copy(&mut entry.by_ref().take(read_limit), &mut file)
            .map_err(storage_error)?;
        if count != entry.size() {
            return Err(AppCommandError::configuration_invalid(
                "连接器 ZIP 文件大小不匹配",
            ));
        }
        if entry.unix_mode().is_some_and(|mode| mode & 0o111 != 0) {
            executable(&target)?;
        }
    }
    Ok(())
}

fn executable(path: &Path) -> Result<(), AppCommandError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(storage_error)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn storage_error(error: std::io::Error) -> AppCommandError {
    tracing::warn!(kind=?error.kind(), os_code=error.raw_os_error(), "[MCP] 写入连接器受管文件失败");
    AppCommandError::io_error("无法写入连接器受管文件目录")
}

async fn download(
    conn: &sea_orm::DatabaseConnection,
    input: (&Document, &str, &Value),
) -> Result<tempfile::NamedTempFile, AppCommandError> {
    let (doc, digest, metadata) = input;
    let expected = metadata["size"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| AppCommandError::configuration_invalid("连接器制品大小声明无效"))?;
    let response = super::market_artifact_ticket::download(conn, (doc, digest, metadata)).await?;
    stream_artifact(response, expected, digest).await
}

async fn stream_artifact(
    mut response: reqwest::Response,
    expected: u64,
    digest: &str,
) -> Result<tempfile::NamedTempFile, AppCommandError> {
    if response
        .content_length()
        .is_some_and(|size| size != expected)
    {
        return Err(AppCommandError::network("连接器文件大小与声明不一致"));
    }
    let file = tempfile::NamedTempFile::new().map_err(storage_error)?;
    let mut output = tokio::fs::File::from_std(file.reopen().map_err(storage_error)?);
    let mut received = 0_u64;
    let mut hasher = Sha256::new();
    progress::download(0, Some(expected))?;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppCommandError::network("连接器文件下载中断"))?
    {
        received = received
            .checked_add(chunk.len() as u64)
            .filter(|value| *value <= expected)
            .ok_or_else(|| AppCommandError::network("连接器文件超过声明大小"))?;
        output.write_all(&chunk).await.map_err(storage_error)?;
        hasher.update(&chunk);
        progress::download(received, Some(expected))?;
    }
    output.flush().await.map_err(storage_error)?;
    output.sync_all().await.map_err(storage_error)?;
    drop(output);
    if received != expected || format!("{:x}", hasher.finalize()) != digest {
        return Err(AppCommandError::configuration_invalid(
            "连接器文件长度或摘要校验失败",
        ));
    }
    Ok(file)
}
