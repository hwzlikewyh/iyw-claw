use reqwest::StatusCode;
use serde_json::Value;

use crate::chat_channel::error::ChatChannelError;

use super::super::types::{ProviderCredentials, ProviderPoll, ProviderSession, ProviderStart};

const BASE_URL: &str = "https://oapi.dingtalk.com";
const SOURCE: &str = "openClaw";

pub async fn start(client: &reqwest::Client) -> Result<ProviderStart, ChatChannelError> {
    let (_, initialized) = post(
        client,
        "/app/registration/init",
        serde_json::json!({
            "source": SOURCE,
        }),
    )
    .await?;
    let nonce = required(&initialized, &["nonce"])?;
    let (_, begun) = post(
        client,
        "/app/registration/begin",
        serde_json::json!({
            "nonce": nonce,
        }),
    )
    .await?;
    Ok(ProviderStart {
        session: ProviderSession::Dingtalk {
            device_code: required(&begun, &["device_code"])?,
        },
        qr_content: required(&begun, &["verification_uri_complete", "verification_url"])?,
        expires_in_secs: first_u64(&begun, &["expires_in"])
            .unwrap_or(7200)
            .clamp(60, 7200),
        retry_after_ms: first_u64(&begun, &["interval"]).unwrap_or(3).clamp(2, 30) * 1000,
    })
}

pub async fn poll(
    client: &reqwest::Client,
    device_code: &str,
) -> Result<ProviderPoll, ChatChannelError> {
    let (status, result) = post(
        client,
        "/app/registration/poll",
        serde_json::json!({
            "device_code": device_code,
        }),
    )
    .await?;
    if !status.is_success() && first_string(&result, &["status", "state"]).is_none() {
        return Err(ChatChannelError::ConnectionFailed(format!(
            "钉钉扫码服务返回 HTTP {status}"
        )));
    }
    let status = first_string(&result, &["status", "state"])
        .unwrap_or_else(|| "WAITING".to_string())
        .to_ascii_uppercase();
    match status.as_str() {
        "WAITING" => Ok(ProviderPoll::Waiting),
        "SCANNED" | "SCAN_SUCCESS" | "PENDING_CONFIRMATION" => Ok(ProviderPoll::Scanned),
        "SUCCESS" | "AUTHORIZED" | "CONFIRMED" | "APPROVED" => approved(&result),
        "EXPIRED" => Ok(ProviderPoll::Expired),
        "FAIL" | "DENIED" | "REJECTED" => Ok(ProviderPoll::Denied("provider_denied")),
        _ => {
            tracing::debug!(
                provider_status = status,
                "[ChatChannel] DingTalk QR returned an unrecognized status"
            );
            Ok(ProviderPoll::Waiting)
        }
    }
}

async fn post(
    client: &reqwest::Client,
    path: &str,
    body: serde_json::Value,
) -> Result<(StatusCode, Value), ChatChannelError> {
    let result = client
        .post(format!("{BASE_URL}{path}"))
        .json(&body)
        .send()
        .await
        .map_err(network_error)?;
    let status = result.status();
    let body = result.json::<Value>().await.map_err(decode_error)?;
    if first_i64(&body, &["errcode", "code"]).is_some_and(|code| code != 0)
        && first_string(&body, &["status", "state"]).is_none()
    {
        return Err(ChatChannelError::ConnectionFailed(
            "钉钉扫码服务拒绝请求".to_string(),
        ));
    }
    Ok((status, body))
}

fn approved(result: &Value) -> Result<ProviderPoll, ChatChannelError> {
    let client_id = required(result, &["client_id", "clientId", "app_id", "appId"])?;
    let token = required(
        result,
        &["client_secret", "clientSecret", "app_secret", "appSecret"],
    )?;
    Ok(ProviderPoll::Approved(ProviderCredentials {
        token,
        config_patch: serde_json::json!({ "clientId": client_id }),
    }))
}

fn required(body: &Value, fields: &[&str]) -> Result<String, ChatChannelError> {
    first_string(body, fields).ok_or_else(|| {
        ChatChannelError::ConnectionFailed(format!(
            "钉钉扫码响应缺少 {}",
            fields.first().copied().unwrap_or("field")
        ))
    })
}

fn first_string(body: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| {
        body.get(*field)
            .or_else(|| body.pointer(&format!("/data/{field}")))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn first_u64(body: &Value, fields: &[&str]) -> Option<u64> {
    fields.iter().find_map(|field| {
        body.get(*field)
            .or_else(|| body.pointer(&format!("/data/{field}")))
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
    })
}

fn first_i64(body: &Value, fields: &[&str]) -> Option<i64> {
    fields.iter().find_map(|field| {
        body.get(*field)
            .or_else(|| body.pointer(&format!("/data/{field}")))
            .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
    })
}

fn network_error(error: reqwest::Error) -> ChatChannelError {
    super::request_error("钉钉", &error)
}

fn decode_error(error: reqwest::Error) -> ChatChannelError {
    super::request_error("钉钉", &error)
}
