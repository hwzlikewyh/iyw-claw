use crate::models::agent::AgentType;

use super::provider_overlay_formats::is_codebuddy_conflicting_env_key;

const MANAGED_PROVIDER_ID: &str = "iyw-claw";

pub(crate) fn patch_json_credential(
    agent: AgentType,
    raw: &str,
    token: Option<&str>,
) -> Result<String, String> {
    if agent == AgentType::Gemini {
        return Ok(raw.to_string());
    }
    let mut root = parse_json_object(raw)?;
    match agent {
        AgentType::ClaudeCode => {
            patch_nested_json(&mut root, &["env"], "ANTHROPIC_AUTH_TOKEN", token);
            patch_anthropic_custom_headers(&mut root, token);
        }
        AgentType::CodeBuddy => {
            if let Some(env) = existing_json_object(&mut root, &["env"]) {
                env.retain(|key, _| !is_codebuddy_conflicting_env_key(key));
            }
            patch_nested_json(&mut root, &["env"], "CODEBUDDY_API_KEY", token);
        }
        AgentType::OpenClaw => {
            patch_nested_json(
                &mut root,
                &["models", "providers", MANAGED_PROVIDER_ID],
                "apiKey",
                token,
            );
            patch_nested_json(
                &mut root,
                &["models", "providers", MANAGED_PROVIDER_ID, "headers"],
                "token",
                token,
            );
        }
        AgentType::OpenCode => patch_provider_auth(&mut root, "api", token),
        AgentType::Cline => patch_root_string(&mut root, "openAiApiKey", token),
        AgentType::Pi => patch_provider_auth(&mut root, "api_key", token),
        _ => return Err(format!("no JSON account credential format for {agent:?}")),
    }
    serde_json::to_string_pretty(&root)
        .map(|value| value + "\n")
        .map_err(|error| error.to_string())
}

pub(crate) fn patch_json_gateway_header(
    agent: AgentType,
    raw: &str,
    token: Option<&str>,
) -> Result<String, String> {
    let mut root = parse_json_object(raw)?;
    let path = match agent {
        AgentType::OpenCode => vec!["provider", MANAGED_PROVIDER_ID, "options", "headers"],
        AgentType::Pi => vec!["providers", MANAGED_PROVIDER_ID, "headers"],
        AgentType::Cline => vec!["openAiHeaders"],
        _ => return Err(format!("no JSON gateway header format for {agent:?}")),
    };
    patch_nested_json(&mut root, &path, "token", token);
    serde_json::to_string_pretty(&root)
        .map(|value| value + "\n")
        .map_err(|error| error.to_string())
}

pub(crate) fn patch_codex_auth_json(raw: &str, token: Option<&str>) -> Result<String, String> {
    let mut root = parse_json_object(raw)?;
    patch_root_string(&mut root, "OPENAI_API_KEY", token);
    serde_json::to_string_pretty(&root)
        .map(|value| value + "\n")
        .map_err(|error| error.to_string())
}

pub(crate) fn patch_toml_credential(
    agent: AgentType,
    raw: &str,
    token: Option<&str>,
) -> Result<String, String> {
    let mut value = parse_toml_root(raw)?;
    let root = value
        .as_table_mut()
        .ok_or("credential TOML root must be a table")?;
    match agent {
        AgentType::Codex => patch_nested_toml(
            root,
            &["model_providers", MANAGED_PROVIDER_ID, "http_headers"],
            "token",
            token,
        )?,
        AgentType::KimiCode => {
            patch_nested_toml(root, &["providers", MANAGED_PROVIDER_ID], "api_key", token)?;
            patch_nested_toml(
                root,
                &["providers", MANAGED_PROVIDER_ID, "custom_headers"],
                "token",
                token,
            )?;
        }
        _ => return Err(format!("no TOML account credential format for {agent:?}")),
    }
    toml::to_string_pretty(&value).map_err(|error| error.to_string())
}

