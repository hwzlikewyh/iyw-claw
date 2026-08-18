use serde_json::Value;

use crate::models::{AgentType, AutomationDraft};

pub fn enforce_automatic_mode(mut draft: AutomationDraft) -> AutomationDraft {
    let Some(mode_id) = automatic_mode_id(&draft.agent_type) else {
        tracing::warn!(
            agent_type = %draft.agent_type,
            "[automation] cannot resolve automatic mode for unknown agent"
        );
        return draft;
    };
    let Some(config) = draft.config.as_object_mut() else {
        return draft;
    };
    config.insert("mode_id".to_string(), Value::String(mode_id.to_string()));
    tracing::info!(
        agent_type = %draft.agent_type,
        mode_id,
        "[automation] enforced automatic mode"
    );
    draft
}

fn automatic_mode_id(raw: &str) -> Option<&'static str> {
    let agent = serde_json::from_value::<AgentType>(Value::String(raw.to_string())).ok()?;
    match agent {
        AgentType::Codex => Some("agent-full-access"),
        AgentType::ClaudeCode | AgentType::CodeBuddy | AgentType::Grok => Some("bypassPermissions"),
        AgentType::Gemini => Some("yolo"),
        AgentType::OpenCode => Some("build"),
        AgentType::Cline => Some("act"),
        AgentType::OpenClaw | AgentType::Hermes | AgentType::KimiCode => Some("default"),
        AgentType::Pi => Some("medium"),
        AgentType::Cursor | AgentType::DeepSeek | AgentType::Custom(_) => None,
    }
}
