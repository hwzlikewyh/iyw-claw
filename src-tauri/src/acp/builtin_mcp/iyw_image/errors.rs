use serde_json::{json, Map, Value};

use crate::acp::stderr_tail::sanitize_diagnostic;

mod wire;
pub(in crate::acp::builtin_mcp) use wire::{response_payload, transport_failure};

const MAX_MESSAGE_CHARS: usize = 900;
const MAX_IDENTIFIER_CHARS: usize = 128;
const MAX_OPAQUE_TOKEN_CHARS: usize = 40;

pub(in crate::acp::builtin_mcp) struct UpstreamFailure<'a> {
    pub source: &'static str,
    pub operation: &'a str,
    pub status: u16,
    pub payload: &'a Value,
    pub request_id: Option<String>,
    pub token: &'a str,
}

pub(in crate::acp::builtin_mcp) fn request_id(response: &reqwest::Response) -> Option<String> {
    ["x-request-id", "request-id", "x-trace-id"]
        .iter()
        .find_map(|key| {
            response
                .headers()
                .get(*key)
                .and_then(|value| value.to_str().ok())
                .and_then(identifier)
        })
}

pub(in crate::acp::builtin_mcp) fn upstream_failure(input: UpstreamFailure<'_>) -> rmcp::ErrorData {
    let details = upstream_details(&input);
    let code = details["error_code"]
        .as_str()
        .unwrap_or("upstream_status_error");
    let message = details["message"]
        .as_str()
        .unwrap_or("inspect error details before retrying");
    tracing::warn!(
        operation = input.operation,
        source = input.source,
        http_status = input.status,
        error_code = code,
        request_id = details["request_id"].as_str(),
        stage = details["stage"].as_str(),
        execution_status = details["execution_status"].as_str(),
        retryable = details["retryable"].as_bool(),
        backend_message = message,
        "[iyw-image] upstream operation failed"
    );
    rmcp::ErrorData::internal_error(
        format!("IYW image request failed ({code}); {message}"),
        Some(details),
    )
}

fn upstream_details(input: &UpstreamFailure<'_>) -> Value {
    let error = input
        .payload
        .get("error")
        .filter(|value| value.is_object())
        .unwrap_or(input.payload);
    let code = safe_code(error.get("code"), input.token);
    let error_code = code
        .as_str()
        .and_then(identifier)
        .unwrap_or_else(|| "upstream_status_error".into());
    let message = ["message", "msg", "reason"]
        .iter()
        .find_map(|key| error.get(*key).and_then(Value::as_str))
        .map(|value| safe_message(value, input.token));
    let execution_status = execution_status(input, &error_code);
    let request_id = input.request_id.clone().or_else(|| {
        input
            .payload
            .get("request_id")
            .and_then(Value::as_str)
            .and_then(identifier)
    });
    let stage = failure_stage(&error_code);
    let retryable = can_retry(input, &error_code, stage);
    json!({"status": input.status, "code": code, "error_code": error_code,
        "message": message, "request_id": request_id, "stage": stage,
        "task_id": response_id(input.payload, &["taskId", "task_id"]),
        "batch_id": response_id(input.payload, &["batchId", "batch_id"]),
        "execution_status": execution_status, "retryable": retryable,
        "generation_completed": matches!(stage, "output_download" | "output_persist")})
}

fn response_id(payload: &Value, keys: &[&str]) -> Option<String> {
    let data = payload
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(payload);
    keys.iter().find_map(|key| match data.get(*key) {
        Some(Value::String(value)) => identifier(value),
        Some(Value::Number(value)) if value.is_u64() || value.is_i64() => Some(value.to_string()),
        _ => None,
    })
}

fn safe_code(value: Option<&Value>, token: &str) -> Value {
    match value {
        Some(value) if value.is_number() => value.clone(),
        Some(Value::String(value)) if value.as_str() != token => {
            identifier(value).map(Value::String).unwrap_or(Value::Null)
        }
        _ => Value::Null,
    }
}

