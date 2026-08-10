use crate::models::AgentType;

/// 返回 Agent 支持范围内的最高自动权限模式。
pub(crate) fn automatic_mode_id(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::Codex => "agent-full-access",
        AgentType::ClaudeCode | AgentType::CodeBuddy | AgentType::Grok => "bypassPermissions",
        AgentType::Gemini => "yolo",
        AgentType::OpenCode => "build",
        AgentType::Cline => "act",
        AgentType::OpenClaw | AgentType::Hermes | AgentType::KimiCode | AgentType::Pi => "default",
    }
}
