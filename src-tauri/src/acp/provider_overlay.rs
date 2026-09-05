use std::collections::BTreeMap;
use std::path::Path;

use crate::models::agent::AgentType;

pub use super::provider_overlay_files::{
    enforce_active_provider_overlay, enforce_all_provider_overlays,
    enforce_existing_active_provider_overlays, enforce_existing_provider_overlays,
    enforce_provider_overlay, enforce_resumed_active_provider_overlay,
};

pub(crate) fn write_if_changed(path: &Path, old: &str, next: &str) -> Result<(), String> {
    super::provider_overlay_files::write_if_changed(path, old, next)
}

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
    matches!(
        agent_type,
        AgentType::ClaudeCode
            | AgentType::Codex
            | AgentType::OpenCode
            | AgentType::OpenClaw
            | AgentType::Cline
            | AgentType::Hermes
            | AgentType::CodeBuddy
            | AgentType::KimiCode
            | AgentType::Pi
            | AgentType::Grok
    )
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
    format!("{MODEL_GATEWAY_PRODUCTION_OPENAI_URL}/models")
}

pub fn model_gateway_image_analysis_url() -> String {
    let base = configured_model_gateway_base_url().unwrap_or_else(|| {
        if MODEL_GATEWAY_BASE_URL == MODEL_GATEWAY_PRODUCTION_URL {
            MODEL_GATEWAY_PRODUCTION_OPENAI_URL.to_string()
        } else {
            MODEL_GATEWAY_BASE_URL.to_string()
        }
    });
    let base = base.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/image-analysis")
    } else {
        format!("{base}/v1/image-analysis")
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

pub fn apply_preferred_model_runtime_env(
    agent_type: AgentType,
    runtime_env: &mut BTreeMap<String, String>,
    preferred_model: Option<&str>,
) {
    if !uses_managed_gateway(agent_type) {
        return;
    }
    let Some(model) = preferred_model
        .map(str::trim)
        .filter(|model| !model.is_empty() && managed_model_ids_for(agent_type).contains(model))
    else {
        return;
    };
    runtime_env.insert(
        provider_model_env_key(agent_type).to_string(),
        model.to_string(),
    );
    apply_native_budget_runtime_env(agent_type, runtime_env, model);
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
        // The catalog can shrink to a single compatible model; reuse the
        // primary for the fast/sub-agent slots rather than indexing past it.
        let primary = models.first().copied().unwrap_or(MANAGED_DEFAULT_MODEL);
        let secondary = models.get(1).copied().unwrap_or(primary);
        for (key, model) in [
            ("CODEBUDDY_MODEL", primary),
            ("CODEBUDDY_BIG_SLOW_MODEL", primary),
            ("CODEBUDDY_SMALL_FAST_MODEL", secondary),
            ("CODEBUDDY_CODE_SUBAGENT_MODEL", secondary),
        ] {
            runtime_env.insert(key.to_string(), model.to_string());
        }
    } else {
        let model = managed_default_model_for(agent_type);
        runtime_env.insert(
            provider_model_env_key(agent_type).to_string(),
            model.to_string(),
        );
        apply_native_budget_runtime_env(agent_type, runtime_env, model);
    }
}

fn apply_native_budget_runtime_env(
    agent_type: AgentType,
    runtime_env: &mut BTreeMap<String, String>,
    model: &str,
) {
    match agent_type {
        AgentType::ClaudeCode => {
            let context = crate::acp::model_budget::context_window(Some(model), 1_000_000)
                .unwrap_or(1_000_000);
            let threshold = crate::acp::model_budget::compaction_threshold(Some(model), context)
                .unwrap_or(context * 9 / 10);
            runtime_env.insert(
                "CLAUDE_CODE_AUTO_COMPACT_WINDOW".into(),
                threshold.to_string(),
            );
        }
        AgentType::KimiCode => {
            if let Some(context) = crate::acp::model_budget::context_window(Some(model), 0) {
                runtime_env.insert("KIMI_MODEL_MAX_CONTEXT_SIZE".into(), context.to_string());
            }
        }
        _ => {}
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
