use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

pub(super) async fn read_checked(path: &Path, limit: Option<u64>) -> Result<Vec<u8>, &'static str> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| "FILE_NOT_READABLE")?;
    let metadata = file.metadata().await.map_err(|_| "FILE_NOT_READABLE")?;
    if !metadata.is_file() {
        return Err("FILE_NOT_READABLE");
    }
    if limit.is_some_and(|value| metadata.len() > value) {
        return Err("FILE_TOO_LARGE");
    }
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(usize::MAX)
        .min(1024 * 1024);
    let mut content = Vec::with_capacity(capacity);
    match limit {
        Some(value) => {
            file.take(value.saturating_add(1))
                .read_to_end(&mut content)
                .await
        }
        None => file.read_to_end(&mut content).await,
    }
    .map_err(|_| "FILE_NOT_READABLE")?;
    if content.is_empty() {
        return Err("FILE_EMPTY");
    }
    if limit.is_some_and(|value| content.len() as u64 > value) {
        return Err("FILE_TOO_LARGE");
    }
    Ok(content)
}

pub(super) fn resolved_path(value: &str, working_dir: &Path) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        working_dir.join(path)
    }
}

pub(super) fn safe_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string()
}

pub(super) fn mime_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "pdf" => "application/pdf",
        "txt" | "md" | "log" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}
