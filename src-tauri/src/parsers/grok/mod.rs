mod paths;
mod session;
mod tools;
mod turns;
mod update_state;
mod updates;

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::models::{
    AgentType, ConversationDetail, ConversationSummary, MessageTurn, SessionStats, TurnRole,
};
use crate::parsers::{
    compute_session_stats, folder_name_from_path, infer_context_window_max_tokens,
    latest_turn_total_usage_tokens, merge_context_window_stats, relocate_orphaned_tool_results,
    structurize_read_tool_output, title_from_user_text, AgentParser, ParseError,
};

use paths::read_subdirs;
pub(crate) use paths::resolve_grok_home_dir;
use session::{read_summary_json, SummaryMeta};
use updates::{parse_updates, ParsedUpdates};

/// Parser for Grok Build's `$GROK_HOME/sessions/<group>/<session>/` store.
pub struct GrokParser {
    base_dir: PathBuf,
}

impl GrokParser {
    pub fn new() -> Self {
        Self {
            base_dir: resolve_grok_home_dir().join("sessions"),
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn build_summary(&self, session_dir: &Path, session_id: &str) -> Option<ConversationSummary> {
        let parsed = parse_updates(&session_dir.join("updates.jsonl"));
        if parsed.content_events == 0 {
            return None;
        }
        let metadata = read_summary_json(session_dir);
        Some(self.summary_from(session_id, &metadata, &parsed))
    }

    fn summary_from(
        &self,
        session_id: &str,
        metadata: &SummaryMeta,
        parsed: &ParsedUpdates,
    ) -> ConversationSummary {
        let folder_path = metadata.cwd.clone();
        ConversationSummary {
            id: session_id.to_string(),
            agent_type: AgentType::Grok,
            folder_name: folder_path.as_deref().map(folder_name_from_path),
            folder_path,
            title: metadata
                .title
                .clone()
                .or_else(|| parsed.first_user_text.as_deref().map(title_from_user_text)),
            started_at: metadata
                .created_at
                .or(parsed.first_ts)
                .unwrap_or_else(Utc::now),
            ended_at: metadata.updated_at.or(parsed.last_ts),
            message_count: parsed.turns.len() as u32,
            model: metadata.model.clone().or_else(|| parsed.model.clone()),
            git_branch: metadata.git_branch.clone(),
            parent_id: None,
            parent_tool_use_id: None,
            delegation_call_id: None,
        }
    }

    fn build_detail(&self, session_dir: &Path, session_id: &str) -> ConversationDetail {
        let mut parsed = parse_updates(&session_dir.join("updates.jsonl"));
        let metadata = read_summary_json(session_dir);
        normalize_turns(&mut parsed.turns);
        let session_model = metadata.model.clone().or_else(|| parsed.model.clone());
        apply_session_model(&mut parsed.turns, session_model.as_deref());
        let session_stats = build_session_stats(&parsed.turns, session_model.as_deref());
        let summary = self.summary_from(session_id, &metadata, &parsed);
        ConversationDetail {
            summary,
            turns: parsed.turns,
            session_stats,
            transcript_watermark: None,
        }
    }

    fn find_session_dir(&self, conversation_id: &str) -> Option<PathBuf> {
        read_subdirs(&self.base_dir)
            .into_iter()
            .map(|group| group.join(conversation_id))
            .find(|candidate| candidate.join("updates.jsonl").is_file())
    }
}

impl Default for GrokParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentParser for GrokParser {
    fn list_conversations(&self) -> Result<Vec<ConversationSummary>, ParseError> {
        if !self.base_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut conversations = Vec::new();
        for group in read_subdirs(&self.base_dir) {
            for session_dir in read_subdirs(&group) {
                let Some(session_id) = session_dir
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                else {
                    continue;
                };
                if let Some(summary) = self.build_summary(&session_dir, &session_id) {
                    conversations.push(summary);
                }
            }
        }
        conversations.sort_by_key(|conversation| std::cmp::Reverse(conversation.started_at));
        Ok(conversations)
    }

    fn get_conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ParseError> {
        let session_dir = self
            .find_session_dir(conversation_id)
            .ok_or_else(|| ParseError::ConversationNotFound(conversation_id.to_string()))?;
        Ok(self.build_detail(&session_dir, conversation_id))
    }
}

fn normalize_turns(turns: &mut Vec<MessageTurn>) {
    relocate_orphaned_tool_results(turns);
    structurize_read_tool_output(turns);
}

fn apply_session_model(turns: &mut [MessageTurn], session_model: Option<&str>) {
    let Some(session_model) = session_model else {
        return;
    };
    for turn in turns {
        if matches!(turn.role, TurnRole::Assistant) && turn.model.is_none() {
            turn.model = Some(session_model.to_string());
        }
    }
}

fn build_session_stats(turns: &[MessageTurn], model: Option<&str>) -> Option<SessionStats> {
    merge_context_window_stats(
        compute_session_stats(turns),
        latest_turn_total_usage_tokens(turns),
        infer_context_window_max_tokens(model),
    )
}

#[cfg(test)]
mod tests;
