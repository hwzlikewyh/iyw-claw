use std::{
    path::Path,
    time::{Duration, Instant},
};

use rmcp::ErrorData;
use serde_json::{json, Value};

use super::{
    error, extract_url,
    file::{validate_size, UploadFile},
    public_url, IywGatewayService,
};

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) async fn upload(
    service: &IywGatewayService,
    file: UploadFile,
) -> Result<Value, ErrorData> {
    validate_size(file.size_bytes)?;
    let client = reqwest::Client::builder()
        .timeout(UPLOAD_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| {
            error(
                "Failed to initialize upload client",
                "client_error",
                "not_started",
            )
        })?;
    let started = Instant::now();
    let signed = presign(service, &file).await?;
    let url = public_url(&signed);
    let size = file.size_bytes;
    tracing::info!(target: "builtin_mcp", size_bytes = size, "[iyw-upload] storage upload started");
    let response = client
        .put(signed)
        .header(reqwest::header::CONTENT_TYPE, &file.mime_type)
        .header(reqwest::header::CONTENT_LENGTH, size)
        .body(file.body)
        .send()
        .await
        .map_err(transport_error)?;
    let status = response.status();
    tracing::info!(target: "builtin_mcp", status = status.as_u16(), size_bytes = size,
        duration_ms = started.elapsed().as_millis(), "[iyw-upload] storage response received");
    if !status.is_success() {
        return Err(error(
            format!("IYW storage rejected the upload (HTTP {}). Check storage permissions and signature validity; changing the local path or name will not fix this rejection.", status.as_u16()),
            "storage_rejected",
            "responded",
        ));
    }
    Ok(json!({"ok": true, "url": url, "name": file.name,
        "mime_type": file.mime_type, "size_bytes": size}))
}

async fn presign(
    service: &IywGatewayService,
    file: &UploadFile,
) -> Result<reqwest::Url, ErrorData> {
    let extension = Path::new(&file.name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| value.len() <= 16 && value.bytes().all(|ch| ch.is_ascii_alphanumeric()))
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let key = format!(
        "AI/{}/{}/{}{extension}",
        file.category,
        chrono::Local::now().format("%y%m%d"),
        uuid::Uuid::new_v4().simple()
    );
    let value = service
        .post_gateway(
            "/ai-application/api/microModel",
            "PreSignedUrl",
            json!({"objectKey": key}),
        )
        .await
        .map_err(presign_error)?;
    extract_url(&value).map_err(|_| {
        error(
            "IYW returned an invalid upload URL",
            "invalid_response",
            "not_started",
        )
    })
}

fn presign_error(cause: ErrorData) -> ErrorData {
    let status = cause
        .data
        .as_ref()
        .and_then(|data| data.get("status"))
        .and_then(Value::as_u64);
    let business_code = cause
        .data
        .as_ref()
        .and_then(|data| data.get("code"))
        .and_then(Value::as_i64);
    let reason = if cause.message == "Sign in to iyw-claw before using IYW tools"
        || matches!(status, Some(401 | 403))
        || matches!(business_code, Some(403 | 404))
    {
        "Sign in again on the MCP host, then retry only after login is restored."
    } else if cause.message == "IYW image gateway request failed" {
        "The MCP host could not reach the upload authorization service. Check its network, proxy and TLS connectivity."
    } else if cause.message == "IYW image gateway response failed" {
        "The upload authorization service returned an unreadable response. Check service availability."
    } else {
        "Check the MCP host's current login and upload authorization service availability."
    };
    let status = status
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unavailable".into());
    let business_code = business_code
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unavailable".into());
    error(
        format!("IYW upload authorization failed (HTTP: {status}, business code: {business_code}). {reason} No file bytes were uploaded. Changing path or name cannot fix this stage."),
        "presign_failed",
        "not_started",
    )
}

fn transport_error(cause: reqwest::Error) -> ErrorData {
    tracing::warn!(target: "builtin_mcp", timeout = cause.is_timeout(),
        connect = cause.is_connect(), status = cause.status().map(|status| status.as_u16()),
        "[iyw-upload] storage transport failed");
    let reason = if cause.is_timeout() {
        "Storage upload timed out"
    } else if cause.is_connect() {
        "The MCP host could not connect to storage; check its network, proxy and TLS connectivity"
    } else {
        "Storage upload transport failed"
    };
    error(
        format!("{reason}; an object may already exist. Do not retry blindly or change name/path to repeat the upload."),
        "transport_error",
        "unknown",
    )
}
