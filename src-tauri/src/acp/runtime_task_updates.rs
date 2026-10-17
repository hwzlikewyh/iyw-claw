use super::{is_terminal, preview, text, RuntimeTask, RuntimeTasks};
use crate::models::agent::AgentType;
use serde_json::{Map, Value};

pub(super) fn meta(state: &mut RuntimeTasks, agent: AgentType, meta: &Map<String, Value>) -> bool {
    let goal = meta
        .get("codex")
        .and_then(|v| v.get("goal"))
        .or_else(|| meta.get("goal"));
    let mut changed = goal
        .filter(|value| {
            agent != AgentType::ClaudeCode || value.get("createdAt").is_some_and(Value::is_number)
        })
        .is_some_and(|value| super::goals::observe(state, value));
    if agent == AgentType::Codex {
        if meta.get("iyw").is_some_and(|v| v["backgroundTasksUncertain"] == true) {
            changed |= super::terminals::unavailable(state);
        }
        if let Some(tasks) = meta.get("iyw").and_then(|v| v.get("backgroundTasks")) {
            changed |= super::terminals::reconcile(state, tasks);
        }
        if let Some(task) = meta.get("iyw").and_then(|v| v.get("backgroundTask")) {
            changed |= super::terminals::observe(state, task);
        }
        if let Some(task) = meta.get("iyw").and_then(|v| v.get("runtimeTask")) {
            changed |= observe_native_task(state, task);
        }
    }
    if agent == AgentType::ClaudeCode {
        if let Some(task) = meta.get("iyw").and_then(|value| value.get("asyncTask")) {
            changed |= super::background::air(state, task);
        }
    }
    changed
}

fn observe_native_task(state: &mut RuntimeTasks, value: &Value) -> bool {
    let Some(native_id) = text(value, &["id"]) else {
        return false;
    };
    let id = format!("subagent:{native_id}");
    let mut task = state
        .tasks
        .get(&id)
        .cloned()
        .unwrap_or_else(|| RuntimeTask {
            id,
            kind: "subagent".into(),
            native_id: native_id.clone(),
            title: native_id,
            agent_type: Some(AgentType::Codex),
            ..Default::default()
        });
    if let Some(status) = text(value, &["status"]) {
        if status != "starting" || task.status.is_empty() {
            task.status = status;
        }
    }
    if let Some(title) = text(value, &["title", "path"]) {
        task.title = title;
    }
    task.transcript_id = Some(task.native_id.clone());
    task.parent_id = text(value, &["parentId"]).or(task.parent_id);
    task.model = text(value, &["model"]).or(task.model);
    task.summary = text(value, &["summary"]).or(task.summary);
    if let Some(delta) = text(value, &["delta"]) {
        task.output = Some(preview(&(task.output.unwrap_or_default() + &delta)));
    }
    task.actions = if matches!(task.status.as_str(), "running" | "blocked") {
        vec!["stop".into()]
    } else {
        vec![]
    };
    state.upsert(task)
}

pub(super) fn sdk(state: &mut RuntimeTasks, message: &Value) -> bool {
    if !accept_sdk_event(state, message) {
        return false;
    }
    sdk_update(state, message)
}

fn accept_sdk_event(state: &mut RuntimeTasks, message: &Value) -> bool {
    if let Some(uuid) = message["uuid"].as_str() {
        if !state.seen_sdk_events.insert(uuid.into()) {
            return false;
        }
        if state.seen_sdk_events.len() > super::SDK_EVENT_LIMIT {
            state.seen_sdk_events.pop_first();
        }
    }
    true
}

fn sdk_update(state: &mut RuntimeTasks, message: &Value) -> bool {
    if message["type"] == "active_goal" {
        return super::goals::sdk(state, message);
    }
    let mut schedule_changed = super::schedules::observe(state, message);
    if message["subtype"] == "background_tasks_changed" {
        return super::background::reconcile_sdk(state, message) || schedule_changed;
    }
    if (message["type"] == "assistant"
        || matches!(
            message["subtype"].as_str(),
            Some("compact_boundary" | "api_retry")
        ))
        && message.get("request_id").and_then(Value::as_str).is_none()
        && !state.billing_incomplete
    {
        state.billing_incomplete = true;
        state.revision = state.revision.saturating_add(1);
        state.persistence_revision = state.persistence_revision.saturating_add(1);
        schedule_changed = true;
    }
    if message.get("type").and_then(Value::as_str) == Some("assistant") {
        remember_goal_tools(state, message);
        return observe_subagent_output(state, message) || schedule_changed;
    }
    let kind = message
        .get("subtype")
        .or_else(|| message.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "task_started" | "task_progress" | "task_notification" | "task_updated" => {
            observe_sdk_task(state, message, kind) || schedule_changed
        }
        _ => schedule_changed,
    }
}

