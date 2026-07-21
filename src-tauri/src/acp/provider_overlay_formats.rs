use crate::models::agent::AgentType;

pub const MANAGED_PROVIDER_ID: &str = "iyw-claw";
/// Seed catalog: the compiled-in fallback used until the first successful
/// online `/v1/models` fetch (see `acp::model_catalog`). Order matters — it
/// is the catalog order, and each agent's default model derives from it.
pub const MANAGED_MODEL_IDS: [&str; 7] = [
    "gpt-5.4",
    "claude-opus-4-6",
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "doubao-seed-2-1-pro-260628",
    "gemini-3.1-pro-preview",
    "qwen3.7-max",
];
pub const MANAGED_DEFAULT_MODEL: &str = MANAGED_MODEL_IDS[0];

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
    let model_ids = managed_model_ids_for(AgentType::KimiCode);
    let default_model = managed_default_model_for(AgentType::KimiCode);
    let mut value = parse_toml_root(raw)?;
    let root = value
        .as_table_mut()
        .ok_or("kimi config root must be a TOML table")?;
    root.insert(
        "default_model".into(),
        toml::Value::String(default_model.into()),
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
        .filter(|model| model_ids.contains(model))
        .unwrap_or(default_model)
        .to_string();
    table_entry(root, "models")?.insert("default".into(), toml::Value::String(selected_model));
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
            set_json(root, &["env"], "ANTHROPIC_BASE_URL", base_url);
            set_json(root, &["env"], "ANTHROPIC_MODEL", default_model);
            set_json(root, &["env"], "ANTHROPIC_DEFAULT_OPUS_MODEL", model_ids[0]);
            set_json(
                root,
                &["env"],
                "ANTHROPIC_DEFAULT_SONNET_MODEL",
                model_ids.get(1).copied().unwrap_or(model_ids[0]),
            );
            set_json(
                root,
                &["env"],
                "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                model_ids.get(1).copied().unwrap_or(model_ids[0]),
            );
        }
        AgentType::CodeBuddy => {
            let env = ensure_json_object(root, &["env"]);
            env.retain(|key, _| !is_codebuddy_conflicting_env_key(key));
            env.insert(
                "CODEBUDDY_BASE_URL".into(),
                serde_json::Value::String(base_url.into()),
            );
            env.insert(
                "CODEBUDDY_MODEL".into(),
                serde_json::Value::String(model_ids[0].into()),
            );
            env.insert(
                "CODEBUDDY_BIG_SLOW_MODEL".into(),
                serde_json::Value::String(model_ids[0].into()),
            );
            env.insert(
                "CODEBUDDY_SMALL_FAST_MODEL".into(),
                serde_json::Value::String(model_ids[1].into()),
            );
            env.insert(
                "CODEBUDDY_CODE_SUBAGENT_MODEL".into(),
                serde_json::Value::String(model_ids[1].into()),
            );
        }
        AgentType::OpenCode => {
            let providers = ensure_json_object(root, &["provider"]);
            providers.retain(|name, _| name == MANAGED_PROVIDER_ID);
            let provider = ensure_json_object(providers, &[MANAGED_PROVIDER_ID]);
            let options = ensure_json_object(provider, &["options"]);
            options.insert("baseURL".into(), serde_json::Value::String(base_url.into()));
            provider.insert("models".into(), managed_model_object(&model_ids));
            root.insert(
                "model".into(),
                serde_json::Value::String(format!("{MANAGED_PROVIDER_ID}/{default_model}")),
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
                serde_json::Value::String(default_model.into()),
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
                serde_json::Value::String(default_model.into()),
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
        Value::String(managed_default_model_for(AgentType::Hermes).into()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_overlay_is_a_noop() {
        let original = serde_json::json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://gemini.example",
                "GEMINI_MODEL": "gemini-native"
            },
            "theme": "custom"
        });
        let patched = patch_json_config(
            AgentType::Gemini,
            original.clone(),
            "https://gateway.example/v1",
        )
        .expect("Gemini remains untouched");

        assert_eq!(patched, original);
    }

    #[test]
    fn claude_gateway_overlay_uses_messages_base_and_managed_models() {
        let base_url = "https://gateway.iyw.cn/iyw-fusion-api/anthropic";
        let patched =
            patch_json_config(AgentType::ClaudeCode, serde_json::json!({}), base_url).unwrap();
        assert_eq!(patched["env"]["ANTHROPIC_BASE_URL"], base_url);
        let models = managed_model_ids_for(AgentType::ClaudeCode);
        assert_eq!(patched["env"]["ANTHROPIC_MODEL"], models[0]);
        assert_eq!(patched["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], models[1]);
    }

    #[test]
    fn codebuddy_overlay_uses_openai_base_and_clears_anthropic_env() {
        let raw = serde_json::json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://old.example/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "old-token",
                "ANTHROPIC_MODEL": "old-model",
                "ANTHROPIC_FUTURE_OPTION": "must-not-leak",
                "CODEBUDDY_INTERNET_ENVIRONMENT": "internal",
                "CODEBUDDY_AUTH_TOKEN": "old-oauth",
                "KEEP": "custom"
            },
            "theme": "custom"
        });
        let base_url = "https://gateway.iyw.cn/iyw-fusion-api/v1";
        let patched = patch_json_config(AgentType::CodeBuddy, raw, base_url).unwrap();
        let env = patched["env"].as_object().expect("env object");
        let models = managed_model_ids_for(AgentType::CodeBuddy);
        assert_eq!(
            env.get("CODEBUDDY_BASE_URL").and_then(|v| v.as_str()),
            Some(base_url)
        );
        assert_eq!(
            env.get("CODEBUDDY_MODEL").and_then(|v| v.as_str()),
            Some(models[0])
        );
        assert_eq!(
            env.get("CODEBUDDY_BIG_SLOW_MODEL").and_then(|v| v.as_str()),
            Some(models[0])
        );
        assert_eq!(
            env.get("CODEBUDDY_SMALL_FAST_MODEL")
                .and_then(|v| v.as_str()),
            Some(models[1])
        );
        assert_eq!(
            env.get("CODEBUDDY_CODE_SUBAGENT_MODEL")
                .and_then(|v| v.as_str()),
            Some(models[1])
        );
        assert_eq!(env.get("KEEP").and_then(|v| v.as_str()), Some("custom"));
        assert!(!env.keys().any(|key| key.starts_with("ANTHROPIC_")));
        assert!(!env.contains_key("CODEBUDDY_INTERNET_ENVIRONMENT"));
        assert!(!env.contains_key("CODEBUDDY_AUTH_TOKEN"));
    }

    #[test]
    fn responses_gateway_overlays_declare_the_responses_wire_api() {
        let base_url = "https://gateway.iyw.cn/iyw-fusion-api/v1";
        let codex: toml::Value = patch_codex_toml("", base_url).unwrap().parse().unwrap();
        assert_eq!(
            codex["model_providers"][MANAGED_PROVIDER_ID]["base_url"].as_str(),
            Some(base_url)
        );
        assert_eq!(
            codex["model_providers"][MANAGED_PROVIDER_ID]["wire_api"].as_str(),
            Some("responses")
        );

        let openclaw =
            patch_json_config(AgentType::OpenClaw, serde_json::json!({}), base_url).unwrap();
        assert_eq!(
            openclaw["models"]["providers"][MANAGED_PROVIDER_ID]["api"],
            "openai-responses"
        );
        let pi = patch_pi_models_json(serde_json::json!({}), base_url, None).unwrap();
        assert_eq!(
            pi["providers"][MANAGED_PROVIDER_ID]["api"],
            "openai-responses"
        );
    }

    #[test]
    fn openai_compatible_gateway_overlays_use_v1_and_managed_models() {
        let base_url = "https://gateway.iyw.cn/iyw-fusion-api/v1";
        let grok: toml::Value = patch_grok_toml("", base_url).unwrap().parse().unwrap();
        assert_eq!(
            grok["model"][MANAGED_DEFAULT_MODEL]["base_url"].as_str(),
            Some(base_url)
        );
        assert_eq!(
            grok["model"][MANAGED_DEFAULT_MODEL]["api_backend"].as_str(),
            Some("chat_completions")
        );

        let kimi: toml::Value = patch_kimi_toml("", base_url).unwrap().parse().unwrap();
        assert_eq!(
            kimi["providers"][MANAGED_PROVIDER_ID]["base_url"].as_str(),
            Some(base_url)
        );
        let hermes: serde_yaml::Value =
            serde_yaml::from_str(&patch_hermes_yaml("", base_url).unwrap()).unwrap();
        assert_eq!(hermes["model"]["base_url"].as_str(), Some(base_url));
    }

    #[test]
    fn grok_overlay_writes_managed_models_and_preserves_unrelated_sections() {
        let raw = "[ui]\npermission_mode = \"acceptEdits\"\n\n\
                   [model.custom]\nmodel = \"custom\"\nbase_url = \"https://old/v1\"\n\n\
                   [models]\ndefault = \"custom\"\n\n\
                   [mcp_servers.files]\ncommand = \"npx\"\n";
        let patched = patch_grok_toml(raw, "https://gateway.example/v1").expect("overlay");
        let root: toml::Value = patched.parse().expect("TOML");
        assert_eq!(root["ui"]["permission_mode"].as_str(), Some("acceptEdits"));
        assert_eq!(
            root["mcp_servers"]["files"]["command"].as_str(),
            Some("npx")
        );
        assert_eq!(
            root["models"]["default"].as_str(),
            Some(MANAGED_DEFAULT_MODEL)
        );
        let models = root["model"].as_table().expect("managed model table");
        assert_eq!(models.len(), MANAGED_MODEL_IDS.len());
        for model_id in MANAGED_MODEL_IDS {
            let model = models[model_id].as_table().expect("model entry");
            assert_eq!(model["model"].as_str(), Some(model_id));
            assert_eq!(
                model["base_url"].as_str(),
                Some("https://gateway.example/v1")
            );
            assert_eq!(model["api_backend"].as_str(), Some("chat_completions"));
            assert_eq!(model["context_window"].as_integer(), Some(1_000_000));
            assert!(!model.contains_key("api_key"));
        }
    }
}
