use reqwest::Url;
use rmcp::{model::CallToolResult, ErrorData};
use serde_json::{json, Value};

use super::{authority::SessionContext, iyw_service::IywGatewayService};

mod file;
mod http;

pub(super) const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

pub(super) fn tool() -> Value {
    json!({
        "name": super::tool_identity::UPLOAD_TOOL,
        "description": "Upload one local file of any type to IYW storage using the current iyw-claw login. Accepts documents, archives, audio, video, images and arbitrary binary files, at most 50 MiB (52,428,800 bytes, inclusive). Provide an absolute path inside the current workspace or a workspace-relative path and a short description of the current action. The path is on the host running this MCP, not a remote client. Directories and paths outside the workspace are rejected; create an archive first when uploading a directory. The host reads bounded file bytes, requests a signed URL and uploads without exposing credentials. Returns ok, public HTTPS url, name, mime_type and size_bytes only after the storage upload succeeds. The resulting URL can be accessed by anyone who has it; upload only the file authorized by the user. No extension allowlist; mime_type is optional and defaults to application/octet-stream. No base64 or URL inputs; place the file in the workspace first. No capability search/read, token or cookie arguments. This does not create a product, parse a document, or register a final artifact: use the returned URL in fetch_iyw_url or present_task_files as appropriate. generate_iyw_image already uploads its image inputs internally. No automatic retries: cancellation or transport failure may leave an uploaded object; report uncertainty without repeating the upload blindly.",
        "inputSchema": {
            "type": "object", "required": ["path", "description"],
            "properties": {
                "path": {"type": "string", "minLength": 1, "description": "Path to one regular file inside the current workspace. Absolute or workspace-relative."},
                "description": super::iyw_progress::schema(),
                "name": {"type": "string", "minLength": 1, "maxLength": 255, "description": "Optional file name including extension; defaults to the local basename. No directories or control characters."},
                "mime_type": {"type": "string", "minLength": 1, "description": "Optional valid MIME type. Defaults to application/octet-stream; no file-type filtering is applied."}
            },
            "additionalProperties": false
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true}
    })
}

pub(super) async fn upload(
    service: &IywGatewayService,
    authority: &SessionContext,
    arguments: Value,
) -> Result<CallToolResult, ErrorData> {
    let file = file::prepare(authority.cwd(), arguments).await?;
    let size = file.bytes.len();
    tracing::info!(target: "builtin_mcp", size_bytes = size, "[iyw-upload] file validated");
    let url = http::upload(service, file).await?;
    Ok(CallToolResult::structured(url))
}

pub(super) async fn upload_image_bytes(
    service: &IywGatewayService,
    bytes: Vec<u8>,
    content: (&str, &str),
) -> Result<String, ErrorData> {
    let file = file::UploadFile {
        bytes,
        name: format!("image.{}", content.1),
        mime_type: content.0.to_string(),
        category: "img",
    };
    let result = http::upload(service, file).await?;
    Ok(result["url"]
        .as_str()
        .expect("upload returns a URL")
        .to_string())
}

pub(super) fn failure(error: ErrorData) -> CallToolResult {
    let value = json!({"ok": false, "error": {
        "message": error.message,
        "code": error.data.as_ref().and_then(|data| data.get("code")),
        "execution_status": error.data.as_ref().and_then(|data| data.get("execution_status")),
        "retryable": false
    }});
    CallToolResult::structured_error(value)
}

pub(super) fn error(message: &'static str, code: &'static str, state: &'static str) -> ErrorData {
    tracing::warn!(target: "builtin_mcp", code, execution_status = state,
        reason = message, "[iyw-upload] upload failed");
    ErrorData::invalid_request(
        message,
        Some(json!({"code": code, "execution_status": state})),
    )
}

pub(super) fn cancelled() -> ErrorData {
    error(
        "Upload cancelled; an object may already exist. Do not retry blindly.",
        "cancelled",
        "unknown",
    )
}

pub(super) fn extract_url(value: &Value) -> Result<Url, rmcp::ErrorData> {
    let raw = value
        .as_str()
        .or_else(|| value.get("value").and_then(Value::as_str))
        .or_else(|| value.get("url").and_then(Value::as_str))
        .ok_or_else(|| rmcp::ErrorData::invalid_params("upload URL is missing", None))?;
    let url = Url::parse(raw)
        .map_err(|_| rmcp::ErrorData::invalid_params("upload URL is invalid", None))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(rmcp::ErrorData::invalid_params(
            "upload URL must be credential-free HTTPS",
            None,
        ));
    }
    Ok(url)
}

pub(super) fn public_url(url: &Url) -> String {
    let mut clean = url.clone();
    clean.set_query(None);
    clean.set_fragment(None);
    clean.to_string()
}
