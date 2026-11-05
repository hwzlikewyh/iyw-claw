use serde_json::Value;

use crate::models::AgentType;

pub(crate) fn incompatibility(agent: AgentType, spec: &Value) -> Option<String> {
    if agent == AgentType::Codex && !crate::internal_xinghe_worker::is_desktop_agent(agent) {
        return Some("服务端星河尚未提供对话级连接器隔离，请使用桌面内置星河".into());
    }
    if agent != AgentType::Codex && spec["cwd"].as_str().is_some_and(|cwd| !cwd.is_empty()) {
        return Some(
            "当前智能体的 ACP MCP 注入不能保留工作目录，请选用内置星河或无 cwd 的选项".into(),
        );
    }
    if !agent_supports_conversation_mcp(agent) {
        return Some("当前智能体尚未提供对话级连接器隔离，请切换到支持的智能体".into());
    }
    if agent == AgentType::Codex && spec["type"].as_str() == Some("sse") {
        return Some("内置星河不支持旧版 HTTP+SSE，请选用 Streamable HTTP 或 STDIO".into());
    }
    if spec["iyw_market"]["auth_mode"].as_str() == Some("oauth") && agent != AgentType::Codex {
        return Some("此 OAuth 连接器需要支持原生 MCP 授权的内置星河".into());
    }
    if !publisher_supports_agent(agent, spec) {
        return Some("发布者未声明此连接器兼容当前智能体".into());
    }
    None
}

fn agent_supports_conversation_mcp(agent: AgentType) -> bool {
    matches!(
        agent,
        AgentType::Codex
            | AgentType::ClaudeCode
            | AgentType::OpenCode
            | AgentType::Gemini
            | AgentType::Cline
            | AgentType::CodeBuddy
    )
}

fn publisher_supports_agent(agent: AgentType, spec: &Value) -> bool {
    let Some(agents) = spec["iyw_market"]["agents"]
        .as_array()
        .filter(|items| !items.is_empty())
    else {
        return true;
    };
    let wire = agent.as_wire();
    agents
        .iter()
        .any(|value| value.as_str() == Some(wire.as_ref()))
}
