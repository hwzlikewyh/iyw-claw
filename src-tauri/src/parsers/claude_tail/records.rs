use std::collections::HashSet;

use chrono::Utc;

use crate::models::{ContentBlock, MessageRole, UnifiedMessage};
use crate::parsers::claude;

use super::user::record_id;
use super::ClaudeTailAccumulator;

impl ClaudeTailAccumulator {
    pub(super) fn feed_assistant(&mut self, value: &serde_json::Value) {
        if claude::is_synthetic_assistant(value) {
            return;
        }
        let timestamp = claude::parse_timestamp(value).unwrap_or_else(Utc::now);
        let model = value
            .get("message")
            .and_then(|message| message.get("model"))
            .and_then(|model| model.as_str())
            .map(str::to_string);
        if self.metadata.model.is_none() {
            self.metadata.model = model.clone();
        }
        self.messages.push(UnifiedMessage {
            id: record_id(value),
            role: MessageRole::Assistant,
            content: claude::extract_assistant_content(value),
            timestamp,
            usage: claude::extract_usage(value),
            duration_ms: None,
            model,
            completed_at: Some(timestamp),
        });
    }

    pub(super) fn feed_system(&mut self, value: &serde_json::Value) {
        if value.get("subtype").and_then(|kind| kind.as_str()) != Some("turn_duration") {
            return;
        }
        let Some(duration) = value.get("durationMs").and_then(|value| value.as_u64()) else {
            return;
        };
        if let Some(message) = self
            .messages
            .iter_mut()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
        {
            message.duration_ms = Some(duration);
        }
    }

    pub(super) fn feed_top_level_tool_use(&mut self, value: &serde_json::Value) {
        let timestamp = claude::parse_timestamp(value).unwrap_or_else(Utc::now);
        let name = value
            .get("tool_name")
            .and_then(|name| name.as_str())
            .unwrap_or("unknown")
            .to_string();
        let block = ContentBlock::ToolUse {
            tool_use_id: Some(format!("tl-tool-{}", self.messages.len())),
            tool_name: name,
            input_preview: value.get("tool_input").map(|input| input.to_string()),
            meta: None,
        };
        if let Some(message) = self.last_assistant_mut() {
            message.content.push(block);
            return;
        }
        self.messages.push(synthetic_assistant(
            format!("synth-assistant-{}", self.messages.len()),
            block,
            timestamp,
        ));
    }

    pub(super) fn feed_top_level_tool_result(&mut self, value: &serde_json::Value) {
        let output = value.get("tool_output");
        let is_error = output
            .and_then(|value| value.get("exit"))
            .and_then(|exit| exit.as_i64())
            .is_some_and(|exit| exit != 0);
        let preview = output
            .and_then(|value| value.get("preview").or_else(|| value.get("output")))
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let tool_name = value
            .get("tool_name")
            .and_then(|name| name.as_str())
            .unwrap_or("");
        let matching_id = self.matching_tool_use_id(tool_name);
        let block = ContentBlock::ToolResult {
            tool_use_id: matching_id,
            output_preview: preview,
            is_error,
            agent_stats: None,
            images: Vec::new(),
        };
        if let Some(message) = self.last_assistant_mut() {
            message.content.push(block);
            return;
        }
        let timestamp = claude::parse_timestamp(value).unwrap_or_else(Utc::now);
        self.messages.push(synthetic_assistant(
            format!("synth-result-{}", self.messages.len()),
            block,
            timestamp,
        ));
    }

    fn last_assistant_mut(&mut self) -> Option<&mut UnifiedMessage> {
        self.messages
            .iter_mut()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
    }

    fn matching_tool_use_id(&self, tool_name: &str) -> Option<String> {
        let message = self
            .messages
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))?;
        let paired: HashSet<&str> = message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolResult {
                    tool_use_id: Some(id),
                    ..
                } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        message
            .content
            .iter()
            .rev()
            .find_map(|block| unpaired_tool_id(block, tool_name, &paired))
            .or_else(|| {
                message
                    .content
                    .iter()
                    .rev()
                    .find_map(|block| unpaired_tool_id(block, "", &paired))
            })
    }
}

fn unpaired_tool_id(
    block: &ContentBlock,
    tool_name: &str,
    paired: &HashSet<&str>,
) -> Option<String> {
    let ContentBlock::ToolUse {
        tool_use_id: Some(id),
        tool_name: name,
        ..
    } = block
    else {
        return None;
    };
    ((tool_name.is_empty() || name == tool_name) && !paired.contains(id.as_str()))
        .then(|| id.clone())
}

fn synthetic_assistant(
    id: String,
    block: ContentBlock,
    timestamp: chrono::DateTime<Utc>,
) -> UnifiedMessage {
    UnifiedMessage {
        id,
        role: MessageRole::Assistant,
        content: vec![block],
        timestamp,
        usage: None,
        duration_ms: None,
        model: None,
        completed_at: Some(timestamp),
    }
}
