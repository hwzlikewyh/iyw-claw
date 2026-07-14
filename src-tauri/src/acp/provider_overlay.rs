use std::collections::BTreeMap;

use crate::models::agent::AgentType;

pub use super::provider_overlay_files::{
    enforce_active_provider_overlay, enforce_all_provider_overlays,
    enforce_existing_active_provider_overlays, enforce_existing_provider_overlays,
    enforce_provider_overlay,
};

pub(crate) use super::provider_overlay_formats::{
    patch_codex_toml, patch_hermes_yaml, patch_json_config, patch_kimi_toml, patch_pi_models_json,
};
pub use super::provider_overlay_formats::{
    MANAGED_DEFAULT_MODEL, MANAGED_MODEL_IDS, MANAGED_PROVIDER_ID,
};

pub const MODEL_GATEWAY_LOCAL_URL: &str = "http://127.0.0.1:6001";
pub const MODEL_GATEWAY_TEST_URL: &str = "http://192.168.1.86:3201/ai-application";
pub const MODEL_GATEWAY_PRODUCTION_URL: &str =
    "https://gateway.iyw.cn/iyw-fusion-api";

#[cfg(debug_assertions)]
pub const MODEL_GATEWAY_BASE_URL: &str = MODEL_GATEWAY_LOCAL_URL;
#[cfg(all(not(debug_assertions), feature = "test-gateway"))]
pub const MODEL_GATEWAY_BASE_URL: &str = MODEL_GATEWAY_TEST_URL;
#[cfg(all(not(debug_assertions), not(feature = "test-gateway")))]
pub const MODEL_GATEWAY_BASE_URL: &str = MODEL_GATEWAY_PRODUCTION_URL;
pub const MODEL_GATEWAY_BASE_URL_ENV: &str = "IYW_CLAW_MODEL_GATEWAY_BASE_URL";

pub fn model_gateway_base_url() -> String {
    std::env::var(MODEL_GATEWAY_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| MODEL_GATEWAY_BASE_URL.to_string())
}

pub fn apply_provider_runtime_env(
    agent_type: AgentType,
    runtime_env: &mut BTreeMap<String, String>,
) {
    let base_url = model_gateway_base_url();
    apply_provider_runtime_env_with_base(agent_type, runtime_env, &base_url);
}

pub(crate) fn apply_provider_runtime_env_with_base(
    agent_type: AgentType,
    runtime_env: &mut BTreeMap<String, String>,
    base_url: &str,
) {
    runtime_env.insert(
        provider_base_url_env_key(agent_type).to_string(),
        base_url.trim().to_string(),
    );
    runtime_env.insert(
        provider_model_env_key(agent_type).to_string(),
        MANAGED_DEFAULT_MODEL.to_string(),
    );
}

pub(crate) fn provider_base_url_env_key(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode => "ANTHROPIC_BASE_URL",
        AgentType::Gemini => "GOOGLE_GEMINI_BASE_URL",
        AgentType::KimiCode => "KIMI_MODEL_BASE_URL",
        _ => "OPENAI_BASE_URL",
    }
}

fn provider_model_env_key(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode | AgentType::CodeBuddy => "ANTHROPIC_MODEL",
        AgentType::Gemini => "GEMINI_MODEL",
        AgentType::KimiCode => "KIMI_MODEL_NAME",
        _ => "OPENAI_MODEL",
    }
}

#[cfg(test)]
#[path = "provider_overlay_tests.rs"]
mod tests;
