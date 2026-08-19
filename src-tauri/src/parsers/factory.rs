use std::collections::HashSet;

use crate::models::AgentType;

use super::acp_transcript::{discover_custom_agent_types, AcpTranscriptParser};
use super::claude::ClaudeParser;
use super::cline::ClineParser;
use super::codebuddy::CodeBuddyParser;
use super::codex::CodexParser;
use super::deepseek::DeepSeekParser;
use super::gemini::GeminiParser;
use super::grok::GrokParser;
use super::hermes::HermesParser;
use super::kimi_code::KimiCodeParser;
use super::openclaw::OpenClawParser;
use super::opencode::OpenCodeParser;
use super::pi::PiParser;
use super::AgentParser;

pub fn parser_for_agent(agent_type: AgentType) -> Box<dyn AgentParser> {
    match agent_type {
        AgentType::ClaudeCode => Box::new(ClaudeParser::new()),
        AgentType::Codex => Box::new(CodexParser::new()),
        AgentType::OpenCode => Box::new(OpenCodeParser::new()),
        AgentType::Gemini => Box::new(GeminiParser::new()),
        AgentType::OpenClaw => Box::new(OpenClawParser::new()),
        AgentType::Cline => Box::new(ClineParser::new()),
        AgentType::Hermes => Box::new(HermesParser::new()),
        AgentType::CodeBuddy => Box::new(CodeBuddyParser::new()),
        AgentType::KimiCode => Box::new(KimiCodeParser::new()),
        AgentType::Pi => Box::new(PiParser::new()),
        AgentType::Grok => Box::new(GrokParser::new()),
        AgentType::DeepSeek => Box::new(DeepSeekParser::new()),
        AgentType::Cursor | AgentType::Custom(_) => Box::new(AcpTranscriptParser::new(agent_type)),
    }
}

pub fn history_parsers() -> Vec<(AgentType, Box<dyn AgentParser>)> {
    let mut agent_types = crate::models::agent::BUILTIN_AGENT_TYPES.to_vec();
    agent_types.extend(discover_custom_agent_types());
    let mut seen = HashSet::new();
    agent_types
        .into_iter()
        .filter(|agent_type| seen.insert(*agent_type))
        .map(|agent_type| (agent_type, parser_for_agent(agent_type)))
        .collect()
}
