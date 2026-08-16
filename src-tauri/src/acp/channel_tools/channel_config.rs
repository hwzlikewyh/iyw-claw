use serde_json::{json, Value};

use super::types::ChannelConfigInput;

pub(super) fn create_config(
    channel_type: &str,
    input: Option<&ChannelConfigInput>,
) -> Result<Value, String> {
    let input = input.cloned().unwrap_or_default();
    match channel_type {
        "lark" => Ok(json!({
            "app_id": input.app_id.unwrap_or_default(),
            "chat_id": input.default_target.unwrap_or_default(),
            "default_agent_type": input.default_agent_type,
        })),
        "weixin" => Ok(json!({
            "base_url": input.base_url.unwrap_or_default(),
            "default_agent_type": input.default_agent_type,
        })),
        "wecom" => Ok(json!({
            "default_chatid": input.default_target.unwrap_or_default(),
            "default_chat_type": input.default_target_type.unwrap_or(1),
            "default_agent_type": input.default_agent_type,
            "poll_interval_secs": input.poll_interval_secs,
        })),
        "wecom_ai_bot" => Ok(json!({
            "bot_id": input.bot_id.unwrap_or_default(),
            "default_chatid": input.default_target.unwrap_or_default(),
            "default_agent_type": input.default_agent_type,
        })),
        "dingtalk" => Ok(json!({
            "client_id": input.client_id.unwrap_or_default(),
            "default_agent_type": input.default_agent_type,
        })),
        _ => Err("CHANNEL_TYPE_UNSUPPORTED".to_string()),
    }
}

pub(super) fn update_config(
    channel_type: &str,
    input: &ChannelConfigInput,
) -> Result<Value, String> {
    let mut patch = serde_json::Map::new();
    match channel_type {
        "lark" => {
            add(&mut patch, "appId", &input.app_id);
            add(&mut patch, "chatId", &input.default_target);
        }
        "weixin" => add(&mut patch, "baseUrl", &input.base_url),
        "wecom" => {
            add(&mut patch, "defaultChatid", &input.default_target);
            add(&mut patch, "defaultChatType", &input.default_target_type);
            add(&mut patch, "pollIntervalSecs", &input.poll_interval_secs);
        }
        "wecom_ai_bot" => {
            add(&mut patch, "botId", &input.bot_id);
            add(&mut patch, "defaultChatid", &input.default_target);
        }
        "dingtalk" => add(&mut patch, "clientId", &input.client_id),
        _ => return Err("CHANNEL_TYPE_UNSUPPORTED".to_string()),
    }
    add(&mut patch, "defaultAgentType", &input.default_agent_type);
    Ok(Value::Object(patch))
}

fn add<T: serde::Serialize>(
    map: &mut serde_json::Map<String, Value>,
    key: &str,
    value: &Option<T>,
) {
    if let Some(value) = value {
        map.insert(key.to_string(), json!(value));
    }
}
