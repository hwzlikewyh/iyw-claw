//! 后台事实与监听类型共用任务注册表；静默不等于离线。
use super::{is_terminal, text, RuntimeTask, RuntimeTasks};
use crate::models::AgentType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BackgroundInfo {
    pub task_type: String,
    pub command: Option<String>,
    pub timeout_ms: Option<u64>,
    pub persistent: Option<bool>,
    #[serde(default)]
    pub ambient: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RuntimeTaskActivity {
    pub outstanding: u32,
    pub monitors: u32,
    pub uncertain: bool,
}

impl RuntimeTasks {
    pub(crate) fn background_activity(&self) -> RuntimeTaskActivity {
        let mut activity = RuntimeTaskActivity::default();
        for task in self.tasks.values().filter(|task| {
            self.live_background.contains(&task.id)
                && !is_terminal(&task.status)
                && !task.background.as_ref().is_some_and(|info| info.ambient)
        }) {
            activity.outstanding = activity.outstanding.saturating_add(1);
            activity.monitors += u32::from(task.kind == "monitor");
            activity.uncertain |= task.status == "unknown";
        }
        activity
    }
}

pub(super) fn upsert(state: &mut RuntimeTasks, task: RuntimeTask) -> bool {
    if state
        .tasks
        .get(&task.id)
        .is_none_or(|previous| previous.status != task.status)
    {
        tracing::info!(task_id = %task.id, kind = %task.kind, status = %task.status, "[runtime-task] background state changed");
    }
    state.live_background.insert(task.id.clone());
    state.upsert(task)
}

pub(super) fn air(state: &mut RuntimeTasks, value: &Value) -> bool {
    let Some(id) = text(value, &["asyncTaskId"]) else {
        return false;
    };
    let mut task = task(state, &id);
    apply_type(&mut task, value);
    if value["sessionUpdate"] == "async_task_spawned" && !is_terminal(&task.status) {
        task.status = "running".into();
    }
    task.title = text(value, &["description", "name"]).unwrap_or(task.title);
    task.summary = text(value, &["summary"]).or(task.summary);
    task.output_file = text(value, &["outputFilePath"]).or(task.output_file);
    task.tool_call_id = text(value, &["toolCallId"]).or(task.tool_call_id);
    if let Some(status) = text(value, &["state"]) {
        apply_status(&mut task, &status);
    }
    if let Some(can_stop) = value["canStop"].as_bool() {
        task.actions = if can_stop {
            vec!["stop".into()]
        } else {
            vec![]
        };
    }
    if is_terminal(&task.status) {
        task.actions.clear();
    }
    upsert(state, task)
}

pub(super) fn sdk(state: &mut RuntimeTasks, value: &Value, kind: &str) -> bool {
    let Some(id) = text(value, &["task_id"]) else {
        return false;
    };
    if kind == "task_started"
        && (value["is_backgrounded"] == false
            || value["task_type"] == "local_bash" && value["is_backgrounded"] != true)
    {
        return false;
    }
    let key = format!("task:{id}");
    if kind != "task_started" && !state.tasks.contains_key(&key) {
        return false;
    }
    let mut task = task(state, &id);
    apply_type(&mut task, value);
    task.title = text(value, &["description"]).unwrap_or(task.title);
    task.summary = text(value, &["summary"]).or(task.summary);
    task.tool_call_id = text(value, &["tool_use_id"]).or(task.tool_call_id);
    task.output_file = text(value, &["output_file"]).or(task.output_file);
    if kind == "task_notification" {
        apply_status(&mut task, value["status"].as_str().unwrap_or("unknown"));
    } else if let Some(status) = value.pointer("/patch/status").and_then(Value::as_str) {
        apply_status(&mut task, status);
        task.summary = text(&value["patch"], &["error"]).or(task.summary);
    }
    if is_terminal(&task.status) {
        task.actions.clear();
    }
    upsert(state, task)
}

pub(super) fn reconcile_sdk(state: &mut RuntimeTasks, value: &Value) -> bool {
    let Some(tasks) = value["tasks"].as_array() else {
        return false;
    };
    let mut changed = false;
    for item in tasks {
        let Some(id) = text(item, &["task_id"]) else {
            continue;
        };
        if item["task_type"] == "local_agent" {
            continue;
        }
        let mut task = task(state, &id);
        apply_type(&mut task, item);
        task.title = text(item, &["description"]).unwrap_or(task.title);
        if !is_terminal(&task.status) {
            task.status = "running".into();
        }
        changed |= upsert(state, task);
    }
    // 缺少边沿通知不能冒充完成；AIR 随后的生命周期会确认真实终态。
    let missing: Vec<_> = state
        .tasks
        .values()
        .filter(|task| {
            task.agent_type == Some(AgentType::ClaudeCode)
                && state.live_background.contains(&task.id)
                && !is_terminal(&task.status)
                && !tasks
                    .iter()
                    .any(|item| item["task_id"].as_str() == Some(task.native_id.as_str()))
        })
        .cloned()
        .collect();
    for mut task in missing {
        task.status = "unknown".into();
        task.actions.clear();
        let removed = state.live_background.remove(&task.id);
        changed |= state.upsert(task) || removed;
    }
    changed
}

pub(super) fn task(state: &RuntimeTasks, native: &str) -> RuntimeTask {
    let mut task = state
        .tasks
        .get(&format!("task:{native}"))
        .cloned()
        .unwrap_or_else(|| RuntimeTask {
            id: format!("task:{native}"),
            native_id: native.into(),
            title: native.into(),
            kind: "background".into(),
            status: "running".into(),
            agent_type: Some(AgentType::ClaudeCode),
            background: Some(BackgroundInfo::default()),
            ..Default::default()
        });
    task.background.get_or_insert_with(BackgroundInfo::default);
    task
}

fn apply_type(task: &mut RuntimeTask, value: &Value) {
    let info = task.background.get_or_insert_with(BackgroundInfo::default);
    if let Some(kind) = text(value, &["taskType", "task_type"]) {
        info.task_type = kind.clone();
        task.kind = if matches!(kind.as_str(), "monitor" | "local_monitor") {
            "monitor"
        } else {
            "background"
        }
        .into();
    }
    if let Some(ambient) = value["ambient"]
        .as_bool()
        .or_else(|| value["skip_transcript"].as_bool())
    {
        info.ambient = ambient;
    }
}

fn apply_status(task: &mut RuntimeTask, status: &str) {
    let status = match status {
        "killed" => "stopped",
        "pending" => "starting",
        other => other,
    };
    if task.status != "stopped" && (!is_terminal(&task.status) || is_terminal(status)) {
        task.status = status.into();
    }
}
