use serde_json::Value;

use super::DingtalkBackend;
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http;

pub(super) async fn session_accepted(
    response: reqwest::Response,
) -> Result<bool, ChatChannelError> {
    let status = response.status();
    if matches!(status.as_u16(), 401 | 403 | 404 | 410) {
        return Ok(false);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ChatChannelError::DeliveryDeferred(
            "PROVIDER_RATE_LIMIT".into(),
        ));
    }
    let bytes = media_http::read_limited(response, 1024 * 1024)
        .await
        .map_err(|_| {
            ChatChannelError::SendFailed("DELIVERY_UNKNOWN:DINGTALK_WEBHOOK_RESPONSE".into())
        })?;
    let body: Value = serde_json::from_slice(&bytes).map_err(|_| {
        ChatChannelError::SendFailed("DELIVERY_UNKNOWN:DINGTALK_WEBHOOK_JSON".into())
    })?;
    match body["errcode"].as_i64() {
        Some(0) => Ok(true),
        Some(_) => Ok(false),
        None => Err(ChatChannelError::SendFailed(
            "DELIVERY_UNKNOWN:DINGTALK_WEBHOOK_RECEIPT".into(),
        )),
    }
}

impl DingtalkBackend {
    pub(super) async fn post_openapi(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<Value, ChatChannelError> {
        for attempt in 0..2 {
            let token = self.access_token().await?;
            let response = self
                .client
                .post(format!("https://api.dingtalk.com/v1.0/robot/{path}"))
                .header("x-acs-dingtalk-access-token", &token)
                .json(body)
                .send()
                .await
                .map_err(|error| {
                    if error.is_connect() {
                        ChatChannelError::NotConnected
                    } else {
                        ChatChannelError::SendFailed("DELIVERY_UNKNOWN:DINGTALK_TRANSPORT".into())
                    }
                })?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                if attempt != 0 {
                    return Err(ChatChannelError::AuthenticationFailed(
                        "DINGTALK_TOKEN_REJECTED".into(),
                    ));
                }
                let mut cache = self.media_token.lock().await;
                if cache.as_ref().is_some_and(|entry| entry.value == token) {
                    *cache = None;
                }
                continue;
            }
            if response.status() == reqwest::StatusCode::FORBIDDEN {
                return Err(ChatChannelError::ConfigurationInvalid(
                    "DINGTALK_PERMISSION_DENIED".into(),
                ));
            }
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(ChatChannelError::DeliveryDeferred(
                    "PROVIDER_RATE_LIMIT".into(),
                ));
            }
            return media_http::json(response).await;
        }
        Err(ChatChannelError::AuthenticationFailed(
            "DINGTALK_TOKEN_REJECTED".into(),
        ))
    }
}
