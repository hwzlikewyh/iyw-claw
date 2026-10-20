use serde_json::Value;

use crate::chat_channel::error::ChatChannelError;

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

pub(super) fn check(value: &Value) -> Result<(), ChatChannelError> {
    let code = super::response_code(value)
        .ok_or_else(|| ChatChannelError::SendFailed("DELIVERY_UNKNOWN:WEIXIN_RESPONSE".into()))?;
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
