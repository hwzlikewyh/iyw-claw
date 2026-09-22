use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{json, Value};

use crate::app_error::AppCommandError;

const MAX_ERROR_DETAIL_CHARS: usize = 240;

#[derive(Debug, Clone)]
pub(crate) struct ModelGatewayChatConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
}

pub(crate) struct StructuredChatRequest<'a> {
    pub system_prompt: &'a str,
    pub user_content: String,
    pub json_schema: Value,
    pub max_tokens: u32,
    pub operation: &'static str,
}

pub(crate) async fn call_structured(
    config: &ModelGatewayChatConfig,
    request: StructuredChatRequest<'_>,
) -> Result<String, AppCommandError> {
    let operation = request.operation;
    let client = reqwest::Client::builder()
        .timeout(config.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| request_error(operation, "HTTP client build failed", error))?;
    let mut payload = structured_payload(config, &request);
    let result = send_structured(&client, config, (&payload, operation)).await;
    if result.as_ref().err().is_some_and(schema_format_unavailable) {
        tracing::warn!(
            operation,
            "[model-gateway] JSON schema rejected; retrying JSON mode"
        );
        payload["response_format"] = json!({"type": "json_object"});
        payload["messages"][0]["content"] = json!(format!(
            "{}\nReturn only one JSON object matching this schema, without Markdown fences:\n{}",
            request.system_prompt, request.json_schema["schema"]
        ));
        return send_structured(&client, config, (&payload, operation)).await;
    }
    result
}

fn structured_payload(
    config: &ModelGatewayChatConfig,
    request: &StructuredChatRequest<'_>,
) -> Value {
    json!({
        "model": config.model,
        "temperature": 0,
        "max_tokens": request.max_tokens,
        "response_format": { "type": "json_schema", "json_schema": request.json_schema },
        "messages": [
            { "role": "system", "content": request.system_prompt },
            { "role": "user", "content": request.user_content }
        ]
    })
}

async fn send_structured(
    client: &reqwest::Client,
    config: &ModelGatewayChatConfig,
    (payload, operation): (&Value, &str),
) -> Result<String, AppCommandError> {
    let response = client
        .post(&config.api_url)
        .bearer_auth(&config.api_key)
        .header("token", &config.api_key)
        .json(payload)
        .send()
        .await
        .map_err(|error| request_error(operation, "request failed", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| request_error(operation, "response read failed", error))?;
    let result = if status.is_success() {
        response_content(operation, &body)
    } else {
        Err(status_error(operation, status, &body))
    };
    if let Err(error) = &result {
        tracing::warn!(operation, http_status = status.as_u16(), code = ?error.code,
            reason = %error.message,
            "[model-gateway] structured request failed");
    }
    result
}

fn schema_format_unavailable(error: &AppCommandError) -> bool {
    if error.code != crate::app_error::AppErrorCode::ConfigurationInvalid {
        return false;
    }
    let detail = error
        .detail
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    (detail.contains("response_format") || detail.contains("json_schema"))
        && [
            "unavailable",
            "unsupported",
            "not supported",
            "does not support",
        ]
        .iter()
        .any(|reason| detail.contains(reason))
}

fn response_content(operation: &str, body: &str) -> Result<String, AppCommandError> {
    let root: Value = serde_json::from_str(body).map_err(|error| {
        AppCommandError::configuration_invalid(format!("{operation} response is not JSON"))
            .with_detail(error.to_string())
    })?;
    if let Some(code) = root.get("code").and_then(business_code) {
        if !matches!(code, 0 | 200) {
            let status = StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY);
            return Err(status_error(operation, status, body));
        }
    }
    if root.get("error").is_some_and(|error| !error.is_null()) {
        return Err(AppCommandError::configuration_invalid(format!(
            "{operation} provider returned an error"
        ))
        .with_detail(provider_error_detail(&root)));
    }
    if root
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        == Some("length")
    {
        return Err(AppCommandError::configuration_invalid(format!(
            "{operation} response exceeded the output budget"
        ))
        .with_detail("finish_reason=length; no complete structured result was returned"));
    }
    if let Some(content) = root.pointer("/choices/0/message/content") {
        if let Some(content) = decode_content(content) {
            return Ok(content);
        }
    }
    Err(
        AppCommandError::configuration_invalid(format!("{operation} response has no content"))
            .with_detail(empty_content_detail(&root)),
    )
}

fn business_code(value: &Value) -> Option<u16> {
    value
        .as_u64()
        .and_then(|code| u16::try_from(code).ok())
        .or_else(|| value.as_str()?.parse().ok())
}

fn empty_content_detail(root: &Value) -> String {
    if let Some(refusal) = root
        .pointer("/choices/0/message/refusal")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return safe_error_text(refusal);
    }
    let finish = root
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let choices = root
        .get("choices")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    format!(
        "missing message content; choices={choices}; finish_reason={}",
        safe_error_text(finish)
    )
}

fn request_error(operation: &str, stage: &str, error: impl std::fmt::Display) -> AppCommandError {
    AppCommandError::network(format!("{operation} {stage}"))
        .with_detail(safe_error_text(&error.to_string()))
}

fn status_error(operation: &str, status: StatusCode, body: &str) -> AppCommandError {
    let detail = serde_json::from_str::<Value>(body)
        .map(|value| provider_error_detail(&value))
        .unwrap_or_else(|_| safe_error_text(body));
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            AppCommandError::authentication_failed(format!("{operation} authentication failed"))
                .with_detail(detail)
        }
        StatusCode::BAD_REQUEST
        | StatusCode::UNPROCESSABLE_ENTITY
        | StatusCode::NOT_IMPLEMENTED => {
            AppCommandError::configuration_invalid(format!("{operation} request is incompatible"))
                .with_detail(detail)
        }
        StatusCode::TOO_MANY_REQUESTS => {
            AppCommandError::network(format!("{operation} rate limited")).with_detail(detail)
        }
        _ => AppCommandError::network(format!("{operation} returned HTTP {status}"))
            .with_detail(detail),
    }
}

fn decode_content(content: &Value) -> Option<String> {
    match content {
        Value::String(value) if !value.trim().is_empty() => Some(value.clone()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            (!text.trim().is_empty()).then_some(text)
        }
        Value::Object(_) => serde_json::to_string(content).ok(),
        _ => None,
    }
}

fn provider_error_detail(value: &Value) -> String {
    let code = value
        .pointer("/error/code")
        .or_else(|| value.get("code"))
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let message = value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(safe_error_text)
        .unwrap_or_else(|| "provider rejected the request".into());
    format!("code={}; message={message}", safe_error_text(&code))
}

fn safe_error_text(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_ascii_lowercase();
    if [
        "token", "password", "secret", "api_key", "bearer", "sk-", "prompt=",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return "[redacted]".into();
    }
    compact.chars().take(MAX_ERROR_DETAIL_CHARS).collect()
}