fn can_retry(input: &UpstreamFailure<'_>, code: &str, stage: &str) -> bool {
    if matches!(
        code,
        "invalid_api_key"
            | "insufficient_quota"
            | "insufficient_balance"
            | "permission_denied"
            | "points_balance_unavailable"
            | "points_auth_rejected"
            | "content_policy_violation"
    ) {
        return false;
    }
    input.source == "fusion"
        && execution_status(input, code) != "unknown"
        && (matches!(input.status, 429 | 503)
            || matches!(stage, "output_download" | "output_persist")
            || matches!(
                code,
                "invalid_upstream_image_response" | "no_available_upstream"
            ))
}

fn execution_status(input: &UpstreamFailure<'_>, code: &str) -> &'static str {
    if input.source == "catalog" {
        return "not_started";
    }
    if input.source != "fusion" || matches!(code, "upstream_failed" | "upstream_wait_canceled") {
        return "unknown";
    }
    "failed"
}

fn failure_stage(code: &str) -> &'static str {
    match code {
        "upstream_image_download_failed" => "output_download",
        "image_persist_failed" | "image_storage_unavailable" => "output_persist",
        _ => "upstream",
    }
}

pub(super) fn details(error: &rmcp::ErrorData) -> Value {
    let mut safe = Map::new();
    if let Some(data) = error.data.as_ref().and_then(Value::as_object) {
        for key in [
            "status",
            "code",
            "error_code",
            "message",
            "request_id",
            "model_name",
            "stage",
            "execution_status",
            "retryable",
            "generation_completed",
            "timeout",
            "connect",
            "task_id",
            "task_ids",
            "batch_id",
            "query",
        ] {
            if let Some(value) = data.get(key).and_then(|value| detail_value(key, value)) {
                safe.insert(key.to_string(), value);
            }
        }
    }
    Value::Object(safe)
}

fn detail_value(key: &str, value: &Value) -> Option<Value> {
    if value.is_number() || value.is_boolean() || value.is_null() {
        return Some(value.clone());
    }
    if key == "task_ids" {
        let ids = value
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .filter_map(identifier)
            .collect::<Vec<_>>();
        return Some(json!(ids));
    }
    let text = value.as_str()?;
    match key {
        "request_id" | "task_id" | "batch_id" | "code" | "error_code" => {
            identifier(text).map(Value::String)
        }
        "query" => (text.starts_with("/ai-") && !text.contains('?') && !text.contains('#'))
            .then(|| json!(text)),
        _ => Some(json!(safe_message(text, ""))),
    }
}

fn identifier(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= MAX_IDENTIFIER_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)))
    .then(|| value.to_string())
}

fn safe_message(value: &str, token: &str) -> String {
    if value.chars().count() > MAX_MESSAGE_CHARS
        || value.to_ascii_lowercase().contains("data:image/")
    {
        return "Backend diagnostic omitted because it may contain image data".into();
    }
    let value = if token.is_empty() {
        value.to_string()
    } else {
        value.replace(token, "***")
    };
    let words = value
        .split_whitespace()
        .map(|word| {
            if word.contains("://")
                || word.len() > MAX_IDENTIFIER_CHARS
                || (word.len() >= MAX_OPAQUE_TOKEN_CHARS
                    && word
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"+/=_-".contains(&byte)))
            {
                "<redacted>"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    sanitize_diagnostic(&words)
}

pub(super) fn model_failure(mut error: rmcp::ErrorData, name: &str) -> rmcp::ErrorData {
    let mut data = details(&error);
    data["model_name"] = json!(safe_message(name, ""));
    error.data = Some(data);
    error
}

pub(super) fn query_failure(
    mut error: rmcp::ErrorData,
    ids: &[String],
    query: &str,
) -> rmcp::ErrorData {
    let mut data = details(&error);
    data["task_ids"] = json!(ids);
    data["task_id"] = json!(ids.first());
    data["query"] = json!(query);
    data["stage"] = json!("task_query");
    data["execution_status"] = json!("unknown");
    data["retryable"] = json!(false);
    error.data = Some(data);
    error
}
