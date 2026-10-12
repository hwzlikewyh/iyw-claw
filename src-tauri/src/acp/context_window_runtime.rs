//! 复用 SDK 已有的原生配置路径；不改变认证、会话历史或存储目录。
use std::collections::BTreeMap;

use super::context_window_config::ContextBudget;
use super::error::AcpError;
use crate::models::AgentType;

pub(crate) const TOKENS_ENV: &str = "IYW_CLAW_CONTEXT_WINDOW_TOKENS";
pub(crate) const COMPACT_ENV: &str = "IYW_CLAW_CONTEXT_COMPACT_TOKENS";
pub(crate) const MODEL_ENV: &str = "IYW_CLAW_CONTEXT_WINDOW_MODEL";

pub(crate) fn supports(agent: AgentType) -> bool {
    !matches!(
        agent,
        AgentType::Cursor | AgentType::DeepSeek | AgentType::Custom(_)
    )
}

pub(crate) fn apply(
    environment: &mut BTreeMap<String, String>,
    budget: &ContextBudget,
) -> Result<(), AcpError> {
    match budget.agent {
        AgentType::Codex => apply_codex(environment, budget)?,
        AgentType::ClaudeCode => {
            environment.insert(
                "CLAUDE_CODE_MAX_CONTEXT_TOKENS".into(),
                budget.window.to_string(),
            );
            environment.insert(
                "CLAUDE_CODE_AUTO_COMPACT_WINDOW".into(),
                budget.compact.to_string(),
            );
        }
        AgentType::CodeBuddy => {
            environment.insert(
                "CODEBUDDY_AUTO_COMPACT_WINDOW".into(),
                budget.compact.to_string(),
            );
            environment.insert("CODEBUDDY_PRE_MESSAGE_COMPACT".into(), "true".into());
        }
        AgentType::OpenCode => apply_opencode(environment, budget)?,
        AgentType::Gemini | AgentType::OpenClaw | AgentType::Cline | AgentType::Pi => {
            super::context_window_json::apply_file(budget)?
        }
        AgentType::KimiCode => {
            environment.insert(
                "KIMI_MODEL_MAX_CONTEXT_SIZE".into(),
                budget.window.to_string(),
            );
            super::context_window_text::apply_file(budget)?
        }
        AgentType::Grok | AgentType::Hermes => super::context_window_text::apply_file(budget)?,
        _ => {
            return Err(AcpError::protocol(
                "Agent SDK has no native context window configuration",
            ))
        }
    }
    environment.insert(TOKENS_ENV.into(), budget.window.to_string());
    environment.insert(COMPACT_ENV.into(), budget.compact.to_string());
    environment.insert(MODEL_ENV.into(), budget.model.clone());
    Ok(())
}

fn apply_codex(
    environment: &mut BTreeMap<String, String>,
    budget: &ContextBudget,
) -> Result<(), AcpError> {
    let mut config: serde_json::Value = environment
        .get("CODEX_CONFIG")
        .map(|raw| serde_json::from_str(raw))
        .transpose()
        .map_err(|error| {
            AcpError::protocol(format!("Invalid native Codex configuration: {error}"))
        })?
        .unwrap_or_else(|| serde_json::json!({}));
    let object = config
        .as_object_mut()
        .ok_or_else(|| AcpError::protocol("Native Codex config must be an object"))?;
    object.insert("model_context_window".into(), budget.window.into());
    object.insert(
        "model_auto_compact_token_limit".into(),
        budget.compact.into(),
    );
    environment.insert("CODEX_CONFIG".into(), config.to_string());
    Ok(())
}

fn apply_opencode(
    environment: &mut BTreeMap<String, String>,
    budget: &ContextBudget,
) -> Result<(), AcpError> {
    let mut config: serde_json::Value = environment
        .get("OPENCODE_CONFIG_CONTENT")
        .map(|raw| serde_json::from_str(raw))
        .transpose()
        .map_err(|error| {
            AcpError::protocol(format!("Invalid OpenCode runtime configuration: {error}"))
        })?
        .unwrap_or_else(|| serde_json::json!({}));
    super::context_window_json::patch_opencode(&mut config, budget)?;
    environment.insert("OPENCODE_CONFIG_CONTENT".into(), config.to_string());
    Ok(())
}
