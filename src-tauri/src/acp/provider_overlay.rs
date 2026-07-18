use std::collections::BTreeMap;

use crate::models::agent::AgentType;

pub use super::provider_overlay_files::{
    enforce_active_provider_overlay, enforce_all_provider_overlays,
    enforce_existing_active_provider_overlays, enforce_existing_provider_overlays,
    enforce_provider_overlay,
};

pub(crate) use super::provider_overlay_formats::{
    is_codebuddy_conflicting_env_key, patch_codex_toml, patch_grok_toml, patch_hermes_yaml,
    patch_json_config, patch_kimi_toml, patch_pi_models_json, CODEBUDDY_CONFLICTING_ENV_KEYS,
};
pub use super::provider_overlay_formats::{
    managed_default_model_for, managed_model_ids_for, MANAGED_DEFAULT_MODEL, MANAGED_MODEL_IDS,
    MANAGED_PROVIDER_ID,
};

pub const MODEL_GATEWAY_LOCAL_URL: &str = "http://127.0.0.1:6001";
pub const MODEL_GATEWAY_TEST_URL: &str = "http://192.168.1.86:3201/ai-application";
pub const MODEL_GATEWAY_PRODUCTION_URL: &str = "https://gateway.iyw.cn/iyw-fusion-api";
pub const MODEL_GATEWAY_PRODUCTION_OPENAI_URL: &str = "https://gateway.iyw.cn/iyw-fusion-api/v1";
pub const MODEL_GATEWAY_PRODUCTION_ANTHROPIC_URL: &str =
    "https://gateway.iyw.cn/iyw-fusion-api/anthropic";

#[cfg(debug_assertions)]
pub const MODEL_GATEWAY_BASE_URL: &str = MODEL_GATEWAY_LOCAL_URL;
#[cfg(all(not(debug_assertions), feature = "test-gateway"))]
pub const MODEL_GATEWAY_BASE_URL: &str = MODEL_GATEWAY_TEST_URL;
#[cfg(all(not(debug_assertions), not(feature = "test-gateway")))]
pub const MODEL_GATEWAY_BASE_URL: &str = MODEL_GATEWAY_PRODUCTION_URL;
pub const MODEL_GATEWAY_BASE_URL_ENV: &str = "IYW_CLAW_MODEL_GATEWAY_BASE_URL";

