use std::time::Duration;

use reqwest::Response;
use serde_json::Value;

use super::{account_error, types, AppCommandError, ACCOUNT_BASE_URL};

const REQUEST_TIMEOUT_SECONDS: u64 = 15;

pub(super) fn http_client() -> Result<reqwest::Client, AppCommandError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .user_agent("iyw-claw")
        .build()
        .map_err(|error| {
            AppCommandError::network("Unable to initialize account client")
                .with_detail(error.without_url().to_string())
        })
}

pub(super) async fn post(
    path: &str,
    body: Value,
    token: Option<&str>,
) -> Result<Value, AppCommandError> {
    let mut request = http_client()?
        .post(format!("{ACCOUNT_BASE_URL}/api{path}"))
        .header("Accept", "application/json, text/plain, */*")
        .header("Origin", ACCOUNT_BASE_URL)
        .header("Referer", format!("{ACCOUNT_BASE_URL}/"))
        .json(&body);
    if let Some(token) = token {
        request = request.header("token", token);
    }
    let response = request.send().await.map_err(|error| {
        tracing::warn!(stage = path, "[iyw-account] request failed");
        AppCommandError::network("Account service is temporarily unavailable")
            .with_detail(error.without_url().to_string())
    })?;
    decode(response, path).await
}

pub(super) async fn decode(response: Response, stage: &str) -> Result<Value, AppCommandError> {
    let status = response.status();
    if !status.is_success() {
        tracing::warn!(
            stage,
            http_status = status.as_u16(),
            "[iyw-account] request rejected"
        );
        return Err(if status.as_u16() == 401 || status.as_u16() == 403 {
            account_error(None, "Account authentication has expired")
        } else {
            AppCommandError::network("Account service request failed")
        });
    }
    response
        .json::<Value>()
        .await
        .map_err(|_| AppCommandError::network("Account service returned an invalid response"))
}

pub(super) fn success(response: &Value, fallback: &str) -> Result<(), AppCommandError> {
    let code = types::code(response);
    if matches!(code, Some(1 | 413)) {
        return Ok(());
    }
    tracing::warn!(business_code = ?code, "[iyw-account] operation rejected");
    let message = types::field(response, "message").unwrap_or_else(|| fallback.to_string());
    Err(if matches!(code, Some(402 | 403 | 405)) {
        account_error(Some(message), fallback)
    } else {
        AppCommandError::invalid_input(message)
    })
}

pub(super) fn required(data: &Value, key: &str) -> Result<String, AppCommandError> {
    types::field(data, key).ok_or_else(|| AppCommandError::invalid_input(format!("Missing {key}")))
}

pub(super) fn phone(data: &Value) -> Result<String, AppCommandError> {
    let phone = required(data, "phone")?;
    if phone.len() != 11
        || !phone.starts_with('1')
        || !phone.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(AppCommandError::invalid_input("请输入正确的 11 位手机号"));
    }
    Ok(phone)
}

pub(super) fn normalize_asset_url(value: Option<String>) -> Option<String> {
    let value = value?.trim().to_string();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Some(value);
    }
    if value.starts_with("/saas/") {
        return Some(format!(
            "https://chdesign.oss-cn-shanghai.aliyuncs.com{value}"
        ));
    }
    if value.trim_start_matches('/') == "static/avatar/default.png" {
        return Some(super::DEFAULT_AVATAR_URL.to_string());
    }
    Some(format!(
        "{ACCOUNT_BASE_URL}/{}",
        value.trim_start_matches('/')
    ))
}
