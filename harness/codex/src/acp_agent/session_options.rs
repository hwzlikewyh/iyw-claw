use serde_json::{json, Value};

pub(super) fn config_options(
    permission: &str,
    collaboration: &str,
    model: Vec<Value>,
) -> Vec<Value> {
    let mut options = vec![
        json!({
            "id": "mode",
            "name": "Approval Preset",
            "category": "mode",
            "type": "select",
            "currentValue": permission,
            "options": [
                select_option("read-only", "Read-only"),
                select_option("agent", "Agent"),
                select_option("agent-full-access", "Full access"),
            ],
        }),
        json!({
            "id": "collaboration_mode",
            "name": "Work mode",
            "category": "collaboration_mode",
            "type": "select",
            "currentValue": collaboration,
            "options": [
                select_option("default", "Default"),
                select_option("plan", "Plan"),
            ],
        }),
    ];
    options.extend(model);
    options
}

pub(super) fn mode_state(mode: &str) -> Value {
    json!({
        "currentModeId": mode,
        "availableModes": [
            mode_option("read-only", "Read-only"),
            mode_option("agent", "Agent"),
            mode_option("agent-full-access", "Full access"),
        ]
    })
}

fn mode_option(id: &str, name: &str) -> Value {
    json!({ "id": id, "name": name })
}

fn select_option(value: &str, name: &str) -> Value {
    json!({ "value": value, "name": name })
}
