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
        _ => Err("CHANNEL_TYPE_UNSUPPORTED".to_string()),
    }
}

pub(super) fn update_config(
    channel_type: &str,
    input: &ChannelConfigInput,
) -> Result<Value, String> {
    let mut patch = serde_json::Map::new();
    add(&mut patch, "baseUrl", &input.base_url);
    add(&mut patch, "appId", &input.app_id);
    add(&mut patch, "defaultAgentType", &input.default_agent_type);
    add(&mut patch, "pollIntervalSecs", &input.poll_interval_secs);
    add(&mut patch, "defaultChatType", &input.default_target_type);
    if let Some(target) = &input.default_target {
        let key = if channel_type == "wecom" {
            "defaultChatid"
        } else {
            "chatId"
        };
        patch.insert(key.to_string(), json!(target));
    }
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
