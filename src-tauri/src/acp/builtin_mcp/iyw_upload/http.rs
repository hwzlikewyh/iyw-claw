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

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) async fn upload(
    service: &IywGatewayService,
    file: UploadFile,
) -> Result<Value, ErrorData> {
    validate_size(file.bytes.len() as u64)?;
    let client = reqwest::Client::builder()
        .timeout(UPLOAD_TIMEOUT)
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
    let size = file.bytes.len();
    tracing::info!(target: "builtin_mcp", size_bytes = size, "[iyw-upload] storage upload started");
    let response = client
        .put(signed)
        .header(reqwest::header::CONTENT_TYPE, &file.mime_type)
        .body(file.bytes)
        .send()
        .await
        .map_err(transport_error)?;
    let status = response.status();
    tracing::info!(target: "builtin_mcp", status = status.as_u16(), size_bytes = size,
        duration_ms = started.elapsed().as_millis(), "[iyw-upload] storage response received");
    if !status.is_success() {
        return Err(error(
            "IYW storage rejected the upload",
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
        .map_err(|_| {
            error(
                "IYW upload authorization failed; check the current login and service availability",
                "presign_failed",
                "not_started",
            )
        })?;
    extract_url(&value).map_err(|_| {
        error(
            "IYW returned an invalid upload URL",
            "invalid_response",
            "not_started",
        )
    })
}

fn transport_error(cause: reqwest::Error) -> ErrorData {
    tracing::warn!(target: "builtin_mcp", timeout = cause.is_timeout(),
        connect = cause.is_connect(), status = cause.status().map(|status| status.as_u16()),
        "[iyw-upload] storage transport failed");
    error(
        "Storage upload did not complete; an object may already exist",
        "transport_error",
        "unknown",
    )
}
