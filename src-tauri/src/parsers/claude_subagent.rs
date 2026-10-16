use super::ClaudeParser;
use crate::models::ConversationDetail;
use crate::parsers::{is_safe_subagent_id, ParseError};

impl ClaudeParser {
    pub(crate) fn runtime_subagent(
        &self,
        parent: &str,
        agent: &str,
    ) -> Result<ConversationDetail, ParseError> {
        if !is_safe_subagent_id(agent) {
            return Err(ParseError::ConversationNotFound(agent.into()));
        }
        let root = super::find_session_file(parent)
            .ok_or_else(|| ParseError::ConversationNotFound(parent.into()))?;
        let path = root
            .with_extension("")
            .join("subagents")
            .join(format!("agent-{agent}.jsonl"));
        self.parse_conversation_detail(&path, agent)
    }
}
