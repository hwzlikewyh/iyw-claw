use serde_json::{json, Value};

use crate::UpstreamError;

#[derive(Debug, Clone)]
pub(super) struct SessionSettings {
    permission_mode: String,
    collaboration_mode: String,
    model: Option<String>,
    reasoning_effort: Option<Value>,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            permission_mode: "agent".to_string(),
            collaboration_mode: "default".to_string(),
            model: None,
            reasoning_effort: None,
        }
    }
}

impl SessionSettings {
    pub(super) fn capture(&mut self, response: &Value) {
        self.permission_mode = permission_mode_from_response(response).to_string();
        self.model = response
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        self.reasoning_effort = response
            .get("reasoningEffort")
            .filter(|value| !value.is_null())
            .cloned();
    }

    pub(super) fn apply(&mut self, change: SettingsChange) {
        match change {
            SettingsChange::Permission(mode) => self.permission_mode = mode,
            SettingsChange::Collaboration(mode) => self.collaboration_mode = mode,
        }
    }
}

pub(super) enum SettingsChange {
    Permission(String),
    Collaboration(String),
}

pub(super) fn new_session_response(id: &str, settings: &SessionSettings) -> Value {
    json!({
        "sessionId": id,
        "modes": mode_state(&settings.permission_mode),
        "configOptions": config_options(settings),
    })
}

pub(super) fn request(
    method: &str,
    params: &Value,
    settings: &SessionSettings,
) -> Result<(Value, SettingsChange), UpstreamError> {
    let thread_id = required_string(params, "sessionId")?;
    if method == "session/set_mode" {
        let mode = required_string(params, "modeId")?;
        return permission_request(thread_id, mode);
    }
    if method != "session/set_config_option" {
        return Err(UpstreamError::InvalidRequest(format!(
            "unsupported ACP settings method: {method}"
        )));
    }
    let config_id = required_string(params, "configId")?;
    let value = required_string(params, "value")?;
    match config_id.as_str() {
        "mode" => permission_request(thread_id, value),
        "collaboration_mode" => collaboration_request(thread_id, value, settings),
        _ => Err(UpstreamError::InvalidRequest(format!(
            "unsupported Codex config option: {config_id}"
        ))),
    }
}

pub(super) fn response(method: &str, settings: &SessionSettings) -> Value {
    if method == "session/set_mode" {
        json!({})
    } else {
        json!({ "configOptions": config_options(settings) })
    }
}

fn permission_request(
    thread_id: String,
    mode: String,
) -> Result<(Value, SettingsChange), UpstreamError> {
    let (permissions, approval_policy) = match mode.as_str() {
        "read-only" => (":read-only", "on-request"),
        "agent" => (":workspace", "on-request"),
        "agent-full-access" => (":danger-full-access", "never"),
        _ => {
            return Err(UpstreamError::InvalidRequest(format!(
                "unsupported Codex permission mode: {mode}"
            )))
        }
    };
    Ok((
        json!({
            "method": "thread/settings/update",
            "params": {
                "threadId": thread_id,
                "permissions": permissions,
                "approvalPolicy": approval_policy,
            }
        }),
        SettingsChange::Permission(mode),
    ))
}

fn collaboration_request(
    thread_id: String,
    mode: String,
    settings: &SessionSettings,
) -> Result<(Value, SettingsChange), UpstreamError> {
    if !matches!(mode.as_str(), "default" | "plan") {
        return Err(UpstreamError::InvalidRequest(format!(
            "unsupported Codex collaboration mode: {mode}"
        )));
    }
    let model = settings.model.clone().ok_or_else(|| {
        UpstreamError::InvalidResponse("Codex session response has no active model".into())
    })?;
    let effort = if mode == "plan" {
        Some(Value::String("medium".to_string()))
    } else {
        settings.reasoning_effort.clone()
    };
    Ok((
        json!({
            "method": "thread/settings/update",
            "params": {
                "threadId": thread_id,
                "collaborationMode": {
                    "mode": mode,
                    "settings": {
                        "model": model,
                        "reasoning_effort": effort,
                        "developer_instructions": Value::Null,
                    }
                }
            }
        }),
        SettingsChange::Collaboration(mode),
    ))
}

fn permission_mode_from_response(response: &Value) -> &'static str {
    match response
        .pointer("/activePermissionProfile/id")
        .and_then(Value::as_str)
    {
        Some(":read-only") => "read-only",
        Some(":danger-full-access") => "agent-full-access",
        Some(":workspace") => "agent",
        _ => match response.get("sandbox").and_then(Value::as_str) {
            Some("readOnly" | "read-only") => "read-only",
            Some("dangerFullAccess" | "danger-full-access") => "agent-full-access",
            _ => "agent",
        },
    }
}

fn config_options(settings: &SessionSettings) -> Vec<Value> {
    vec![
        json!({
            "id": "mode",
            "name": "Approval Preset",
            "category": "mode",
            "type": "select",
            "currentValue": settings.permission_mode,
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
            "currentValue": settings.collaboration_mode,
            "options": [
                select_option("default", "Default"),
                select_option("plan", "Plan"),
            ],
        }),
    ]
}

fn mode_state(mode: &str) -> Value {
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

fn required_string(params: &Value, field: &str) -> Result<String, UpstreamError> {
    params
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| UpstreamError::InvalidRequest(format!("ACP request has no {field}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_permission_modes_to_builtin_profiles() {
        let settings = SessionSettings::default();
        let cases = [
            ("read-only", ":read-only", "on-request"),
            ("agent", ":workspace", "on-request"),
            ("agent-full-access", ":danger-full-access", "never"),
        ];
        for (mode, profile, approval) in cases {
            let (request, _) = request(
                "session/set_mode",
                &json!({"sessionId": "thread", "modeId": mode}),
                &settings,
            )
            .expect("mode maps");
            assert_eq!(request["params"]["permissions"], profile);
            assert_eq!(request["params"]["approvalPolicy"], approval);
        }
    }

    #[test]
    fn maps_official_plan_preset_without_changing_model() {
        let mut settings = SessionSettings::default();
        settings.capture(&json!({"model": "gpt-test", "reasoningEffort": "high"}));
        let (request, change) = request(
            "session/set_config_option",
            &json!({
                "sessionId": "thread",
                "configId": "collaboration_mode",
                "value": "plan"
            }),
            &settings,
        )
        .expect("plan maps");
        assert_eq!(request["params"]["collaborationMode"]["mode"], "plan");
        assert_eq!(
            request["params"]["collaborationMode"]["settings"]["model"],
            "gpt-test"
        );
        assert_eq!(
            request["params"]["collaborationMode"]["settings"]["reasoning_effort"],
            "medium"
        );
        settings.apply(change);
        assert_eq!(
            response("session/set_config_option", &settings)["configOptions"][1]["currentValue"],
            "plan"
        );
    }
}
