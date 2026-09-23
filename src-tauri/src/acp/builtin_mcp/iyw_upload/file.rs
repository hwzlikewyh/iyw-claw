use std::path::Path;

use reqwest::header::HeaderValue;
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

use super::{error, MAX_FILE_BYTES};

const UPLOAD_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadRequest {
    path: String,
    description: String,
    name: Option<String>,
    mime_type: Option<String>,
}

pub(super) struct UploadFile {
    pub(super) body: reqwest::Body,
    pub(super) size_bytes: u64,
    pub(super) name: String,
    pub(super) mime_type: String,
    pub(super) category: &'static str,
}

pub(super) async fn prepare(cwd: &Path, arguments: Value) -> Result<UploadFile, ErrorData> {
    let request: UploadRequest = serde_json::from_value(arguments).map_err(|_| {
        invalid("Upload requires path and description as strings; name and mime_type are optional strings. Pass fields directly, without an arguments wrapper or extra fields.")
    })?;
    super::super::iyw_progress::validate(Some(&request.description))?;
    if request.path.trim().is_empty() {
        return Err(invalid("path must be non-empty"));
    }
    let path = tokio::fs::canonicalize(cwd.join(&request.path))
        .await
        .map_err(|cause| file_error("Resolve upload file", cause))?;
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
    let (body, size_bytes) = open_file(&path).await?;
    Ok(UploadFile {
        body,
        size_bytes,
        name,
        mime_type,
        category: "files",
    })
}

async fn open_file(path: &Path) -> Result<(reqwest::Body, u64), ErrorData> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|cause| file_error("Read upload file metadata", cause))?;
    if !metadata.is_file() {
        return Err(invalid(
            "Upload requires a regular file; archive directories first",
        ));
    }
    validate_size(metadata.len())?;
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|cause| file_error("Open upload file", cause))?;
    let opened = file
        .metadata()
        .await
        .map_err(|cause| file_error("Read opened file metadata", cause))?;
    if !opened.is_file() {
        return Err(invalid("Upload requires a regular file"));
    }
    let size = opened.len();
    validate_size(size)?;
    // 持有已校验句柄并限制读取长度，避免整文件入内存或追加内容越过上限。
    let stream = ReaderStream::with_capacity(file.take(size), UPLOAD_BUFFER_BYTES);
    Ok((reqwest::Body::wrap_stream(stream), size))
}

pub(super) fn validate_size(size: u64) -> Result<(), ErrorData> {
    if size > MAX_FILE_BYTES {
        return Err(error(
            "File exceeds the 1 GiB (1,073,741,824 bytes) limit",
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
        return Err(invalid("name must be a filename of 1 to 255 characters; Chinese and spaces are allowed, but slash, backslash, colon and control characters are not. Usually omit name; it does not locate the source file."));
    }
    Ok(())
}

fn validate_mime(mime: &str) -> Result<(), ErrorData> {
    let valid = reqwest::multipart::Part::bytes(Vec::new())
        .mime_str(mime)
        .is_ok()
        && HeaderValue::from_str(mime).is_ok();
    if !valid {
        return Err(invalid("mime_type must be a standard MIME type such as application/pdf. Omit it when unsure; do not put a filename or extension here."));
    }
    Ok(())
}

fn invalid(message: &'static str) -> ErrorData {
    error(message, "invalid_request", "not_started")
}

fn file_error(operation: &'static str, cause: std::io::Error) -> ErrorData {
    let reason = match cause.kind() {
        std::io::ErrorKind::NotFound => "File not found on the MCP host. Use its exact absolute path, or resolve a relative path from the session working directory. Chinese and paths outside the workspace are supported. Do not URL-encode, add shell quotes, or use a remote client's path.",
        std::io::ErrorKind::PermissionDenied => "The MCP host cannot access this file. Check its read permissions; changing name cannot grant access.",
        _ => "The MCP host could not access this file. Check that the path exists and is readable on that host; Chinese filenames and paths outside the workspace are supported.",
    };
    tracing::warn!(target: "builtin_mcp", operation, io_kind = ?cause.kind(),
        os_error = cause.raw_os_error(), "[iyw-upload] local file access failed");
    error(
        format!(
            "{operation}: {reason} (I/O kind: {:?}, OS code: {:?})",
            cause.kind(),
            cause.raw_os_error()
        ),
        "invalid_request",
        "not_started",
    )
}
