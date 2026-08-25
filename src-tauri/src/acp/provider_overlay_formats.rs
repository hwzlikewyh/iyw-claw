use crate::models::agent::AgentType;

pub const MANAGED_PROVIDER_ID: &str = "iyw-claw";
/// Seed catalog: the compiled-in fallback used until the first successful
/// online `/v1/models` fetch (see `acp::model_catalog`). Order matters — it
/// is the catalog order, and each agent's default model derives from it.
pub const MANAGED_MODEL_IDS: [&str; 9] = [
    "claude-fable-5",
    "gpt-5.6-sol",
    "gpt-5.6-luna",
    "gpt-5.6-terra",
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "doubao-seed-2-1-pro-260628",
    "gemini-3.1-pro-preview",
    "qwen3.7-max",
];
pub const MANAGED_DEFAULT_MODEL: &str = MANAGED_MODEL_IDS[0];

fn selected_model_or_default(
    value: Option<&str>,
    model_ids: &[&str],
    default_model: &str,
) -> String {
    value
        .map(str::trim)
        .filter(|model| model_ids.contains(model))
        .unwrap_or(default_model)
        .to_string()
}

fn selected_provider_model_or_default(
    value: Option<&str>,
    model_ids: &[&str],
    default_model: &str,
) -> String {
    let raw = value.map(str::trim).unwrap_or_default();
    let model = raw.strip_prefix(&format!("{MANAGED_PROVIDER_ID}/"));
    selected_model_or_default(
        model.or((!raw.is_empty()).then_some(raw)),
        model_ids,
        default_model,
    )
}

pub fn managed_model_ids_for(agent: AgentType) -> Vec<&'static str> {
    crate::acp::model_catalog::model_ids_for(agent)
}

pub fn managed_default_model_for(agent: AgentType) -> &'static str {
    crate::acp::model_catalog::default_model_for(agent)
}

pub(crate) const CODEBUDDY_CONFLICTING_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_URL",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_REASONING_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_CUSTOM_MODEL_OPTION",
    "ANTHROPIC_CUSTOM_MODEL_OPTION_NAME",
    "ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION",
    "CODEBUDDY_AUTH_TOKEN",
    "CODEBUDDY_INTERNET_ENVIRONMENT",
];

pub(crate) fn is_codebuddy_conflicting_env_key(key: &str) -> bool {
    const ANTHROPIC_PREFIX: &str = "ANTHROPIC_";
    key.get(..ANTHROPIC_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(ANTHROPIC_PREFIX))
        || CODEBUDDY_CONFLICTING_ENV_KEYS
            .iter()
            .any(|candidate| key.eq_ignore_ascii_case(candidate))
}

pub(crate) fn patch_codex_toml(raw: &str, base_url: &str) -> Result<String, String> {
    let model_ids = managed_model_ids_for(AgentType::Codex);
    let default_model = managed_default_model_for(AgentType::Codex);
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
        .filter(|model| model_ids.contains(model))
        .unwrap_or(default_model)
        .to_string();
    root.insert("model".into(), toml::Value::String(model));
    root.remove("request_max_retries");
    root.remove("stream_max_retries");

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
    provider.insert("request_max_retries".into(), toml::Value::Integer(10));
    provider.insert("stream_max_retries".into(), toml::Value::Integer(10));
    toml::to_string_pretty(&value).map_err(|error| error.to_string())
}

pub(crate) fn patch_kimi_toml(raw: &str, base_url: &str) -> Result<String, String> {
    let model_ids = managed_model_ids_for(AgentType::KimiCode);
    let default_model = managed_default_model_for(AgentType::KimiCode);
    let mut value = parse_toml_root(raw)?;
    let root = value
        .as_table_mut()
        .ok_or("kimi config root must be a TOML table")?;
    let selected_model = selected_model_or_default(
        root.get("default_model").and_then(toml::Value::as_str),
        &model_ids,
        default_model,
    );
    root.insert("default_model".into(), toml::Value::String(selected_model));
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
    for model_id in model_ids {
        let model = table_entry(models, model_id)?;
        model.insert(
            "provider".into(),
            toml::Value::String(MANAGED_PROVIDER_ID.into()),
        );
        model.insert("model".into(), toml::Value::String((*model_id).into()));
        model.insert("max_context_size".into(), toml::Value::Integer(1_000_000));
    }
    toml::to_string_pretty(&value).map_err(|error| error.to_string())
}

