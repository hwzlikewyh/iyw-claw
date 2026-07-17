use std::collections::HashMap;

use crate::models::{ContentBlock, UnifiedMessage};
use crate::parsers::truncate_str;

pub(crate) const BACKGROUND_TASK_MARKER: &str = "[[codeg-background-task]]";
pub(crate) const BACKGROUND_RESULT_MAX_CHARS: usize = 20_000;

#[derive(Clone)]
pub(crate) struct TaskNotification {
    pub task_id: String,
    pub status: String,
    pub summary: Option<String>,
    pub tool_use_id: Option<String>,
    pub result: Option<String>,
}

impl TaskNotification {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        if !text.starts_with("<task-notification>") {
            return None;
        }
        Some(Self {
            task_id: capture_tag(text, "task-id")?,
            status: capture_tag(text, "status").unwrap_or_else(|| "completed".into()),
            summary: capture_tag(text, "summary"),
            tool_use_id: capture_tag(text, "tool-use-id"),
            result: capture_tag(text, "result")
                .map(|result| truncate_str(&result, BACKGROUND_RESULT_MAX_CHARS)),
        })
    }
}

fn capture_tag(text: &str, tag: &str) -> Option<String> {
    let opening = format!("<{tag}>");
    let closing = format!("</{tag}>");
    let (_, tail) = text.split_once(&opening)?;
    let (value, _) = tail.split_once(&closing)?;
    Some(value.trim().to_string())
}

pub(crate) struct BackgroundLifecycle {
    acknowledgements: HashMap<String, String>,
    notifications: HashMap<String, TaskNotification>,
}

impl BackgroundLifecycle {
    pub(crate) fn new() -> Self {
        Self {
            acknowledgements: HashMap::new(),
            notifications: HashMap::new(),
        }
    }

    pub(crate) fn observe_notification(&mut self, value: &serde_json::Value) {
        let Some(raw) = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
        else {
            return;
        };
        let Some(notification) = TaskNotification::parse(raw.trim_start()) else {
            return;
        };
        self.notifications
            .insert(notification.task_id.clone(), notification);
    }

    pub(crate) fn observe_ack(&mut self, result: &serde_json::Value, content: &[ContentBlock]) {
        if result.get("status").and_then(|status| status.as_str()) != Some("async_launched") {
            return;
        }
        let Some(task_id) = result
            .get("agentId")
            .and_then(|id| id.as_str())
            .filter(|id| !id.is_empty())
        else {
            return;
        };
        let Some(tool_use_id) = content.iter().find_map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id: Some(id),
                ..
            } => Some(id.clone()),
            _ => None,
        }) else {
            return;
        };
        self.acknowledgements
            .insert(tool_use_id, task_id.to_string());
    }

    pub(crate) fn apply(&self, messages: &mut [UnifiedMessage]) {
        for message in messages {
            for block in &mut message.content {
                self.apply_block(block);
            }
        }
    }

    fn apply_block(&self, block: &mut ContentBlock) {
        let ContentBlock::ToolResult {
            tool_use_id: Some(tool_use_id),
            output_preview,
            is_error: false,
            ..
        } = block
        else {
            return;
        };
        let Some(task_id) = self.acknowledgements.get(tool_use_id) else {
            return;
        };
        let notification = self.notifications.get(task_id);
        let payload = serde_json::json!({
            "task_id": task_id,
            "status": notification.map(|value| value.status.clone()),
            "summary": notification.and_then(|value| value.summary.clone()),
            "result": notification.and_then(|value| value.result.clone()),
        });
        *output_preview = Some(format!("{BACKGROUND_TASK_MARKER}{payload}"));
    }
}
