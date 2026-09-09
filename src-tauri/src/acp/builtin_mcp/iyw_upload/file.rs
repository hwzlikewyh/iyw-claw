use std::path::Path;

use reqwest::header::HeaderValue;
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::AsyncReadExt;

use super::{error, MAX_FILE_BYTES};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadRequest {
    path: String,
    description: String,
    name: Option<String>,
    mime_type: Option<String>,
}

pub(super) struct UploadFile {
    pub(super) bytes: Vec<u8>,
    pub(super) name: String,
    pub(super) mime_type: String,
    pub(super) category: &'static str,
}

pub(super) async fn prepare(cwd: &Path, arguments: Value) -> Result<UploadFile, ErrorData> {
    let request: UploadRequest = serde_json::from_value(arguments)
        .map_err(|_| invalid("Invalid upload fields; follow the upload_iyw_file input schema"))?;
    super::super::iyw_progress::validate(Some(&request.description))?;
    if request.path.trim().is_empty() {
        return Err(invalid("path must be non-empty"));
    }
    let root = tokio::fs::canonicalize(cwd)
        .await
        .map_err(|_| invalid("Current workspace is unavailable"))?;
    let path = tokio::fs::canonicalize(cwd.join(&request.path))
        .await
        .map_err(|_| invalid("Upload file is unavailable"))?;
    if !path.starts_with(&root) {
        return Err(invalid(
            "Upload path must stay inside the current workspace",
        ));
    }
    let name = request.name.unwrap_or_else(|| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    });
    validate_name(&name)?;
    let mime_type = request
        .mime_type
        .unwrap_or_else(|| "application/octet-stream".into());
    validate_mime(&mime_type)?;
    let bytes = read_file(&path).await?;
    Ok(UploadFile {
        bytes,
        name,
        mime_type,
        category: "files",
    })
}

async fn read_file(path: &Path) -> Result<Vec<u8>, ErrorData> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| invalid("Upload file metadata is unavailable"))?;
    if !metadata.is_file() {
        return Err(invalid(
            "Upload requires a regular file; archive directories first",
        ));
    }
    validate_size(metadata.len())?;
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| invalid("Upload file cannot be opened"))?;
    let opened = file
        .metadata()
        .await
        .map_err(|_| invalid("Upload file metadata is unavailable"))?;
    if !opened.is_file() {
        return Err(invalid("Upload requires a regular file"));
    }
    validate_size(opened.len())?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| invalid("Upload file cannot be read"))?;
    validate_size(bytes.len() as u64)?;
    Ok(bytes)
}

pub(super) fn validate_size(size: u64) -> Result<(), ErrorData> {
    if size > MAX_FILE_BYTES {
        return Err(error(
            "File exceeds the 50 MiB (52,428,800 bytes) limit",
            "file_too_large",
            "not_started",
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), ErrorData> {
    if name.trim().is_empty()
        || name.chars().count() > 255
        || matches!(name, "." | "..")
        || name
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':'))
    {
        return Err(invalid("name must be a filename of 1 to 255 characters without directories or control characters"));
    }
    Ok(())
}

fn validate_mime(mime: &str) -> Result<(), ErrorData> {
    let valid = reqwest::multipart::Part::bytes(Vec::new())
        .mime_str(mime)
        .is_ok()
        && HeaderValue::from_str(mime).is_ok();
    if !valid {
        return Err(invalid("mime_type must be a valid MIME type"));
    }
    Ok(())
}

fn invalid(message: &'static str) -> ErrorData {
    error(message, "invalid_request", "not_started")
}
