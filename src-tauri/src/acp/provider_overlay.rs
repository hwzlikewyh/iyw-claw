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

pub const MODEL_GATEWAY_BASE_URL: &str = "http://127.0.0.1:6001";
pub const MODEL_GATEWAY_BASE_URL_ENV: &str = "IYW_CLAW_MODEL_GATEWAY_BASE_URL";

pub fn apply_provider_runtime_env(
    agent_type: AgentType,
    runtime_env: &mut BTreeMap<String, String>,
) {
    let base_url = std::env::var(MODEL_GATEWAY_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| MODEL_GATEWAY_BASE_URL.to_string());
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
}

pub(crate) fn provider_base_url_env_key(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode => "ANTHROPIC_BASE_URL",
        AgentType::Gemini => "GOOGLE_GEMINI_BASE_URL",
        AgentType::KimiCode => "KIMI_MODEL_BASE_URL",
        _ => "OPENAI_BASE_URL",
    }
}

#[cfg(test)]
#[path = "provider_overlay_tests.rs"]
mod tests;