pub(crate) fn patch_yaml_credential(raw: &str, token: Option<&str>) -> Result<String, String> {
    use serde_yaml::{Mapping, Value};

    let mut root = if raw.trim().is_empty() {
        Value::Mapping(Mapping::new())
    } else {
        serde_yaml::from_str(raw).map_err(|error| error.to_string())?
    };
    let map = root
        .as_mapping_mut()
        .ok_or("credential YAML root must be a mapping")?;
    let model_key = Value::String("model".into());
    if let Some(token) = token {
        let model = map
            .entry(model_key)
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        if !model.is_mapping() {
            *model = Value::Mapping(Mapping::new());
        }
        let model = model.as_mapping_mut().expect("mapping ensured");
        model.insert(Value::String("api_key".into()), Value::String(token.into()));
        let headers_key = Value::String("default_headers".into());
        let headers = model
            .entry(headers_key)
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        if !headers.is_mapping() {
            *headers = Value::Mapping(Mapping::new());
        }
        headers
            .as_mapping_mut()
            .expect("mapping ensured")
            .insert(Value::String("token".into()), Value::String(token.into()));
    } else if let Some(model) = map.get_mut(&model_key).and_then(Value::as_mapping_mut) {
        model.remove(Value::String("api_key".into()));
        if let Some(headers) = model
            .get_mut(Value::String("default_headers".into()))
            .and_then(Value::as_mapping_mut)
        {
            headers.remove(Value::String("token".into()));
        }
    }
    serde_yaml::to_string(&root).map_err(|error| error.to_string())
}

fn parse_json_object(raw: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    if raw.trim().is_empty() {
        return Ok(serde_json::Map::new());
    }
    serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|error| error.to_string())?
        .as_object()
        .cloned()
        .ok_or_else(|| "credential JSON root must be an object".to_string())
}

fn patch_provider_auth(
    root: &mut serde_json::Map<String, serde_json::Value>,
    auth_type: &str,
    token: Option<&str>,
) {
    if let Some(token) = token {
        root.insert(
            MANAGED_PROVIDER_ID.into(),
            serde_json::json!({"type": auth_type, "key": token}),
        );
    } else {
        root.remove(MANAGED_PROVIDER_ID);
    }
}

fn patch_anthropic_custom_headers(
    root: &mut serde_json::Map<String, serde_json::Value>,
    token: Option<&str>,
) {
    let existing = root
        .get("env")
        .and_then(serde_json::Value::as_object)
        .and_then(|env| env.get("ANTHROPIC_CUSTOM_HEADERS"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let mut lines = existing
        .lines()
        .filter(|line| {
            line.split_once(':')
                .is_none_or(|(name, _)| !name.trim().eq_ignore_ascii_case("token"))
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(token) = token {
        lines.push(format!("token: {token}"));
    }
    let custom_headers = (!lines.is_empty()).then(|| lines.join("\n"));
    patch_nested_json(
        root,
        &["env"],
        "ANTHROPIC_CUSTOM_HEADERS",
        custom_headers.as_deref(),
    );
}

fn patch_root_string(
    root: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    token: Option<&str>,
) {
    if let Some(token) = token {
        root.insert(key.into(), serde_json::Value::String(token.into()));
    } else {
        root.remove(key);
    }
}

fn patch_nested_json(
    root: &mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
    key: &str,
    token: Option<&str>,
) {
    if token.is_none() {
        if let Some(target) = existing_json_object(root, path) {
            target.remove(key);
        }
        return;
    }
    let target = ensure_json_object(root, path);
    target.insert(key.into(), serde_json::Value::String(token.unwrap().into()));
}

fn ensure_json_object<'a>(
    root: &'a mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
) -> &'a mut serde_json::Map<String, serde_json::Value> {
    let mut current = root;
    for segment in path {
        let value = current
            .entry(*segment)
            .or_insert_with(|| serde_json::json!({}));
        if !value.is_object() {
            *value = serde_json::json!({});
        }
        current = value.as_object_mut().expect("object ensured");
    }
    current
}

fn existing_json_object<'a>(
    root: &'a mut serde_json::Map<String, serde_json::Value>,
    path: &[&str],
) -> Option<&'a mut serde_json::Map<String, serde_json::Value>> {
    let mut current = root;
    for segment in path {
        current = current.get_mut(*segment)?.as_object_mut()?;
    }
    Some(current)
}

