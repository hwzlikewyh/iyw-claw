//! 宿主维护原生持续任务事实；组件挂载和父回合结束不改变任务生命周期。
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::agent::AgentType;

#[path = "runtime_task_control.rs"]
pub mod control;
#[path = "runtime_task_background.rs"]
mod background;
#[path = "runtime_task_background_tools.rs"]
mod background_tools;
#[path = "runtime_task_terminals.rs"]
mod terminals;
pub use background::{BackgroundInfo, RuntimeTaskActivity};
#[path = "runtime_task_delegations.rs"]
mod delegations;
#[path = "runtime_task_detail.rs"]
mod detail;
#[path = "runtime_task_events.rs"]
mod events;
#[path = "runtime_task_goals.rs"]
mod goals;
#[path = "runtime_task_history.rs"]
mod history;
#[path = "runtime_task_schedules.rs"]
mod schedules;
#[path = "runtime_task_snapshot.rs"]
mod snapshot;
#[path = "runtime_task_updates.rs"]
mod updates;
pub(crate) use events::{observe_meta, observe_sdk};

const TERMINAL_TASK_LIMIT: usize = 100;
const OUTPUT_PREVIEW_CHARS: usize = 8_000;
const SDK_EVENT_LIMIT: usize = 2_048;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RuntimeTask {
    pub id: String,
    pub kind: String,
    pub native_id: String,
    pub title: String,
    pub status: String,
    pub actions: Vec<String>,
    pub parent_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub objective: Option<String>,
    pub summary: Option<String>,
    pub output: Option<String>,
    pub output_file: Option<String>,
    pub model: Option<String>,
    pub schedule: Option<String>,
    pub next_run: Option<String>,
    pub updated_at: String,
    pub child_conversation_id: Option<i32>,
    pub child_connection_id: Option<String>,
    pub agent_type: Option<AgentType>,
    pub goal_scope: Option<String>,
    #[serde(default)]
    pub transcript_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<BackgroundInfo>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RuntimeTaskSnapshot {
    pub tasks: Vec<RuntimeTask>,
    pub revision: u64,
    #[serde(default)]
    pub billing_children: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub billing_incomplete: bool,
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeTasks {
    tasks: BTreeMap<String, RuntimeTask>,
    revision: u64,
    persistence_revision: u64,
    pub(crate) restored: bool,
    goal_observed: bool,
    tool_goals: BTreeMap<String, String>,
    pending_tools: BTreeMap<String, Value>,
    seen_sdk_events: BTreeSet<String>,
    goal_actions: Vec<String>,
    billing_children: BTreeMap<String, Option<String>>,
    billing_incomplete: bool,
    live_background: BTreeSet<String>,
}

impl RuntimeTasks {
    pub(crate) fn negotiate(
        &mut self,
        agent: AgentType,
        meta: Option<&serde_json::Map<String, Value>>,
    ) {
        self.goal_actions = if agent == AgentType::Codex {
            vec!["pause".into(), "resume".into(), "clear".into()]
        } else if agent == AgentType::ClaudeCode
            && meta
                .and_then(|m| m.get("goal"))
                .and_then(|g| g.get("controlMethod"))
                .and_then(Value::as_str)
                == Some("_session/goal")
            && meta
                .and_then(|m| m.get("goal"))
                .and_then(|g| g.get("actions"))
                .and_then(Value::as_array)
                .is_some_and(|actions| {
                    actions
                        .iter()
                        .any(|action| action.as_str() == Some("clear"))
                })
        {
            vec!["clear".into()]
        } else {
            Vec::new()
        };
    }

    pub(crate) fn snapshot(&self) -> RuntimeTaskSnapshot {
        RuntimeTaskSnapshot {
            tasks: self.tasks.values().cloned().collect(),
            revision: self.revision,
            billing_children: self.billing_children.clone(),
            billing_incomplete: self.billing_incomplete,
        }
    }

    pub(crate) fn get(&self, id: &str) -> Option<&RuntimeTask> {
        self.tasks.get(id)
    }

    pub(crate) fn switch_session(&mut self) -> RuntimeTaskSnapshot {
        let mut previous = self.snapshot();
        for task in &mut previous.tasks {
            task.output = None;
        }
        let actions = std::mem::take(&mut self.goal_actions);
        *self = Self {
            goal_actions: actions,
            ..Default::default()
        };
        previous
    }

    fn upsert(&mut self, mut task: RuntimeTask) -> bool {
        if task.kind == "subagent" {
            self.billing_children
                .insert(task.native_id.clone(), task.parent_id.clone());
        }
        let persistent = self.tasks.get(&task.id).is_none_or(|previous| {
            previous.status != task.status
                || previous.kind != task.kind
                || previous.title != task.title
                || previous.native_id != task.native_id
                || previous.tool_call_id != task.tool_call_id
                || previous.goal_scope != task.goal_scope
                || previous.objective != task.objective
                || previous.summary != task.summary
                || previous.schedule != task.schedule
                || previous.next_run != task.next_run
                || previous.actions != task.actions
                || previous.parent_id != task.parent_id
                || previous.transcript_id != task.transcript_id
                || previous.model != task.model
                || previous.agent_type != task.agent_type
                || previous.child_conversation_id != task.child_conversation_id
                || previous.child_connection_id != task.child_connection_id
                || previous.background != task.background
        });
        if let Some(previous) = self.tasks.get(&task.id) {
            task.updated_at.clone_from(&previous.updated_at);
            if previous == &task {
                return false;
            }
        }
        task.updated_at = chrono::Utc::now().to_rfc3339();
        self.tasks.insert(task.id.clone(), task);
        self.revision = self.revision.saturating_add(1);
        if persistent {
            self.persistence_revision = self.persistence_revision.saturating_add(1);
        }
        self.prune_terminal();
        true
    }

    fn prune_terminal(&mut self) {
        let mut terminal: Vec<_> = self
            .tasks
            .values()
            .filter(|task| is_terminal(&task.status))
            .map(|task| (task.updated_at.clone(), task.id.clone()))
            .collect();
        terminal.sort();
        let excess = terminal.len().saturating_sub(TERMINAL_TASK_LIMIT);
        for (_, id) in terminal.into_iter().take(excess) {
            self.live_background.remove(&id);
            self.tasks.remove(&id);
        }
    }

    pub(crate) fn observe_meta(
        &mut self,
        agent: AgentType,
        meta: &serde_json::Map<String, Value>,
    ) -> bool {
        updates::meta(self, agent, meta)
    }

    pub(crate) fn observe_sdk(&mut self, message: &Value) -> bool {
        updates::sdk(self, message)
    }

    pub(crate) fn goal_id(&self) -> Option<String> {
        self.tasks
            .get("goal")
            .filter(|task| task.status == "active" && task.native_id != "unverified")
            .map(|task| task.native_id.clone())
    }

    pub(crate) fn receipt_goal(&self, message: &Value) -> Option<String> {
        if let Some(parent) = message.get("parent_tool_use_id").and_then(Value::as_str) {
            return self
                .tasks
                .values()
                .find(|task| task.tool_call_id.as_deref() == Some(parent))
                .and_then(|task| task.goal_scope.clone());
        }
        self.goal_id()
    }

    fn parent_task_for(&self, call: &Value) -> Option<String> {
        let parent = call["_iywParentToolUseId"].as_str()?;
        self.tasks
            .values()
            .find(|task| task.tool_call_id.as_deref() == Some(parent))
            .map(|task| task.id.clone())
    }

    pub(crate) fn restore(&mut self, snapshot: RuntimeTaskSnapshot) {
        self.billing_children.extend(snapshot.billing_children);
        self.billing_incomplete |= snapshot.billing_incomplete;
        for mut task in snapshot.tasks {
            if self.tasks.contains_key(&task.id) || task.id == "goal" && self.goal_observed {
                continue;
            }
            if !is_terminal(&task.status) {
                task.status = "unknown".into();
            }
            task.actions.clear();
            self.upsert(task);
        }
    }
}

pub fn is_terminal(status: &str) -> bool {
    matches!(
        status,
        "completed"
            | "complete"
            | "failed"
            | "stopped"
            | "cancelled"
            | "cleared"
            | "budget_limited"
            | "budgetLimited"
    )
}

pub(crate) fn text(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

pub(crate) fn preview(value: &str) -> String {
    value
        .chars()
        .rev()
        .take(OUTPUT_PREVIEW_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
