mod blocks;
mod events;
mod fields;
mod paths;
mod storage;

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::models::{
    AgentType, ConversationDetail, ConversationSummary, MessageTurn, SessionStats,
};
use crate::parsers::{
    compute_session_stats, folder_name_from_path, infer_context_window_max_tokens,
    is_safe_subagent_id, merge_context_window_stats, relocate_orphaned_tool_results,
    resolve_patch_line_numbers, structurize_read_tool_output, AgentParser, ParseError,
};

use events::{EventState, SessionParse};
use paths::read_subdirs;

pub(crate) use paths::{
    resolve_deepseek_sessions_root, resolve_dsh_agents_home_dir, resolve_dsh_home_dir,
};

/// Parser for DeepSeek Harness's append-only session event logs.
pub struct DeepSeekParser {
    base_dir: PathBuf,
}

impl DeepSeekParser {
    pub fn new() -> Self {
        Self {
            base_dir: resolve_deepseek_sessions_root(),
        }
    }

    fn build_summary(&self, session_dir: &Path, session_id: &str) -> Option<ConversationSummary> {
        let parsed = parse_session_log(session_dir)?;
        if parsed.delegation_depth > 0 || parsed.content_events == 0 {
            return None;
        }
        self.summary_from(session_id, &parsed)
    }

    fn summary_from(&self, session_id: &str, parsed: &SessionParse) -> Option<ConversationSummary> {
        let started_at = parsed.created_at.clone().or(parsed.first_ts.clone())?;
        let folder_path = parsed.cwd.clone();
        Some(ConversationSummary {
            id: session_id.to_string(),
            agent_type: AgentType::DeepSeek,
            folder_name: folder_path.as_deref().map(folder_name_from_path),
            folder_path,
            title: parsed.title.clone().or(parsed.first_user_text.clone()),
            started_at,
            ended_at: parsed.last_ts.clone(),
            message_count: parsed.message_count,
            model: parsed.model.clone(),
            git_branch: None,
            parent_id: None,
            parent_tool_use_id: None,
            delegation_call_id: None,
        })
    }

    fn build_detail(&self, session_dir: &Path, session_id: &str) -> ConversationDetail {
        let mut parsed = parse_session_log(session_dir).unwrap_or_default();
        normalize_turns(&mut parsed.turns, parsed.cwd.as_deref());
        let summary = self
            .summary_from(session_id, &parsed)
            .unwrap_or(ConversationSummary {
                id: session_id.to_string(),
                agent_type: AgentType::DeepSeek,
                folder_path: parsed.cwd.clone(),
                folder_name: parsed.cwd.as_deref().map(folder_name_from_path),
                title: parsed.title.clone().or(parsed.first_user_text.clone()),
                started_at: Utc::now(),
                ended_at: parsed.last_ts.clone(),
                message_count: parsed.message_count,
                model: parsed.model.clone(),
                git_branch: None,
                parent_id: None,
                parent_tool_use_id: None,
                delegation_call_id: None,
            });
        let session_stats = build_session_stats(&parsed);
        ConversationDetail {
            summary,
            turns: parsed.turns,
            session_stats,
            transcript_watermark: None,
        }
    }

    fn find_session_dir(&self, conversation_id: &str) -> Option<PathBuf> {
        if !is_safe_subagent_id(conversation_id) {
            return None;
        }
        read_subdirs(&self.base_dir)
            .into_iter()
            .map(|bucket| bucket.join(conversation_id))
            .find(|candidate| candidate.is_dir())
    }
}

impl Default for DeepSeekParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentParser for DeepSeekParser {
    fn list_conversations(&self) -> Result<Vec<ConversationSummary>, ParseError> {
        if !self.base_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut conversations = Vec::new();
        for bucket in read_subdirs(&self.base_dir) {
            for session_dir in read_subdirs(&bucket) {
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

fn normalize_turns(turns: &mut Vec<MessageTurn>, cwd: Option<&str>) {
    relocate_orphaned_tool_results(turns);
    structurize_read_tool_output(turns);
    resolve_patch_line_numbers(turns, cwd);
}

fn build_session_stats(parsed: &SessionParse) -> Option<SessionStats> {
    let max_tokens = parsed
        .context_window
        .or_else(|| infer_context_window_max_tokens(parsed.model.as_deref()));
    merge_context_window_stats(
        compute_session_stats(&parsed.turns),
        parsed.last_step_input_side,
        max_tokens,
    )
}

fn parse_session_log(session_dir: &Path) -> Option<SessionParse> {
    storage::read_session_log_text(session_dir).map(|text| EventState::parse(&text))
}
