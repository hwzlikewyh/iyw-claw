use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, Method, Request, Response};
use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Value};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

pub(super) async fn execute(
    client: &Client,
    request: Request,
    token: &str,
) -> Result<CallToolResult, ErrorData> {
    let head = request.method() == Method::HEAD;
    let mut response = client.execute(request).await.map_err(transport_error)?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .replace(token, "[REDACTED]");
    tracing::info!(target: "builtin_mcp", host = response.url().host_str(),
        status = status.as_u16(), "[iyw-fetch] response received");
    let bytes = if head {
        Ok(Vec::new())
    } else {
        read_body(&mut response).await
    }
    .map_err(|mut error| {
        let data = error.data.get_or_insert_with(|| json!({}));
        data["status"] = json!(status.as_u16());
        data["content_type"] = json!(content_type);
        error
    })?;
    let (body_type, body) = decode(&bytes, &content_type, token);
    let error = (!status.is_success()).then(|| {
        json!({
            "code": "http_error", "message": format!("HTTP {}", status.as_u16()),
            "execution_status": "responded", "retryable": false
        })
    });
    tracing::info!(target: "builtin_mcp", status = status.as_u16(),
        response_bytes = bytes.len(), "[iyw-fetch] response completed");
    Ok(envelope(json!({
        "ok": status.is_success(), "status": status.as_u16(), "content_type": content_type,
        "body_type": body_type, "body": body, "error": error
    })))
}

async fn read_body(response: &mut Response) -> Result<Vec<u8>, ErrorData> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn decode(bytes: &[u8], content_type: &str, token: &str) -> (&'static str, Value) {
    if bytes.is_empty() {
        return ("empty", Value::Null);
    }
    if let Ok(mut value) = serde_json::from_slice(bytes) {
        redact_json(&mut value, token);
        return ("json", value);
    }
    if is_text(content_type) {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return ("text", json!(text.replace(token, "[REDACTED]")));
        }
    }
    ("base64", json!(STANDARD.encode(bytes)))
}

fn is_text(content_type: &str) -> bool {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    mime.is_empty()
        || mime.starts_with("text/")
        || mime.ends_with("json")
        || mime.ends_with("xml")
        || mime == "application/javascript"
        || mime == "application/x-www-form-urlencoded"
}

fn redact_json(value: &mut Value, token: &str) {
    match value {
        Value::String(text) => *text = text.replace(token, "[REDACTED]"),
        Value::Array(items) => items.iter_mut().for_each(|item| redact_json(item, token)),
        Value::Object(fields) => {
            *fields = std::mem::take(fields)
                .into_iter()
                .map(|(key, mut value)| {
                    redact_json(&mut value, token);
                    (key.replace(token, "[REDACTED]"), value)
                })
                .collect();
        }
        _ => {}
    }
}

pub(super) fn failure(error: ErrorData) -> CallToolResult {
    let data = error.data.as_ref();
    let code = data
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("request_failed");
    let execution = data
        .and_then(|value| value.get("execution_status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    tracing::warn!(target: "builtin_mcp", code, execution_status = execution,
        "[iyw-fetch] execution failed");
    envelope(json!({
        "ok": false,
        "status": data.and_then(|value| value.get("status")),
        "content_type": data.and_then(|value| value.get("content_type")).and_then(Value::as_str).unwrap_or_default(),
        "body_type": "empty", "body": null,
        "error": {"code": code, "message": error.message, "execution_status": execution, "retryable": false}
    }))
}

fn envelope(value: Value) -> CallToolResult {
    let is_error = value["ok"] != true;
    let mut result = CallToolResult::structured(value);
    result.is_error = Some(is_error);
    result
}

fn transport_error(error: reqwest::Error) -> ErrorData {
    tracing::warn!(target: "builtin_mcp", timeout = error.is_timeout(),
        connect = error.is_connect(), body = error.is_body(),
        status = error.status().map(|value| value.as_u16()), "[iyw-fetch] transport failed");
    let (code, message) = if error.is_timeout() {
        (
            "timeout",
            "IYW request timed out; execution may have occurred. Do not retry writes blindly.",
        )
    } else {
        (
            "transport_error",
            "IYW request failed; execution may have occurred. Do not retry writes blindly.",
        )
    };
    ErrorData::internal_error(
        message,
        Some(json!({"code": code, "execution_status": "unknown"})),
    )
}

fn too_large() -> ErrorData {
    ErrorData::internal_error(
        "IYW response exceeds the 2 MiB limit",
        Some(json!({
            "code": "response_too_large", "execution_status": "unknown"
        })),
    )
}

pub(super) fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["ok", "status", "content_type", "body_type", "body", "error"],
        "properties": {
            "ok": {"type": "boolean", "description": "HTTP 2xx and complete response; does not interpret business codes."},
            "status": {"type": ["integer", "null"], "description": "HTTP status, null when no HTTP response was received."},
            "content_type": {"type": "string"},
            "body_type": {"type": "string", "enum": ["json", "text", "base64", "empty"]},
            "body": {"description": "Parsed JSON, text/base64 string, or null for empty/unavailable body."},
            "error": {"anyOf": [
                {"type": "null"},
                {"type": "object", "required": ["code", "message", "execution_status", "retryable"],
                    "properties": {
                        "code": {"type": "string"}, "message": {"type": "string"},
                        "execution_status": {"type": "string", "enum": ["not_started", "responded", "unknown"]},
                        "retryable": {"type": "boolean"}
                    }, "additionalProperties": false}
            ]}
        },
        "additionalProperties": false
    })
}
