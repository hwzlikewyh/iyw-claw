mod lifecycle;
mod records;
mod user;

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::models::{ContentBlock, MessageRole, MessageTurn, TurnRole, UnifiedMessage};
use crate::parsers::claude::{
    is_meta_message, is_slash_command_expansion, slash_command_value_display,
};

use lifecycle::BackgroundLifecycle;

#[derive(Clone)]
pub(crate) struct ClaudeTailMetadata {
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
}

pub(crate) struct ClaudeTailAccumulator {
    session_path: PathBuf,
    messages: Vec<UnifiedMessage>,
    pending_command: Option<(UnifiedMessage, Option<String>)>,
    lifecycle: BackgroundLifecycle,
    metadata: ClaudeTailMetadata,
    ai_title: Option<String>,
}

impl ClaudeTailAccumulator {
    pub(crate) fn new(session_path: PathBuf) -> Self {
        Self {
            session_path,
            messages: Vec::new(),
            pending_command: None,
            lifecycle: BackgroundLifecycle::new(),
            metadata: ClaudeTailMetadata {
                cwd: None,
                git_branch: None,
                model: None,
                title: None,
                first_timestamp: None,
                last_timestamp: None,
            },
            ai_title: None,
        }
    }

    pub(crate) fn feed_value(&mut self, value: serde_json::Value) {
        let record_type = value
            .get("type")
            .and_then(|kind| kind.as_str())
            .unwrap_or("");
        if record_type == "file-history-snapshot" || record_type == "progress" {
            return;
        }
        self.resolve_pending_command(&value);
        if is_meta_message(&value) {
            return;
        }
        self.observe_metadata(&value, record_type);
        if record_type == "user" && self.buffer_slash_command(&value) {
            return;
        }
        match record_type {
            "assistant" => self.feed_assistant(&value),
            "user" => self.feed_user(&value),
            "system" => self.feed_system(&value),
            "tool_use" => self.feed_top_level_tool_use(&value),
            "tool_result" => self.feed_top_level_tool_result(&value),
            _ => {}
        }
    }

    pub(crate) fn message_count(&self) -> usize {
        self.messages.len()
    }

    #[cfg(test)]
    pub(crate) fn metadata(&self) -> ClaudeTailMetadata {
        let mut metadata = self.metadata.clone();
        metadata.title = self.ai_title.clone().or(metadata.title);
        metadata
    }

    pub(crate) fn collect_turns(&self, cwd: Option<&str>) -> Vec<MessageTurn> {
        let mut messages = self.messages.clone();
        self.lifecycle.apply(&mut messages);
        let mut turns = crate::parsers::claude::group_into_turns(messages);
        crate::parsers::relocate_orphaned_tool_results(&mut turns);
        crate::parsers::structurize_read_tool_output(&mut turns);
        crate::parsers::resolve_patch_line_numbers(&mut turns, cwd);
        strip_private_user_context_from_turns(&mut turns);
        turns
    }

    fn resolve_pending_command(&mut self, value: &serde_json::Value) {
        let Some((message, prompt_id)) = self.pending_command.take() else {
            return;
        };
        if is_slash_command_expansion(value, prompt_id.as_deref()) {
            self.messages.push(message);
        }
    }

    fn buffer_slash_command(&mut self, value: &serde_json::Value) -> bool {
        let Some((display, prompt_id)) = slash_command_value_display(value) else {
            return false;
        };
        let timestamp = crate::parsers::claude::parse_timestamp(value).unwrap_or_else(Utc::now);
        let id = value
            .get("uuid")
            .and_then(|uuid| uuid.as_str())
            .unwrap_or("")
            .to_string();
        self.pending_command = Some((
            UnifiedMessage {
                id,
                role: MessageRole::User,
                content: vec![ContentBlock::Text { text: display }],
                timestamp,
                usage: None,
                duration_ms: None,
                model: None,
                completed_at: Some(timestamp),
            },
            prompt_id,
        ));
        true
    }

    fn observe_metadata(&mut self, value: &serde_json::Value, record_type: &str) {
        if record_type == "ai-title" {
            if let Some(title) = value.get("aiTitle").and_then(|title| title.as_str()) {
                let visible = crate::user_memory::strip_user_context(title);
                let title = visible.trim();
                if !title.is_empty() {
                    self.ai_title = Some(crate::parsers::truncate_str(title, 100));
                }
            }
        }
        if self.metadata.cwd.is_none() {
            self.metadata.cwd = value
                .get("cwd")
                .and_then(|value| value.as_str())
                .map(str::to_string);
        }
        if self.metadata.git_branch.is_none() {
            self.metadata.git_branch = value
                .get("gitBranch")
                .and_then(|value| value.as_str())
                .map(str::to_string);
        }
        let Some(timestamp) = crate::parsers::claude::parse_timestamp(value) else {
            return;
        };
        if self.metadata.first_timestamp.is_none() {
            self.metadata.first_timestamp = Some(timestamp);
        }
        self.metadata.last_timestamp = Some(timestamp);
    }
}

pub(super) fn strip_private_user_context_from_content(content: &mut Vec<ContentBlock>) {
    content.retain_mut(|block| match block {
        ContentBlock::Text { text } => {
            *text = crate::user_memory::strip_user_context(text);
            !text.trim().is_empty()
        }
        _ => true,
    });
}

fn strip_private_user_context_from_turns(turns: &mut Vec<MessageTurn>) {
    for turn in turns.iter_mut() {
        if matches!(turn.role, TurnRole::User) {
            strip_private_user_context_from_content(&mut turn.blocks);
        }
    }
    turns.retain(|turn| !matches!(turn.role, TurnRole::User) || !turn.blocks.is_empty());
}
