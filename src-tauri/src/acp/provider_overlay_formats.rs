use crate::models::agent::AgentType;

const MANAGED_PROVIDER_ID: &str = "iyw-claw";

pub(crate) fn patch_codex_toml(raw: &str, base_url: &str) -> Result<String, String> {
    let mut value = parse_toml_root(raw)?;
    let root = value
        .as_table_mut()
        .ok_or("codex config root must be a TOML table")?;
    root.insert(
        "model_provider".into(),
        toml::Value::String(MANAGED_PROVIDER_ID.into()),
    );
    let provider = table_entry(table_entry(root, "model_providers")?, MANAGED_PROVIDER_ID)?;
    provider.insert(
        "name".into(),
        toml::Value::String(MANAGED_PROVIDER_ID.into()),
    );
    provider.insert("base_url".into(), toml::Value::String(base_url.into()));
    provider.insert("wire_api".into(), toml::Value::String("responses".into()));
    provider.insert("requires_openai_auth".into(), toml::Value::Boolean(true));
    toml::to_string_pretty(&value).map_err(|error| error.to_string())
}

pub(crate) fn patch_kimi_toml(raw: &str, base_url: &str) -> Result<String, String> {
    let mut value = parse_toml_root(raw)?;
    let root = value
        .as_table_mut()
        .ok_or("kimi config root must be a TOML table")?;
    let alias = root
        .get("default_model")
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            root.get("models")
                .and_then(toml::Value::as_table)
                .and_then(|v| v.keys().next().cloned())
        });
    let provider = table_entry(table_entry(root, "providers")?, MANAGED_PROVIDER_ID)?;
    provider.insert(
        "type".into(),
        toml::Value::String("openai_compatible".into()),
    );
    provider.insert("base_url".into(), toml::Value::String(base_url.into()));
    if let Some(alias) = alias {
        if let Some(model) = root
            .get_mut("models")
            .and_then(toml::Value::as_table_mut)
            .and_then(|v| v.get_mut(&alias))
            .and_then(toml::Value::as_table_mut)
        {
            model.insert(
                "provider".into(),
                toml::Value::String(MANAGED_PROVIDER_ID.into()),
            );
        }
    }
    toml::to_string_pretty(&value).map_err(|error| error.to_string())
}

pub(crate) fn patch_json_config(
    agent: AgentType,
    mut value: serde_json::Value,
    base_url: &str,
) -> Result<serde_json::Value, String> {
    let root = value
        .as_object_mut()
        .ok_or("agent config root must be a JSON object")?;
    match agent {
        AgentType::ClaudeCode | AgentType::CodeBuddy => {
            set_json(root, &["env"], "ANTHROPIC_BASE_URL", base_url)
        }
        AgentType::Gemini => set_json(root, &["env"], "GOOGLE_GEMINI_BASE_URL", base_url),
        AgentType::OpenCode => set_json(
            root,
            &["provider", MANAGED_PROVIDER_ID, "options"],
            "baseURL",
            base_url,
        ),
        AgentType::OpenClaw => {
            set_json(
                root,
                &["models", "providers", MANAGED_PROVIDER_ID],
                "baseUrl",
                base_url,
            );
            set_json(
                root,
                &["models", "providers", MANAGED_PROVIDER_ID],
                "api",
                "openai-responses",
            );
        }
        AgentType::Cline => {
            root.insert(
                "actModeApiProvider".into(),
                serde_json::Value::String("openai".into()),
            );
            root.insert(
                "planModeApiProvider".into(),
                serde_json::Value::String("openai".into()),
            );
            root.insert(
                "openAiBaseUrl".into(),
                serde_json::Value::String(base_url.into()),
            );
            root.insert("welcomeViewCompleted".into(), serde_json::Value::Bool(true));
        }
        AgentType::Pi => {
            root.insert(
                "defaultProvider".into(),
                serde_json::Value::String(MANAGED_PROVIDER_ID.into()),
            );
        }
        _ => return Err(format!("no JSON provider overlay for {agent:?}")),
    }
    Ok(value)
}

pub(crate) fn patch_pi_models_json(
    mut value: serde_json::Value,
    base_url: &str,
    model: Option<&str>,
) -> Result<serde_json::Value, String> {
    let root = value
        .as_object_mut()
        .ok_or("pi models root must be a JSON object")?;
    set_json(
        root,
        &["providers", MANAGED_PROVIDER_ID],
        "baseUrl",
        base_url,
    );
    set_json(
        root,
        &["providers", MANAGED_PROVIDER_ID],
        "api",
        "openai-responses",
    );
    if let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) {
        let providers = root
            .get_mut("providers")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|providers| providers.get_mut(MANAGED_PROVIDER_ID))
            .and_then(serde_json::Value::as_object_mut)
            .expect("managed Pi provider inserted above");
        let models = providers
            .entry("models")
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        if !models.is_array() {
            *models = serde_json::Value::Array(Vec::new());
        }
        let models = models.as_array_mut().expect("models array ensured");
        if !models
            .iter()
            .any(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(model))
        {
            models.push(serde_json::json!({"id": model, "name": model}));
        }
    }
    Ok(value)
}

pub(crate) fn patch_hermes_yaml(raw: &str, base_url: &str) -> Result<String, String> {
    use serde_yaml::{Mapping, Value};
    let mut root = if raw.trim().is_empty() {
        Value::Mapping(Mapping::new())
    } else {
        serde_yaml::from_str(raw).map_err(|e| e.to_string())?
    };
    let map = root
        .as_mapping_mut()
        .ok_or("hermes config root must be a YAML mapping")?;
    let model = map
        .entry(Value::String("model".into()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if !model.is_mapping() {
        *model = Value::Mapping(Mapping::new());
    }
    let model = model
        .as_mapping_mut()
        .ok_or("hermes model must be a YAML mapping")?;
    model.insert(
        Value::String("provider".into()),
        Value::String("custom".into()),
    );
    model.insert(
        Value::String("base_url".into()),
        Value::String(base_url.into()),
    );
    serde_yaml::to_string(&root).map_err(|e| e.to_string())
}

fn parse_toml_root(raw: &str) -> Result<toml::Value, String> {
    if raw.trim().is_empty() {
        Ok(toml::Value::Table(toml::map::Map::new()))
    } else {
        raw.parse().map_err(|e: toml::de::Error| e.to_string())
    }
}

fn table_entry<'a>(
    table: &'a mut toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<&'a mut toml::map::Map<String, toml::Value>, String> {
    let value = table
        .entry(key)
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if !value.is_table() {
        *value = toml::Value::Table(toml::map::Map::new());
    }
    value
        .as_table_mut()
        .ok_or_else(|| format!("{key} must be a TOML table"))
}

fn set_json(
    root: &mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
    key: &str,
    value: &str,
) {
    let mut current = root;
    for segment in path {
        let entry = current
            .entry(*segment)
            .or_insert_with(|| serde_json::json!({}));
        if !entry.is_object() {
            *entry = serde_json::json!({});
        }
        current = entry.as_object_mut().expect("object ensured");
    }
    current.insert(key.into(), serde_json::Value::String(value.into()));
}
