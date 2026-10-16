use crate::acp::{error::AcpError, manager::ConnectionManager};
use crate::models::{AgentType, ConversationDetail};
use crate::parsers::AgentParser;

impl ConnectionManager {
    pub async fn runtime_task_detail(
        &self,
        connection: &str,
        task_id: &str,
    ) -> Result<ConversationDetail, AcpError> {
        let state = self
            .get_state(connection)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(connection.into()))?;
        let (agent, root, task) = {
            let state = state.read().await;
            let task = state
                .runtime_tasks
                .get(task_id)
                .filter(|task| task.kind == "subagent")
                .cloned()
                .ok_or_else(|| AcpError::protocol("Subagent does not belong to this session"))?;
            (state.agent_type, state.external_id.clone(), task)
        };
        let root = root.ok_or_else(|| AcpError::protocol("Session identity is unavailable"))?;
        let id = task.transcript_id.ok_or_else(|| {
            AcpError::protocol("Subagent transcript identity has not been confirmed")
        })?;
        if !matches!(agent, AgentType::Codex | AgentType::ClaudeCode) {
            return Err(AcpError::protocol(
                "Native subagent transcripts are unavailable",
            ));
        }
        tokio::task::spawn_blocking(move || match agent {
            AgentType::Codex => crate::parsers::codex::CodexParser::new().get_conversation(&id),
            _ => crate::parsers::claude::ClaudeParser::new().runtime_subagent(&root, &id),
        })
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?
        .map_err(|error| AcpError::protocol(error.to_string()))
    }
}
