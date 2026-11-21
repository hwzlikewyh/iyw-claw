use serde_json::Value;

use crate::chat_channel::error::ChatChannelError;

const MAX_RECEIPT_BYTES: usize = 1024 * 1024;

pub(super) async fn receipt(response: reqwest::Response) -> Result<Value, ChatChannelError> {
    let status = response.status();
    if !status.is_success() {
        return Err(ChatChannelError::SendFailed(format!("HTTP {status}")));
    }
    let bytes = crate::chat_channel::media_http::read_limited(response, MAX_RECEIPT_BYTES)
        .await
        .map_err(|error| {
            tracing::warn!(%status, reason = %error, "[Weixin] sendmessage receipt unavailable");
            ChatChannelError::SendFailed("DELIVERY_UNKNOWN:WEIXIN_RESPONSE".into())
        })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        tracing::warn!(%status, bytes = bytes.len(), kind = ?error.classify(), "[Weixin] invalid sendmessage JSON");
        ChatChannelError::SendFailed("DELIVERY_UNKNOWN:WEIXIN_RESPONSE".into())
    })
}

pub(super) fn message_id(value: &Value) -> Option<String> {
    let id = value.get("message_id")?;
    id.as_u64()
        .or_else(|| id.as_str()?.parse().ok())
        .map(|id| id.to_string())
}

pub(super) fn context_expired(value: &Value) -> bool {
    if super::response_code(value).unwrap_or(0) == 0 {
        return false;
    }
    let message = value["errmsg"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    (message.contains("context") && (message.contains("expired") || message.contains("invalid")))
        || message.contains("上下文过期")
}

fn is_success_without_code(value: &Value) -> bool {
    // iLink 的 SendMessageResp 允许省略 ret、errmsg 和 message_id，包括返回 {}。
    let Some(fields) = value.as_object() else {
        return false;
    };
    fields.iter().all(|(key, field)| match key.as_str() {
        "errmsg" => field.as_str() == Some(""),
        "message_id" => {
            field.is_u64() || field.as_str().is_some_and(|id| id.parse::<u64>().is_ok())
        }
        _ => false,
    })
}

pub(super) fn check(value: &Value) -> Result<(), ChatChannelError> {
    let Some(code) = super::response_code(value) else {
        if is_success_without_code(value) {
            return Ok(());
        }
        tracing::warn!(
            field_count = value.as_object().map(|fields| fields.len()),
            has_ret = value.get("ret").is_some(),
            has_errcode = value.get("errcode").is_some(),
            has_message_id = value.get("message_id").is_some(),
            has_errmsg = value.get("errmsg").is_some(),
            "[Weixin] unrecognized sendmessage receipt"
        );
        return Err(ChatChannelError::SendFailed(
            "DELIVERY_UNKNOWN:WEIXIN_RESPONSE".into(),
        ));
    };
    if code == 0 {
        return Ok(());
    }
    if code == -14 {
        return Err(ChatChannelError::AuthenticationFailed(
            "WEIXIN_ACCOUNT_SESSION_UNAVAILABLE".into(),
        ));
    }
    let message = value["errmsg"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if message.contains("rate limit") || message.contains("频率") {
        return Err(ChatChannelError::DeliveryDeferred(
            "PROVIDER_RATE_LIMIT".into(),
        ));
    }
    if context_expired(value) {
        return Err(ChatChannelError::DeliveryDeferred(
            "TARGET_CONTEXT_EXPIRED".into(),
        ));
    }
    if code == -2 {
        // -2 的公开契约不完整，保留 token，等待新上下文；不能断言它一定已过期。
        return Err(ChatChannelError::DeliveryDeferred(
            "WEIXIN_CONTEXT_OR_LIMIT_REJECTED".into(),
        ));
    }
    Err(ChatChannelError::SendFailed(format!(
        "PROVIDER_REJECTED:{code}"
    )))
}
