use crate::models::agent::AgentType;

pub const MANAGED_PROVIDER_ID: &str = "iyw-claw";
pub const MANAGED_MODEL_IDS: [&str; 3] = [
    "deepseek-v4-pro",
    "doubao-seed-2-1-pro-260628",
    "deepseek-v4-flash",
];
pub const MANAGED_DEFAULT_MODEL: &str = MANAGED_MODEL_IDS[0];

pub(crate) fn patch_codex_toml(raw: &str, base_url: &str) -> Result<String, String> {
    let mut value = parse_toml_root(raw)?;
    let root = value
        .as_table_mut()
        .ok_or("codex config root must be a TOML table")?;
    root.insert(
        "model_provider".into(),
        toml::Value::String(MANAGED_PROVIDER_ID.into()),
    );
    let model = root
        .get("model")
        .and_then(toml::Value::as_str)
        .filter(|model| MANAGED_MODEL_IDS.contains(model))
        .unwrap_or(MANAGED_DEFAULT_MODEL)
        .to_string();
    root.insert("model".into(), toml::Value::String(model));

    let providers = table_entry(root, "model_providers")?;
    providers.retain(|name, _| name == MANAGED_PROVIDER_ID);
    let provider = table_entry(providers, MANAGED_PROVIDER_ID)?;
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
    root.insert(
        "default_model".into(),
        toml::Value::String(MANAGED_DEFAULT_MODEL.into()),
    );
    let providers = table_entry(root, "providers")?;
    providers.retain(|name, _| name == MANAGED_PROVIDER_ID);
    let provider = table_entry(providers, MANAGED_PROVIDER_ID)?;
    provider.insert(
        "type".into(),
        toml::Value::String("openai_compatible".into()),
    );
    provider.insert("base_url".into(), toml::Value::String(base_url.into()));
    let models = table_entry(root, "models")?;
    models.clear();
    for model_id in MANAGED_MODEL_IDS {
        let model = table_entry(models, model_id)?;
        model.insert(
            "provider".into(),
            toml::Value::String(MANAGED_PROVIDER_ID.into()),
        );
        model.insert("model".into(), toml::Value::String(model_id.into()));
        model.insert("max_context_size".into(), toml::Value::Integer(1_000_000));
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
            set_json(root, &["env"], "ANTHROPIC_BASE_URL", base_url);
            set_json(root, &["env"], "ANTHROPIC_MODEL", MANAGED_DEFAULT_MODEL);
            set_json(
                root,
                &["env"],
                "ANTHROPIC_DEFAULT_OPUS_MODEL",
                MANAGED_MODEL_IDS[0],
            );
            set_json(
                root,
                &["env"],
                "ANTHROPIC_DEFAULT_SONNET_MODEL",
                MANAGED_MODEL_IDS[1],
            );
            set_json(
                root,
                &["env"],
                "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                MANAGED_MODEL_IDS[2],
            );
        }
        AgentType::Gemini => {
            set_json(root, &["env"], "GOOGLE_GEMINI_BASE_URL", base_url);
            set_json(root, &["env"], "GEMINI_MODEL", MANAGED_DEFAULT_MODEL);
        }
        AgentType::OpenCode => {
            let providers = ensure_json_object(root, &["provider"]);
            providers.retain(|name, _| name == MANAGED_PROVIDER_ID);
            let provider = ensure_json_object(providers, &[MANAGED_PROVIDER_ID]);
            let options = ensure_json_object(provider, &["options"]);
            options.insert("baseURL".into(), serde_json::Value::String(base_url.into()));
            provider.insert("models".into(), managed_model_object());
            root.insert(
                "model".into(),
                serde_json::Value::String(format!("{MANAGED_PROVIDER_ID}/{MANAGED_DEFAULT_MODEL}")),
            );
        }
        AgentType::OpenClaw => {
            let providers = ensure_json_object(root, &["models", "providers"]);
            providers.retain(|name, _| name == MANAGED_PROVIDER_ID);
            let provider = ensure_json_object(providers, &[MANAGED_PROVIDER_ID]);
            provider.insert("baseUrl".into(), serde_json::Value::String(base_url.into()));
            provider.insert(
                "api".into(),
                serde_json::Value::String("openai-responses".into()),
            );
            provider.insert("models".into(), managed_model_array());
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
            root.insert(
                "openAiModelId".into(),
                serde_json::Value::String(MANAGED_DEFAULT_MODEL.into()),
            );
            root.insert("welcomeViewCompleted".into(), serde_json::Value::Bool(true));
        }
        AgentType::Pi => {
            root.insert(
                "defaultProvider".into(),
                serde_json::Value::String(MANAGED_PROVIDER_ID.into()),
            );
            root.insert(
                "defaultModel".into(),
                serde_json::Value::String(MANAGED_DEFAULT_MODEL.into()),
            );
        }
        _ => return Err(format!("no JSON provider overlay for {agent:?}")),
    }
    Ok(value)
}

pub(crate) fn patch_pi_models_json(
    mut value: serde_json::Value,
    base_url: &str,
    _model: Option<&str>,
) -> Result<serde_json::Value, String> {
    let root = value
        .as_object_mut()
        .ok_or("pi models root must be a JSON object")?;
    let providers = ensure_json_object(root, &["providers"]);
    providers.retain(|name, _| name == MANAGED_PROVIDER_ID);
    let provider = ensure_json_object(providers, &[MANAGED_PROVIDER_ID]);
    provider.insert("baseUrl".into(), serde_json::Value::String(base_url.into()));
    provider.insert(
        "api".into(),
        serde_json::Value::String("openai-responses".into()),
    );
    provider.insert("models".into(), managed_model_array());
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
    model.insert(
        Value::String("default".into()),
        Value::String(MANAGED_DEFAULT_MODEL.into()),
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
    ensure_json_object(root, path).insert(key.into(), serde_json::Value::String(value.into()));
}

fn ensure_json_object<'a>(
    root: &'a mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
) -> &'a mut serde_json::Map<String, serde_json::Value> {
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
    current
}

fn managed_model_object() -> serde_json::Value {
    serde_json::Value::Object(
        MANAGED_MODEL_IDS
            .iter()
            .map(|model| ((*model).to_string(), serde_json::json!({"name": model})))
            .collect(),
    )
}

fn managed_model_array() -> serde_json::Value {
    serde_json::Value::Array(
        MANAGED_MODEL_IDS
            .iter()
            .map(|model| serde_json::json!({"id": model, "name": model}))
            .collect(),
    )
}