pub(crate) fn patch_grok_toml(raw: &str, base_url: &str) -> Result<String, String> {
    let model_ids = managed_model_ids_for(AgentType::Grok);
    let default_model = managed_default_model_for(AgentType::Grok);
    let mut value = parse_toml_root(raw)?;
    let root = value
        .as_table_mut()
        .ok_or("grok config root must be a TOML table")?;
    let selected_model = root
        .get("models")
        .and_then(toml::Value::as_table)
        .and_then(|models| models.get("default"))
        .and_then(toml::Value::as_str)
        .map(|model| selected_model_or_default(Some(model), &model_ids, default_model))
        .unwrap_or_else(|| default_model.to_string());
    {
        let models_table = table_entry(root, "models")?;
        models_table.insert("default".into(), toml::Value::String(selected_model));
        models_table.insert("max_retries".into(), toml::Value::Integer(10));
    }
    let models = table_entry(root, "model")?;
    models.clear();
    for model_id in model_ids {
        let model = table_entry(models, model_id)?;
        model.insert("model".into(), toml::Value::String((*model_id).into()));
        model.insert("base_url".into(), toml::Value::String(base_url.into()));
        model.insert(
            "api_backend".into(),
            toml::Value::String("chat_completions".into()),
        );
        model.insert("context_window".into(), toml::Value::Integer(1_000_000));
    }
    toml::to_string_pretty(&value).map_err(|error| error.to_string())
}

