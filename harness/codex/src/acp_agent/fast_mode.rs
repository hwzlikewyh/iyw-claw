use serde_json::{json, Value};

pub(super) fn supported(model: Option<&Value>) -> bool {
    model.is_some_and(|model| {
        model
            .get("serviceTiers")
            .and_then(Value::as_array)
            .is_some_and(|tiers| tiers.iter().any(|tier| tier["id"] == "fast"))
            || model
                .get("additionalSpeedTiers")
                .and_then(Value::as_array)
                .is_some_and(|tiers| tiers.iter().any(|tier| tier == "fast"))
    })
}

pub(super) fn option(enabled: bool) -> Value {
    json!({
        "id": "fast-mode", "name": "快速模式", "category": "model_config", "type": "select",
        "description": "更快响应，会增加用量", "currentValue": if enabled { "on" } else { "off" },
        "options": [
            { "value": "off", "name": "关闭" },
            { "value": "on", "name": "开启" },
        ],
    })
}
