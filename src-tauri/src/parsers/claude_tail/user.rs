use chrono::Utc;

use crate::models::{ContentBlock, MessageRole, UnifiedMessage};
use crate::parsers::{claude, is_safe_subagent_id};

use super::ClaudeTailAccumulator;

impl ClaudeTailAccumulator {
    pub(super) fn feed_user(&mut self, value: &serde_json::Value) {
        self.lifecycle.observe_notification(value);
        let mut content = claude::extract_user_content(value);
        if content.is_empty() {
            return;
        }
        if let Some(result) = value.get("toolUseResult") {
            apply_structured_patch(result, &mut content);
            self.lifecycle.observe_ack(result, &content);
            apply_agent_stats(result, &mut content, &self.session_path);
        }
        let timestamp = claude::parse_timestamp(value).unwrap_or_else(Utc::now);
        let role = if claude::is_context_continuation(&content) {
            MessageRole::System
        } else {
            MessageRole::User
        };
        if matches!(role, MessageRole::User) && self.metadata.title.is_none() {
            if let Some(text) = content.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            }) {
                self.metadata.title = Some(crate::parsers::title_from_user_text(text));
            }
        }
        self.messages.push(UnifiedMessage {
            id: record_id(value),
            role,
            content,
            timestamp,
            usage: None,
            duration_ms: None,
            model: None,
            completed_at: Some(timestamp),
        });
    }
}

fn apply_structured_patch(result: &serde_json::Value, content: &mut [ContentBlock]) {
    let Some(patch) = result.get("structuredPatch") else {
        return;
    };
    let file_path = result
        .get("filePath")
        .and_then(|value| value.as_str())
        .unwrap_or("file");
    let Some(diff) = claude::rebuild_diff_from_structured_patch(file_path, patch) else {
        return;
    };
    if let Some(ContentBlock::ToolResult { output_preview, .. }) =
        content.iter_mut().find(|block| {
            matches!(
                block,
                ContentBlock::ToolResult {
                    is_error: false,
                    ..
                }
            )
        })
    {
        *output_preview = Some(diff);
    }
}

fn apply_agent_stats(
    result: &serde_json::Value,
    content: &mut [ContentBlock],
    session_path: &std::path::Path,
) {
    if result.get("agentType").is_none() {
        return;
    }
    let mut stats = claude::extract_agent_execution_stats(result);
    if let Some(agent_id) = result.get("agentId").and_then(|value| value.as_str()) {
        if is_safe_subagent_id(agent_id) {
            let path = session_path
                .with_extension("")
                .join("subagents")
                .join(format!("agent-{agent_id}.jsonl"));
            if path.exists() {
                stats.tool_calls = claude::parse_subagent_tool_calls(&path);
            }
        }
    }
    if let Some(ContentBlock::ToolResult { agent_stats, .. }) = content
        .iter_mut()
        .find(|block| matches!(block, ContentBlock::ToolResult { .. }))
    {
        *agent_stats = Some(stats);
    }
}

pub(super) fn record_id(value: &serde_json::Value) -> String {
    value
        .get("uuid")
        .and_then(|uuid| uuid.as_str())
        .unwrap_or("")
        .to_string()
}