pub(crate) fn patch_json_config(
    agent: AgentType,
    mut value: serde_json::Value,
    base_url: &str,
) -> Result<serde_json::Value, String> {
    if agent == AgentType::Gemini {
        return Ok(value);
    }
    let root = value
        .as_object_mut()
        .ok_or("agent config root must be a JSON object")?;
    let model_ids = managed_model_ids_for(agent);
    let default_model = managed_default_model_for(agent);
    match agent {
        AgentType::ClaudeCode => {
            let existing = root
                .get("env")
                .and_then(serde_json::Value::as_object)
                .cloned()
                .unwrap_or_default();
            set_json(root, &["env"], "ANTHROPIC_BASE_URL", base_url);
            let model = selected_model_or_default(
                existing
                    .get("ANTHROPIC_MODEL")
                    .and_then(serde_json::Value::as_str),
                &model_ids,
                default_model,
            );
            set_json(root, &["env"], "ANTHROPIC_MODEL", &model);
            set_json(root, &["env"], "ANTHROPIC_MAX_RETRIES", "10");
            let opus = selected_model_or_default(
                existing
                    .get("ANTHROPIC_DEFAULT_OPUS_MODEL")
                    .and_then(serde_json::Value::as_str),
                &model_ids,
                default_model,
            );
            set_json(root, &["env"], "ANTHROPIC_DEFAULT_OPUS_MODEL", &opus);
            let sonnet = selected_model_or_default(
                existing
                    .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                    .and_then(serde_json::Value::as_str),
                &model_ids,
                default_model,
            );
            set_json(root, &["env"], "ANTHROPIC_DEFAULT_SONNET_MODEL", &sonnet);
            let haiku = selected_model_or_default(
                existing
                    .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
                    .and_then(serde_json::Value::as_str),
                &model_ids,
                default_model,
            );
            set_json(root, &["env"], "ANTHROPIC_DEFAULT_HAIKU_MODEL", &haiku);
        }
        AgentType::CodeBuddy => {
            let existing = root
                .get("env")
                .and_then(serde_json::Value::as_object)
                .cloned()
                .unwrap_or_default();
            let env = ensure_json_object(root, &["env"]);
            env.retain(|key, _| !is_codebuddy_conflicting_env_key(key));
            env.insert(
                "CODEBUDDY_BASE_URL".into(),
                serde_json::Value::String(base_url.into()),
            );
            let primary = selected_model_or_default(
                existing
                    .get("CODEBUDDY_MODEL")
                    .and_then(serde_json::Value::as_str),
                &model_ids,
                default_model,
            );
            let secondary = selected_model_or_default(
                existing
                    .get("CODEBUDDY_SMALL_FAST_MODEL")
                    .and_then(serde_json::Value::as_str),
                &model_ids,
                model_ids.get(1).copied().unwrap_or(default_model),
            );
            env.insert(
                "CODEBUDDY_MODEL".into(),
                serde_json::Value::String(primary.clone()),
            );
            env.insert(
                "CODEBUDDY_BIG_SLOW_MODEL".into(),
                serde_json::Value::String(primary),
            );
            env.insert(
                "CODEBUDDY_SMALL_FAST_MODEL".into(),
                serde_json::Value::String(secondary.clone()),
            );
            env.insert(
                "CODEBUDDY_CODE_SUBAGENT_MODEL".into(),
                serde_json::Value::String(secondary),
            );
        }
        AgentType::OpenCode => {
            let providers = ensure_json_object(root, &["provider"]);
            providers.retain(|name, _| name == MANAGED_PROVIDER_ID);
            let provider = ensure_json_object(providers, &[MANAGED_PROVIDER_ID]);
            let options = ensure_json_object(provider, &["options"]);
            options.insert("baseURL".into(), serde_json::Value::String(base_url.into()));
            provider.insert("models".into(), managed_model_object(&model_ids));
            let selected = selected_provider_model_or_default(
                root.get("model").and_then(serde_json::Value::as_str),
                &model_ids,
                default_model,
            );
            root.insert(
                "model".into(),
                serde_json::Value::String(format!("{MANAGED_PROVIDER_ID}/{selected}")),
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
            provider.insert("models".into(), managed_model_array(&model_ids));
        }
        AgentType::Cline => {
            let selected = selected_model_or_default(
                root.get("openAiModelId")
                    .and_then(serde_json::Value::as_str),
                &model_ids,
                default_model,
            );
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
            root.insert("openAiModelId".into(), serde_json::Value::String(selected));
            root.insert("welcomeViewCompleted".into(), serde_json::Value::Bool(true));
        }
        AgentType::Pi => {
            let selected = selected_model_or_default(
                root.get("defaultModel").and_then(serde_json::Value::as_str),
                &model_ids,
                default_model,
            );
            root.insert(
                "defaultProvider".into(),
                serde_json::Value::String(MANAGED_PROVIDER_ID.into()),
            );
            root.insert("defaultModel".into(), serde_json::Value::String(selected));
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
    provider.insert(
        "models".into(),
        managed_model_array(&managed_model_ids_for(AgentType::Pi)),
    );
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
    let existing_default = map
        .get(Value::String("model".into()))
        .and_then(Value::as_mapping)
        .and_then(|model| model.get(Value::String("default".into())))
        .and_then(Value::as_str);
    let model_ids = managed_model_ids_for(AgentType::Hermes);
    let default_model = managed_default_model_for(AgentType::Hermes);
    let selected_model = selected_model_or_default(existing_default, &model_ids, default_model);
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
        Value::String(selected_model),
    );
    // Inject retry limit into the `agent` section
    let agent_entry = map
        .entry(Value::String("agent".into()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if !agent_entry.is_mapping() {
        *agent_entry = Value::Mapping(Mapping::new());
    }
    let agent_map = agent_entry
        .as_mapping_mut()
        .ok_or("hermes agent must be a YAML mapping")?;
    agent_map.insert(
        Value::String("api_max_retries".into()),
        Value::Number(serde_yaml::Number::from(10_i64)),
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

fn managed_model_object(model_ids: &[&str]) -> serde_json::Value {
    serde_json::Value::Object(
        model_ids
            .iter()
            .map(|model| ((*model).to_string(), serde_json::json!({"name": model})))
            .collect(),
    )
}

fn managed_model_array(model_ids: &[&str]) -> serde_json::Value {
    serde_json::Value::Array(
        model_ids
            .iter()
            .map(|model| serde_json::json!({"id": model, "name": model}))
            .collect(),
    )
}
