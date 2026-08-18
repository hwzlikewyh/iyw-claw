use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::acp::transcript::{
    list_agent_dirs_in, list_session_ids_in, read_chain_in, read_header_in,
    superseded_session_ids_in, TranscriptData,
};
use crate::models::{
    AgentType, ContentBlock, ConversationDetail, ConversationSummary, MessageTurn, TurnRole,
};

use super::{compute_session_stats, folder_name_from_path, truncate_str, AgentParser, ParseError};

pub struct AcpTranscriptParser {
    agent_type: AgentType,
    root: PathBuf,
}

impl AcpTranscriptParser {
    pub fn new(agent_type: AgentType) -> Self {
        Self {
            agent_type,
            root: crate::paths::iyw_claw_acp_transcripts_root(),
        }
    }

    fn agent_dir(&self) -> &'static str {
        crate::acp::registry::registry_id_for(self.agent_type)
    }

    fn summarize(&self, session_id: &str, data: &TranscriptData) -> ConversationSummary {
        let started_at = data
            .header
            .as_ref()
            .map(|header| header.started_at)
            .or_else(|| data.turns.first().map(|turn| turn.timestamp))
            .unwrap_or_else(Utc::now);
        let ended_at = data
            .turns
            .last()
            .map(|turn| turn.completed_at.unwrap_or(turn.timestamp));
        let folder_path = data
            .header
            .as_ref()
            .map(|header| header.cwd.trim().to_string())
            .filter(|path| !path.is_empty());
        let folder_name = folder_path
            .as_deref()
            .map(folder_name_from_path)
            .filter(|name| !name.is_empty());

        ConversationSummary {
            id: session_id.to_string(),
            agent_type: self.agent_type,
            folder_path,
            folder_name,
            title: first_user_title(&data.turns),
            started_at,
            ended_at,
            message_count: data.turns.len() as u32,
            model: data.turns.iter().rev().find_map(|turn| turn.model.clone()),
            git_branch: None,
            parent_id: None,
            parent_tool_use_id: None,
            delegation_call_id: None,
        }
    }
}

impl AgentParser for AcpTranscriptParser {
    fn list_conversations(&self) -> Result<Vec<ConversationSummary>, ParseError> {
        let agent_dir = self.agent_dir();
        let superseded = superseded_session_ids_in(&self.root, agent_dir);
        let mut summaries = list_session_ids_in(&self.root, agent_dir)
            .into_iter()
            .filter(|session_id| !superseded.contains(session_id))
            .filter_map(|session_id| {
                let data = read_chain_in(&self.root, self.agent_type, &session_id);
                valid_transcript(self.agent_type, &data).then(|| self.summarize(&session_id, &data))
            })
            .collect::<Vec<_>>();
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.started_at));
        Ok(summaries)
    }

    fn get_conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ParseError> {
        let data = read_chain_in(&self.root, self.agent_type, conversation_id);
        if !valid_transcript(self.agent_type, &data) {
            return Err(ParseError::ConversationNotFound(
                conversation_id.to_string(),
            ));
        }
        Ok(ConversationDetail {
            summary: self.summarize(conversation_id, &data),
            session_stats: compute_session_stats(&data.turns),
            turns: data.turns,
            transcript_watermark: None,
        })
    }
}

pub fn discover_custom_agent_types() -> Vec<AgentType> {
    discover_custom_agent_types_in(&crate::paths::iyw_claw_acp_transcripts_root())
}

fn discover_custom_agent_types_in(root: &Path) -> Vec<AgentType> {
    let mut found = HashSet::new();
    for agent_dir in list_agent_dirs_in(root) {
        let Some(candidate) = AgentType::custom(&agent_dir) else {
            continue;
        };
        let matches_header = list_session_ids_in(root, &agent_dir)
            .into_iter()
            .filter_map(|session_id| read_header_in(root, &agent_dir, &session_id))
            .any(|header| header.agent == candidate);
        if matches_header {
            found.insert(candidate);
        }
    }
    let mut found = found.into_iter().collect::<Vec<_>>();
    found.sort();
    found
}

fn valid_transcript(agent_type: AgentType, data: &TranscriptData) -> bool {
    !data.turns.is_empty()
        && data
            .header
            .as_ref()
            .is_some_and(|header| header.agent == agent_type)
}

fn first_user_title(turns: &[MessageTurn]) -> Option<String> {
    let text = turns
        .iter()
        .filter(|turn| matches!(&turn.role, TurnRole::User))
        .find_map(|turn| {
            turn.blocks.iter().find_map(|block| match block {
                ContentBlock::Text { text } if !text.trim().is_empty() => Some(text.trim()),
                _ => None,
            })
        })?;
    Some(truncate_str(text, 80))
}
