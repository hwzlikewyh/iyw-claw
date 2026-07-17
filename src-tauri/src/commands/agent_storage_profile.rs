use std::path::{Path, PathBuf};

use crate::models::agent::AgentType;

pub(crate) fn is_user_global_profile_path(
    agent_type: AgentType,
    candidate: &Path,
    home: &Path,
    xdg_config: &Path,
) -> bool {
    let expected = match agent_type {
        AgentType::ClaudeCode => home.join(".claude"),
        AgentType::Codex => home.join(".codex"),
        AgentType::Gemini => home.join(".gemini"),
        AgentType::OpenClaw => home.join(".openclaw"),
        AgentType::OpenCode => xdg_config.join("opencode"),
        AgentType::Cline => home.join(".cline").join("data"),
        AgentType::Hermes => home.join(".hermes"),
        AgentType::CodeBuddy => home.join(".codebuddy"),
        AgentType::KimiCode => home.join(".kimi-code"),
        AgentType::Pi => home.join(".pi").join("agent"),
        AgentType::Grok => home.join(".grok"),
    };
    comparable_path(candidate) == comparable_path(&expected)
}

fn comparable_path(path: &Path) -> String {
    let normalized = path
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .to_string();
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}