fn configured_model_gateway_base_url() -> Option<String> {
    std::env::var(MODEL_GATEWAY_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn uses_managed_gateway(agent_type: AgentType) -> bool {
    agent_type != AgentType::Gemini
}

pub(crate) fn production_model_gateway_base_url(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode => MODEL_GATEWAY_PRODUCTION_ANTHROPIC_URL,
        AgentType::Gemini => MODEL_GATEWAY_PRODUCTION_URL,
        _ => MODEL_GATEWAY_PRODUCTION_OPENAI_URL,
    }
}

pub fn model_gateway_base_url_for(agent_type: AgentType) -> String {
    if let Some(configured) = configured_model_gateway_base_url() {
        return configured;
    }
    if MODEL_GATEWAY_BASE_URL == MODEL_GATEWAY_PRODUCTION_URL {
        return production_model_gateway_base_url(agent_type).to_string();
    }
    MODEL_GATEWAY_BASE_URL.to_string()
}

pub fn model_gateway_models_url() -> String {
    let base = model_gateway_base_url_for(AgentType::Codex);
    let base = base.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

pub fn apply_provider_runtime_env(
    agent_type: AgentType,
    runtime_env: &mut BTreeMap<String, String>,
) {
    if !uses_managed_gateway(agent_type) {
        return;
    }
    let base_url = model_gateway_base_url_for(agent_type);
    apply_provider_runtime_env_with_base(agent_type, runtime_env, &base_url);
}

pub(crate) fn apply_provider_runtime_env_with_base(
    agent_type: AgentType,
    runtime_env: &mut BTreeMap<String, String>,
    base_url: &str,
) {
    if !uses_managed_gateway(agent_type) {
        return;
    }
    if agent_type == AgentType::CodeBuddy {
        runtime_env.retain(|key, _| !is_codebuddy_conflicting_env_key(key));
    }
    runtime_env.insert(
        provider_base_url_env_key(agent_type).to_string(),
        base_url.trim().to_string(),
    );
    if agent_type == AgentType::CodeBuddy {
        // CodeBuddy uses the OpenAI-compatible client for custom endpoints and
        // exposes separate model selectors for the main, reasoning, fast, and
        // sub-agent paths. Keep all selectors pinned to the managed catalog.
        let models = managed_model_ids_for(agent_type);
        for (key, model) in [
            ("CODEBUDDY_MODEL", models[0]),
            ("CODEBUDDY_BIG_SLOW_MODEL", models[0]),
            ("CODEBUDDY_SMALL_FAST_MODEL", models[1]),
            ("CODEBUDDY_CODE_SUBAGENT_MODEL", models[1]),
        ] {
            runtime_env.insert(key.to_string(), model.to_string());
        }
    } else {
        runtime_env.insert(
            provider_model_env_key(agent_type).to_string(),
            managed_default_model_for(agent_type).to_string(),
        );
    }
}

pub(crate) fn provider_base_url_env_key(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode => "ANTHROPIC_BASE_URL",
        AgentType::CodeBuddy => "CODEBUDDY_BASE_URL",
        AgentType::Gemini => "GOOGLE_GEMINI_BASE_URL",
        AgentType::KimiCode => "KIMI_MODEL_BASE_URL",
        AgentType::Grok => "GROK_XAI_API_BASE_URL",
        _ => "OPENAI_BASE_URL",
    }
}

fn provider_model_env_key(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode => "ANTHROPIC_MODEL",
        AgentType::CodeBuddy => "CODEBUDDY_MODEL",
        AgentType::Gemini => "GEMINI_MODEL",
        AgentType::KimiCode => "KIMI_MODEL_NAME",
        AgentType::Grok => "GROK_DEFAULT_MODEL",
        _ => "OPENAI_MODEL",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_base_urls_follow_supported_gateway_protocols() {
        let openai = "https://gateway.iyw.cn/iyw-fusion-api/v1";
        let anthropic = "https://gateway.iyw.cn/iyw-fusion-api/anthropic";
        for agent_type in crate::acp::registry::all_acp_agents() {
            if agent_type == AgentType::Gemini {
                assert!(!uses_managed_gateway(agent_type));
                continue;
            }
            assert!(uses_managed_gateway(agent_type));
            let expected = match agent_type {
                AgentType::ClaudeCode => anthropic,
                _ => openai,
            };
            assert_eq!(
                production_model_gateway_base_url(agent_type),
                expected,
                "unexpected managed Base URL for {agent_type}"
            );
        }
    }

    #[test]
    fn grok_runtime_uses_native_gateway_environment_keys() {
        let mut environment = BTreeMap::new();
        let codex_base_url = production_model_gateway_base_url(AgentType::Codex);
        apply_provider_runtime_env_with_base(AgentType::Grok, &mut environment, codex_base_url);
        assert_eq!(
            environment.get("GROK_XAI_API_BASE_URL").map(String::as_str),
            Some(codex_base_url)
        );
        assert_eq!(
            environment.get("GROK_DEFAULT_MODEL").map(String::as_str),
            Some(MANAGED_DEFAULT_MODEL)
        );
        assert!(!environment.contains_key("OPENAI_BASE_URL"));
        assert!(!environment.contains_key("OPENAI_MODEL"));
    }

    #[test]
    fn gemini_runtime_env_is_unchanged_by_the_managed_gateway() {
        let mut environment = BTreeMap::from([
            (
                "GOOGLE_GEMINI_BASE_URL".to_string(),
                "https://gemini.example".to_string(),
            ),
            ("GEMINI_MODEL".to_string(), "gemini-native".to_string()),
        ]);
        let original = environment.clone();

        apply_provider_runtime_env_with_base(
            AgentType::Gemini,
            &mut environment,
            MODEL_GATEWAY_PRODUCTION_URL,
        );

        assert_eq!(environment, original);
    }

    #[test]
    fn codebuddy_runtime_env_uses_openai_compatible_keys() {
        let mut environment = BTreeMap::from([
            (
                "ANTHROPIC_BASE_URL".to_string(),
                "https://old/anthropic".to_string(),
            ),
            ("ANTHROPIC_AUTH_TOKEN".to_string(), "old-token".to_string()),
            (
                "ANTHROPIC_FUTURE_OPTION".to_string(),
                "must-not-leak".to_string(),
            ),
            ("CODEBUDDY_AUTH_TOKEN".to_string(), "old-oauth".to_string()),
            (
                "CODEBUDDY_INTERNET_ENVIRONMENT".to_string(),
                "internal".to_string(),
            ),
        ]);
        apply_provider_runtime_env_with_base(
            AgentType::CodeBuddy,
            &mut environment,
            MODEL_GATEWAY_PRODUCTION_OPENAI_URL,
        );

        assert_eq!(
            environment.get("CODEBUDDY_BASE_URL").map(String::as_str),
            Some(MODEL_GATEWAY_PRODUCTION_OPENAI_URL)
        );
        assert_eq!(
            environment.get("CODEBUDDY_MODEL").map(String::as_str),
            Some(managed_default_model_for(AgentType::CodeBuddy))
        );
        let models = managed_model_ids_for(AgentType::CodeBuddy);
        assert_eq!(
            environment
                .get("CODEBUDDY_BIG_SLOW_MODEL")
                .map(String::as_str),
            Some(models[0])
        );
        assert_eq!(
            environment
                .get("CODEBUDDY_SMALL_FAST_MODEL")
                .map(String::as_str),
            Some(models[1])
        );
        assert_eq!(
            environment
                .get("CODEBUDDY_CODE_SUBAGENT_MODEL")
                .map(String::as_str),
            Some(models[1])
        );
        assert!(!environment.keys().any(|key| key.starts_with("ANTHROPIC_")));
        assert!(!environment.contains_key("CODEBUDDY_AUTH_TOKEN"));
        assert!(!environment.contains_key("CODEBUDDY_INTERNET_ENVIRONMENT"));
        assert!(!environment.contains_key("OPENAI_BASE_URL"));
    }
}