fn parse_toml_root(raw: &str) -> Result<toml::Value, String> {
    if raw.trim().is_empty() {
        Ok(toml::Value::Table(toml::map::Map::new()))
    } else {
        raw.parse()
            .map_err(|error: toml::de::Error| error.to_string())
    }
}

fn patch_nested_toml(
    root: &mut toml::map::Map<String, toml::Value>,
    path: &[&str],
    key: &str,
    token: Option<&str>,
) -> Result<(), String> {
    if token.is_none() {
        if let Some(target) = existing_toml_table(root, path) {
            target.remove(key);
        }
        return Ok(());
    }
    let target = ensure_toml_table(root, path)?;
    target.insert(key.into(), toml::Value::String(token.unwrap().into()));
    Ok(())
}

fn ensure_toml_table<'a>(
    root: &'a mut toml::map::Map<String, toml::Value>,
    path: &[&str],
) -> Result<&'a mut toml::map::Map<String, toml::Value>, String> {
    let mut current = root;
    for segment in path {
        let value = current
            .entry(*segment)
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if !value.is_table() {
            *value = toml::Value::Table(toml::map::Map::new());
        }
        current = value
            .as_table_mut()
            .ok_or_else(|| format!("{segment} must be a TOML table"))?;
    }
    Ok(current)
}

fn existing_toml_table<'a>(
    root: &'a mut toml::map::Map<String, toml::Value>,
    path: &[&str],
) -> Option<&'a mut toml::map::Map<String, toml::Value>> {
    let mut current = root;
    for segment in path {
        current = current.get_mut(*segment)?.as_table_mut()?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_credential_patch_is_a_noop() {
        let raw = r#"{
  "env": {
    "GEMINI_API_KEY": "native-key",
    "GEMINI_CLI_CUSTOM_HEADERS": "x-native: true"
  }
}
"#;

        assert_eq!(
            patch_json_credential(AgentType::Gemini, raw, Some("managed-token")).unwrap(),
            raw
        );
        assert_eq!(
            patch_json_credential(AgentType::Gemini, raw, None).unwrap(),
            raw
        );
    }

    #[test]
    fn codebuddy_credential_patch_uses_native_key_and_cleans_legacy_auth() {
        let raw = r#"{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "old-token",
    "ANTHROPIC_CUSTOM_HEADERS": "token: old-token\nx-keep: yes",
    "ANTHROPIC_MODEL": "old-model",
    "ANTHROPIC_FUTURE_OPTION": "must-not-leak",
    "CODEBUDDY_AUTH_TOKEN": "old-oauth-token",
    "KEEP": "custom"
  }
}
"#;

        let patched = patch_json_credential(AgentType::CodeBuddy, raw, Some("managed-token"))
            .expect("patch CodeBuddy credentials");
        let value: serde_json::Value = serde_json::from_str(&patched).expect("valid JSON");
        let env = value["env"].as_object().expect("env object");
        assert_eq!(
            env.get("CODEBUDDY_API_KEY")
                .and_then(|value| value.as_str()),
            Some("managed-token")
        );
        assert_eq!(
            env.get("KEEP").and_then(|value| value.as_str()),
            Some("custom")
        );
        assert!(!env.contains_key("CODEBUDDY_AUTH_TOKEN"));
        assert!(!env.keys().any(|key| key.starts_with("ANTHROPIC_")));

        let cleared = patch_json_credential(AgentType::CodeBuddy, &patched, None)
            .expect("clear CodeBuddy credentials");
        let value: serde_json::Value = serde_json::from_str(&cleared).expect("valid JSON");
        let env = value["env"].as_object().expect("env object");
        assert!(!env.contains_key("CODEBUDDY_API_KEY"));
        assert_eq!(
            env.get("KEEP").and_then(|value| value.as_str()),
            Some("custom")
        );
    }
}
