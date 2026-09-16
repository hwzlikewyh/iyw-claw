use std::collections::{HashMap, HashSet};
use std::time::Duration;
use std::time::Instant;

use crate::acp::types::BackgroundSettledInfo;
use crate::parsers::claude_background::{is_terminal_task_status, TaskNotification};

#[path = "accounting_records.rs"]
mod records;

struct TaskEntry {
    kind: &'static str,
    started_at: Instant,
}

pub(super) struct TaskAccounting {
    tasks: HashMap<String, TaskEntry>,
    /// Heartbeat expiry is not proof that an external task stopped. Keep such
    /// tasks protected until a terminal record or explicit stop is observed.
    uncertain_ids: HashSet<String>,
    settled_ids: HashSet<String>,
    held_turn_ids: HashSet<String>,
    was_prompting: bool,
    currently_prompting: bool,
    pending_stops: HashMap<String, String>,
    pending_resumes: HashMap<String, String>,
    launch_tools: HashMap<String, String>,
}

impl TaskAccounting {
    pub(super) fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            uncertain_ids: HashSet::new(),
            settled_ids: HashSet::new(),
            held_turn_ids: HashSet::new(),
            was_prompting: false,
            currently_prompting: false,
            pending_stops: HashMap::new(),
            pending_resumes: HashMap::new(),
            launch_tools: HashMap::new(),
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

    pub(super) fn uncertain(&self) -> bool {
        !self.uncertain_ids.is_empty()
    }

    pub(super) fn is_held_task(&self, task_id: &str) -> bool {
        self.held_turn_ids.contains(task_id)
    }

    pub(super) fn expire(&mut self, max_age: Duration) -> bool {
        let mut changed = false;
        for (task_id, entry) in &self.tasks {
            if entry.started_at.elapsed() >= max_age && self.uncertain_ids.insert(task_id.clone()) {
                changed = true;
                tracing::info!(
                    "[bg-watch] marking {} task={} uncertain after keepalive limit",
                    entry.kind,
                    task_id
                );
            }
        }
        changed
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
        let mut settled = self.observe_tool_results(value);
        if let Some(result) = value.get("toolUseResult") {
            self.observe_launch(result);
            self.remember_launch_tool(value, result);
            settled.extend(self.observe_task_output(result));
        }
        let Some(text) = user_record_text(value) else {
            return settled;
        };
        for notification in TaskNotification::parse_all(&text)
            .into_iter()
            .filter(|n| is_terminal_task_status(&n.status))
        {
            let mut outcome = self.settle(&notification.task_id, &notification.status);
            outcome.summary = notification.summary;
            outcome.tool_use_id = notification.tool_use_id.or(outcome.tool_use_id);
            outcome.result = notification.result;
            settled.push(outcome);
        }
        settled
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

    fn observe_task_output(&mut self, result: &serde_json::Value) -> Option<BackgroundSettledInfo> {
        let Some(task) = result.get("task") else {
            return None;
        };
        let Some(id) = nonempty_str(task.get("task_id")) else {
            return None;
        };
        let status = task
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if is_terminal_task_status(status) {
            if self.settled_ids.contains(id) {
                return None;
            }
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
            let mut outcome = self.settle(id, status);
            outcome.result = task.get("output").and_then(|v| v.as_str()).map(|output| {
                crate::parsers::truncate_str(
                    output,
                    crate::parsers::claude_background::BACKGROUND_RESULT_MAX_CHARS,
                )
            });
            return Some(outcome);
        }
        if status == "running" {
            self.register(id, "task");
            self.uncertain_ids.remove(id);
            if let Some(entry) = self.tasks.get_mut(id) {
                entry.started_at = Instant::now();
            }
        }
        None
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
                "SendMessage" => {
                    if let (Some(call), Some(id)) = (
                        nonempty_str(block.get("id")),
                        input.and_then(|v| nonempty_str(v.get("to"))),
                    ) {
                        self.pending_resumes
                            .insert(call.to_string(), id.to_string());
                    }
                }
                "TaskStop" | "KillShell" => {
                    if let (Some(call), Some(id)) = (
                        nonempty_str(block.get("id")),
                        input.and_then(|v| {
                            nonempty_str(v.get("task_id"))
                                .or_else(|| nonempty_str(v.get("shell_id")))
                        }),
                    ) {
                        self.pending_stops.insert(call.to_string(), id.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    fn observe_resume(&mut self, id: &str) {
        if self.settled_ids.remove(id) {
            self.register(id, "agent");
            if self.currently_prompting {
                self.held_turn_ids.insert(id.to_string());
            }
        }
    }

    fn settle(&mut self, id: &str, status: &str) -> BackgroundSettledInfo {
        self.tasks.remove(id);
        self.uncertain_ids.remove(id);
        if self.settled_ids.insert(id.to_string()) {
            tracing::info!(
                task_id = id,
                status,
                outstanding = self.tasks.len(),
                "[bg-watch] background task settled"
            );
        }
        BackgroundSettledInfo {
            task_id: id.to_string(),
            status: status.to_string(),
            summary: None,
            result: None,
            tool_use_id: self.launch_tools.get(id).cloned(),
            wire_visible: self.currently_prompting || self.held_turn_ids.contains(id),
        }
    }

    fn register(&mut self, id: &str, kind: &'static str) {
        if self.settled_ids.contains(id) {
            return;
        }
        if !self.tasks.contains_key(id) {
            tracing::info!(task_id = id, kind, "[bg-watch] background task registered");
        }
        self.uncertain_ids.remove(id);
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
