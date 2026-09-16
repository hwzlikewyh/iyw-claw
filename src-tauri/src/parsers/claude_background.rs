use std::collections::HashMap;

use crate::models::{ContentBlock, UnifiedMessage};
use crate::parsers::truncate_str;

pub(crate) const BACKGROUND_TASK_MARKER: &str = "[[codeg-background-task]]";
pub(crate) const BACKGROUND_RESULT_MAX_CHARS: usize = 20_000;

pub(crate) fn is_terminal_task_status(status: &str) -> bool {
    matches!(
        status,
        "completed"
            | "failed"
            | "canceled"
            | "cancelled"
            | "killed"
            | "stopped"
            | "interrupted"
            | "errored"
            | "timeout"
            | "timed_out"
            | "error"
    )
}

#[derive(Clone)]
pub(crate) struct TaskNotification {
    pub task_id: String,
    pub status: String,
    pub summary: Option<String>,
    pub tool_use_id: Option<String>,
    pub result: Option<String>,
}

impl TaskNotification {
    pub(crate) fn parse_all(text: &str) -> Vec<Self> {
        text.match_indices("<task-notification>")
            .filter_map(|(offset, _)| Self::parse(&text[offset..]))
            .collect()
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        if !text.starts_with("<task-notification>") {
            return None;
        }
        let text = text.split_once("</task-notification>")?.0;
        Some(Self {
            task_id: capture_tag(text, "task-id")?,
            status: capture_tag(text, "status")?,
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
        let Some(content) = value
            .get("message")
            .and_then(|message| message.get("content"))
        else {
            return;
        };
        if let Some(raw) = content.as_str() {
            self.observe_notification_text(raw);
        }
        for block in content.as_array().into_iter().flatten() {
            if block["type"] == "text" {
                if let Some(raw) = block.get("text").and_then(|v| v.as_str()) {
                    self.observe_notification_text(raw);
                }
            }
        }
    }

    fn observe_notification_text(&mut self, raw: &str) {
        for notification in TaskNotification::parse_all(raw) {
            self.notifications
                .insert(notification.task_id.clone(), notification);
        }
    }

    pub(crate) fn observe_ack(&mut self, result: &serde_json::Value, content: &[ContentBlock]) {
        self.observe_task_result(result);
        let task_id = if result["status"] == "async_launched" {
            result.get("agentId")
        } else {
            result
                .get("backgroundTaskId")
                .or_else(|| result.pointer("/task/task_id"))
        };
        let Some(task_id) = task_id
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

    fn observe_task_result(&mut self, result: &serde_json::Value) {
        let task = result.get("task").unwrap_or(result);
        let Some(task_id) = task.get("task_id").and_then(|v| v.as_str()) else {
            return;
        };
        let status = task.get("status").and_then(|v| v.as_str()).or_else(|| {
            result
                .get("message")
                .and_then(|v| v.as_str())
                .filter(|message| message.starts_with("Successfully stopped task"))
                .map(|_| "stopped")
        });
        let Some(status) = status.filter(|status| is_terminal_task_status(status)) else {
            return;
        };
        let status = if status == "completed"
            && task
                .get("exit_code")
                .and_then(|v| v.as_i64())
                .is_some_and(|code| code != 0)
        {
            "failed"
        } else {
            status
        };
        self.notifications
            .entry(task_id.to_string())
            .or_insert_with(|| TaskNotification {
                task_id: task_id.to_string(),
                status: status.to_string(),
                summary: None,
                tool_use_id: None,
                result: task
                    .get("output")
                    .and_then(|v| v.as_str())
                    .map(|v| truncate_str(v, BACKGROUND_RESULT_MAX_CHARS)),
            });
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
