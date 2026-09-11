use serde_json::{json, Value};

use crate::UpstreamError;

use super::model_settings::{ModelSelection, ModelSettings};
use super::session_options;

#[derive(Debug, Clone)]
pub(super) struct SessionSettings {
    permission_mode: String,
    collaboration_mode: String,
    model: ModelSettings,
    native_title: Option<String>,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            permission_mode: "agent".to_string(),
            collaboration_mode: "default".to_string(),
            model: ModelSettings::default(),
            native_title: None,
        }
    }
}

impl SessionSettings {
    pub(super) fn title_model(&self) -> Option<String> { self.model.current.clone() }
    pub(super) fn native_title(&self) -> Option<&str> { self.native_title.as_deref() }
    pub(super) fn fork_values(&self) -> Value {
        let mut values = self.model.current_values();
        values["collaborationMode"] = json!({ "mode": self.collaboration_mode, "settings": {
            "model": self.model.current, "reasoning_effort": self.model.effort, "developer_instructions": null,
        } });
        values
    }
    pub(super) fn capture(&mut self, response: &Value) {
        if let Some(thread) = response.get("thread") {
            self.native_title = thread.get("name").and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty()).map(str::to_string);
        }
        self.permission_mode = permission_mode_from_response(response).to_string();
        self.model.capture(response);
        if let Some(mode) = response
            .pointer("/collaborationMode/mode")
            .and_then(Value::as_str)
        {
            self.collaboration_mode = mode.to_string();
        }
    }

    pub(super) async fn load_models(
        &mut self,
        upstream: &crate::UpstreamClient,
    ) -> Result<(), UpstreamError> {
        self.model.load(upstream).await
    }

    pub(super) fn prompt_capabilities(
        &self,
        capabilities: crate::CapabilitySet,
    ) -> crate::CapabilitySet {
        if self.model.accepts_images() {
            capabilities
        } else {
            capabilities.without(crate::Capability::Images)
        }
    }

    pub(super) fn apply(&mut self, change: SettingsChange) {
        match change {
            SettingsChange::Permission(mode) => self.permission_mode = mode,
            SettingsChange::Collaboration(mode) => self.collaboration_mode = mode,
            SettingsChange::Model(selection) => self.model.apply(selection),
        }
    }
}

pub(super) enum SettingsChange {
    Permission(String),
    Collaboration(String),
    Model(ModelSelection),
}

pub(super) fn new_session_response(id: &str, settings: &SessionSettings) -> Value {
    json!({
        "sessionId": id,
        "modes": session_options::mode_state(&settings.permission_mode),
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
    if method == "session/set_model" {
        let model = required_string(params, "modelId")?;
        return settings
            .model
            .request(&thread_id, ("model", &model))
            .map(|(request, selection)| (request, SettingsChange::Model(selection)));
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
        "model" | "reasoning_effort" | "fast-mode" => settings
            .model
            .request(&thread_id, (&config_id, &value))
            .map(|(request, selection)| (request, SettingsChange::Model(selection))),
        _ => Err(UpstreamError::InvalidRequest(format!(
            "unsupported Codex config option: {config_id}"
        ))),
    }
}

pub(super) fn response(method: &str, settings: &SessionSettings) -> Value {
    if matches!(method, "session/set_mode" | "session/set_model") {
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
    let model = settings.model.current.clone().ok_or_else(|| {
        UpstreamError::InvalidResponse("Codex session response has no active model".into())
    })?;
    let effort = if mode == "plan" {
        Some(Value::String("medium".to_string()))
    } else {
        settings.model.effort.clone()
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
        _ => match response
            .pointer("/sandboxPolicy/type")
            .or_else(|| response.pointer("/sandbox/type"))
            .or_else(|| response.get("sandbox"))
            .and_then(Value::as_str)
        {
            Some("readOnly" | "read-only") => "read-only",
            Some("dangerFullAccess" | "danger-full-access") => "agent-full-access",
            _ => "agent",
        },
    }
}

pub(super) fn config_options(settings: &SessionSettings) -> Vec<Value> {
    session_options::config_options(
        &settings.permission_mode,
        &settings.collaboration_mode,
        settings.model.options(),
    )
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
