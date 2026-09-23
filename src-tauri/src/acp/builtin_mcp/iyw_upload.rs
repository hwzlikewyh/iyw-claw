use reqwest::Url;
use rmcp::{model::CallToolResult, ErrorData};
use serde_json::{json, Value};

use super::{authority::SessionContext, iyw_service::IywGatewayService};

mod file;
mod http;

pub(super) const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;

pub(super) fn tool() -> Value {
    json!({
        "name": super::tool_identity::UPLOAD_TOOL,
        "description": "Upload one local file readable by the MCP host (any type, up to 1 GiB / 1,073,741,824 bytes) using the current iyw-claw login. Call directly with path and description; omit name and mime_type unless needed. Chinese names, spaces and parentheses are supported: pass the exact filesystem path, without URL encoding, shell quotes or file://. Absolute paths may be anywhere on the host, including outside the workspace. Relative paths resolve from the session working directory; parent-directory paths and symlink targets outside the workspace are allowed. Archive directories first. name only overrides the returned filename, never the source path. Returns ok, public HTTPS url, name, mime_type and size_bytes after storage accepts the upload. Anyone with the URL can access it; upload only authorized files. No token, cookie, base64 or capability search/read needed. This does not create a business record or register a deliverable; use the returned URL in fetch_iyw_url or present_task_files. Image generation uploads its own inputs. On failure read error.code/message/execution_status: correct invalid input once; resolve login/network errors before another attempt. Changing name or path cannot fix presign_failed or storage_rejected. Never retry an unknown outcome blindly or invent an alternate upload endpoint.",
        "inputSchema": {
            "type": "object", "required": ["path", "description"],
            "examples": [
                {"path": "D:/Downloads/设计方案（最终版）.pdf", "description": "上传设计方案"},
                {"path": "交付文件/设计方案（最终版）.pdf", "description": "上传设计方案"}
            ],
            "properties": {
                "path": {"type": "string", "minLength": 1, "description": "Exact path to a readable regular file on the MCP host. Prefer an absolute path; files outside the workspace are allowed. Relative paths resolve from the session working directory, including ../ paths. Chinese and spaces are supported. Do not URL-encode or include shell quotes. Windows JSON: use D:/Downloads/中文.pdf or escaped backslashes."},
                "description": super::iyw_progress::schema(),
                "name": {"type": "string", "minLength": 1, "maxLength": 255, "description": "Usually omit. Overrides the returned filename, not the file to read; defaults to the local basename. Chinese and spaces are supported. Include an extension; no slash, backslash, colon or control characters."},
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
    let size = file.size_bytes;
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
        size_bytes: bytes.len() as u64,
        body: bytes.into(),
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

pub(super) fn error(
    message: impl Into<String>,
    code: &'static str,
    state: &'static str,
) -> ErrorData {
    let message = message.into();
    tracing::warn!(target: "builtin_mcp", code, execution_status = state,
        reason = %message, "[iyw-upload] upload failed");
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