fn observe_sdk_task(state: &mut RuntimeTasks, value: &Value, kind: &str) -> bool {
    let Some(native_id) = text(value, &["task_id", "taskId"]) else {
        return false;
    };
    let id = format!("task:{native_id}");
    let is_subagent = value["task_type"] == "local_agent" || value["subagent_type"].as_str().is_some()
        || state.tasks.get(&id).is_some_and(|task| task.kind == "subagent");
    if !is_subagent {
        return super::background::sdk(state, value, kind);
    }
    let mut task = state
        .tasks
        .get(&id)
        .cloned()
        .unwrap_or_else(|| RuntimeTask {
            id,
            kind: "background".into(),
            native_id: native_id.clone(),
            title: native_id,
            status: "running".into(),
            ..Default::default()
        });
    apply_sdk_details(state, &mut task, value);
    apply_sdk_status(&mut task, value, kind);
    if is_terminal(&task.status) {
        task.actions.clear();
    }
    // 仅 asyncTasks 协商出的 canStop 允许显示停止；子代理不能冒用后台任务控制。
    state.upsert(task)
}

fn observe_subagent_output(state: &mut RuntimeTasks, value: &Value) -> bool {
    let Some(parent) = text(value, &["parent_tool_use_id"]) else {
        return false;
    };
    let Some(mut task) = state
        .tasks
        .values()
        .find(|task| task.tool_call_id.as_deref() == Some(parent.as_str()))
        .cloned()
    else {
        return false;
    };
    let content = value.pointer("/message/content").and_then(Value::as_array);
    let text = content
        .into_iter()
        .flatten()
        .filter_map(|block| {
            (block.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| block.get("text").and_then(Value::as_str))
                .flatten()
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return false;
    }
    task.output = Some(preview(&(task.output.unwrap_or_default() + &text)));
    task.model = value
        .pointer("/message/model")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or(task.model);
    state.upsert(task)
}

fn apply_sdk_details(state: &RuntimeTasks, task: &mut RuntimeTask, value: &Value) {
    if text(value, &["task_type", "taskType"]).as_deref() == Some("local_agent")
        || value["subagent_type"].as_str().is_some()
    {
        task.kind = "subagent".into();
        task.agent_type = Some(AgentType::ClaudeCode);
    }
    task.title = text(value, &["description", "name"]).unwrap_or_else(|| task.title.clone());
    task.tool_call_id = text(value, &["tool_use_id", "toolCallId"]).or(task.tool_call_id.take());
    task.parent_id = task
        .tool_call_id
        .as_ref()
        .and_then(|id| state.pending_tools.get(id))
        .and_then(|call| state.parent_task_for(call))
        .or(task.parent_id.take());
    if let Some(goal) = task
        .tool_call_id
        .as_ref()
        .and_then(|id| state.tool_goals.get(id))
        .cloned()
    {
        task.goal_scope = Some(goal);
    }
    task.output_file = text(value, &["output_file", "outputFilePath"]).or(task.output_file.take());
    task.summary = text(value, &["summary"]).or(task.summary.take());
    if let Some(prompt) = text(value, &["prompt"]) {
        task.objective = Some(prompt);
    }
    if task.objective.is_none() {
        task.objective = task
            .tool_call_id
            .as_ref()
            .and_then(|id| state.pending_tools.get(id))
            .and_then(|call| text(&call["input"], &["prompt"]));
    }
}

fn apply_sdk_status(task: &mut RuntimeTask, value: &Value, kind: &str) {
    if kind == "task_started" {
        task.status = "running".into();
    }
    if kind == "task_notification" {
        task.status = text(value, &["status"]).unwrap_or_else(|| "unknown".into());
    } else if kind == "task_updated" {
        if let Some(patch) = value.get("patch") {
            if let Some(status) = text(patch, &["status"]) {
                task.status = normalized_sdk_status(&status).into();
            }
            task.title = text(patch, &["description"]).unwrap_or_else(|| task.title.clone());
            task.summary = text(patch, &["error"]).or(task.summary.take());
        }
    }
}

fn normalized_sdk_status(status: &str) -> &str {
    match status {
        "killed" => "stopped",
        "pending" => "starting",
        _ => status,
    }
}

fn remember_goal_tools(state: &mut RuntimeTasks, message: &Value) {
    let Some(goal) = state.receipt_goal(message) else {
        return;
    };
    for block in message
        .pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "tool_use")
    {
        if let Some(id) = block["id"].as_str() {
            state.tool_goals.insert(id.into(), goal.clone());
        }
    }
}
