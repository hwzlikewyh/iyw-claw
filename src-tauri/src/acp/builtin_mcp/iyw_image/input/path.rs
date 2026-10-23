use std::path::{Path, PathBuf};

use rmcp::ErrorData;
use serde_json::json;

use super::{invalid, IywGatewayService, SessionContext};
use crate::acp::agent_image_input::{access, CodexImageScope};

pub(super) async fn resolve(
    (service, authority): (&IywGatewayService, &SessionContext),
    value: &str,
) -> Result<PathBuf, ErrorData> {
    let candidate = source_path(authority.cwd(), value)?;
    let root = tokio::fs::canonicalize(authority.cwd())
        .await
        .map_err(|error| io_error("working_directory", error))?;
    let path = tokio::fs::canonicalize(&candidate)
        .await
        .map_err(|error| io_error("image_path", error))?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|error| io_error("image_metadata", error))?;
    if !metadata.is_file() {
        return Err(invalid("image source must be a regular file"));
    }
    if path.starts_with(root) {
        return Ok(path);
    }
    let lookup = &service.listener().parent_lookup;
    let conversation_id = lookup
        .current_conversation_id(authority.connection_id())
        .await;
    let scope = CodexImageScope {
        connection_id: authority.connection_id().to_owned(),
        conversation_id,
        working_dir: Some(authority.cwd().to_path_buf()),
    };
    let path = access::validate_path(scope, &candidate).await.map_err(|error| {
        tracing::warn!(target: "builtin_mcp", connection_id = authority.connection_id(),
            stage = "attachment_scope", error = %error, "[iyw-image] attachment access rejected");
        invalid("Image path must be inside the workspace or belong to this session. Use the exact attachment path supplied with the image; do not guess paths.")
    })?;
    if authority.cancellation().is_cancelled()
        || lookup
            .current_conversation_id(authority.connection_id())
            .await
            != conversation_id
    {
        return Err(invalid(
            "Image attachment session changed; reconnect before retrying",
        ));
    }
    tracing::debug!(target: "builtin_mcp", connection_id = authority.connection_id(),
        "[iyw-image] session attachment validated");
    Ok(path)
}

fn source_path(cwd: &Path, value: &str) -> Result<PathBuf, ErrorData> {
    if value.starts_with("file:") {
        return reqwest::Url::parse(value)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or_else(|| invalid("image file URI is invalid"));
    }
    if value.contains("://") {
        return Err(invalid(
            "image source must be HTTPS, a file URI, or an exact local path",
        ));
    }
    Ok(cwd.join(value))
}

fn io_error(stage: &'static str, error: std::io::Error) -> ErrorData {
    tracing::warn!(target: "builtin_mcp", stage, error_kind = ?error.kind(),
        os_error = error.raw_os_error(), error = %error, "[iyw-image] local input validation failed");
    ErrorData::invalid_params(
        "Image input could not be read. Use the exact supplied attachment path or an existing workspace file; do not guess paths.",
        Some(json!({"code": "image_input_unavailable", "stage": stage,
            "execution_status": "not_started", "retryable": false,
            "error_kind": format!("{:?}", error.kind()), "os_error": error.raw_os_error()})),
    )
}
