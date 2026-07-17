use std::collections::{HashMap, HashSet};
use std::time::Duration;
use std::time::Instant;

use crate::acp::types::BackgroundSettledInfo;
use crate::parsers::claude_background::TaskNotification;

struct TaskEntry {
    kind: &'static str,
    started_at: Instant,
}

pub(super) struct TaskAccounting {
    tasks: HashMap<String, TaskEntry>,
    settled_ids: HashSet<String>,
    held_turn_ids: HashSet<String>,
    was_prompting: bool,
    currently_prompting: bool,
}

impl TaskAccounting {
    pub(super) fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            settled_ids: HashSet::new(),
            held_turn_ids: HashSet::new(),
            was_prompting: false,
            currently_prompting: false,
        }
    }

    pub(super) fn begin_tick(&mut self, prompting: bool, ended_abnormally: bool) {
        if prompting && !self.was_prompting {
            self.held_turn_ids.clear();
        }
        if !prompting && self.was_prompting && ended_abnormally {
            self.held_turn_ids.clear();
        }
        self.was_prompting = prompting;
        self.currently_prompting = prompting;
    }

    pub(super) fn outstanding(&self) -> u32 {
        self.tasks.len() as u32
    }

    pub(super) fn is_held_task(&self, task_id: &str) -> bool {
        self.held_turn_ids.contains(task_id)
    }

    pub(super) fn expire(&mut self, max_age: Duration) -> bool {
        let before = self.tasks.len();
        self.tasks.retain(|task_id, entry| {
            let keep = entry.started_at.elapsed() < max_age;
            if !keep {
                tracing::info!(
                    "[bg-watch] expiring {} task={} after keepalive limit",
                    entry.kind,
                    task_id
                );
            }
            keep
        });
        self.tasks.len() != before
    }

    pub(super) fn observe(&mut self, value: &serde_json::Value) -> Vec<BackgroundSettledInfo> {
        match value.get("type").and_then(|kind| kind.as_str()) {
            Some("user") => self.observe_user(value),
            Some("assistant") => {
                self.observe_assistant(value);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn observe_user(&mut self, value: &serde_json::Value) -> Vec<BackgroundSettledInfo> {
        if let Some(result) = value.get("toolUseResult") {
            self.observe_launch(result);
            self.observe_task_output(result);
        }
        let Some(text) = user_record_text(value) else {
            return Vec::new();
        };
        let Some(notification) = TaskNotification::parse(text.trim_start()) else {
            return Vec::new();
        };
        let task_id = notification.task_id.clone();
        self.tasks.remove(&task_id);
        self.settled_ids.insert(task_id.clone());
        vec![BackgroundSettledInfo {
            task_id: task_id.clone(),
            status: notification.status,
            summary: notification.summary,
            tool_use_id: notification.tool_use_id,
            result: notification.result,
            wire_visible: self.held_turn_ids.contains(&task_id),
        }]
    }

    fn observe_launch(&mut self, result: &serde_json::Value) {
        if result.get("status").and_then(|value| value.as_str()) == Some("async_launched") {
            if let Some(id) = nonempty_str(result.get("agentId")) {
                self.register(id, "agent");
                if self.currently_prompting {
                    self.held_turn_ids.insert(id.to_string());
                }
            }
            return;
        }
        if let Some(id) = nonempty_str(result.get("backgroundTaskId")) {
            self.register(id, "shell");
        }
    }

    fn observe_task_output(&mut self, result: &serde_json::Value) {
        let Some(task) = result.get("task") else {
            return;
        };
        let Some(id) = nonempty_str(task.get("task_id")) else {
            return;
        };
        let status = task
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if is_terminal_task_status(status) && self.tasks.remove(id).is_some() {
            self.settled_ids.insert(id.to_string());
        }
    }

    fn observe_assistant(&mut self, value: &serde_json::Value) {
        let Some(blocks) = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_array())
        else {
            return;
        };
        for block in blocks {
            if block.get("type").and_then(|kind| kind.as_str()) != Some("tool_use") {
                continue;
            }
            let name = block
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let input = block.get("input");
            match name {
                "SendMessage" => self.observe_resume(input),
                "TaskStop" | "KillShell" => self.observe_stop(input),
                _ => {}
            }
        }
    }

    fn observe_resume(&mut self, input: Option<&serde_json::Value>) {
        let Some(id) = input.and_then(|value| nonempty_str(value.get("to"))) else {
            return;
        };
        if self.settled_ids.remove(id) {
            self.register(id, "agent");
            if self.currently_prompting {
                self.held_turn_ids.insert(id.to_string());
            }
        }
    }

    fn observe_stop(&mut self, input: Option<&serde_json::Value>) {
        let Some(id) = input.and_then(|value| {
            nonempty_str(value.get("task_id")).or_else(|| nonempty_str(value.get("shell_id")))
        }) else {
            return;
        };
        if self.tasks.remove(id).is_some() {
            self.settled_ids.insert(id.to_string());
        }
    }

    fn register(&mut self, id: &str, kind: &'static str) {
        self.tasks.entry(id.to_string()).or_insert(TaskEntry {
            kind,
            started_at: Instant::now(),
        });
    }
}

fn nonempty_str(value: Option<&serde_json::Value>) -> Option<&str> {
    value
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
}

fn user_record_text(value: &serde_json::Value) -> Option<String> {
    let content = value.get("message")?.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let texts: Vec<&str> = content
        .as_array()?
        .iter()
        .filter(|block| block.get("type").and_then(|kind| kind.as_str()) == Some("text"))
        .filter_map(|block| block.get("text").and_then(|text| text.as_str()))
        .collect();
    (!texts.is_empty()).then(|| texts.join("\n"))
}

fn is_terminal_task_status(status: &str) -> bool {
    matches!(
        status,
        "completed"
            | "failed"
            | "canceled"
            | "cancelled"
            | "killed"
            | "stopped"
            | "timeout"
            | "timed_out"
            | "error"
    )
}
