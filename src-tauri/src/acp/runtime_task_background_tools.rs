use super::{background, is_terminal, text, RuntimeTasks};
use serde_json::Value;

pub(super) fn observe(state: &mut RuntimeTasks, call: &Value, output: &Value) -> bool {
    match call["name"].as_str() {
        Some("Monitor" | "Bash" | "PowerShell") => launched(state, call, output),
        Some("TaskStop") => stopped(state, output),
        _ => false,
    }
}

fn launched(state: &mut RuntimeTasks, call: &Value, output: &Value) -> bool {
    let monitor = call["name"] == "Monitor";
    let Some(id) = text(
        output,
        if monitor {
            &["taskId"]
        } else {
            &["backgroundTaskId"]
        },
    ) else {
        return false;
    };
    let mut task = background::task(state, &id);
    let info = task
        .background
        .get_or_insert_with(super::BackgroundInfo::default);
    info.task_type = if monitor {
        "local_monitor"
    } else {
        "local_bash"
    }
    .into();
    info.command = text(&call["input"], &["command"]);
    if monitor {
        info.timeout_ms = output["timeoutMs"].as_u64();
        info.persistent = output["persistent"].as_bool();
    }
    task.kind = if monitor { "monitor" } else { "background" }.into();
    task.tool_call_id = text(call, &["id"]).or(task.tool_call_id);
    task.parent_id = state.parent_task_for(call).or(task.parent_id);
    task.goal_scope = text(call, &["id"])
        .and_then(|id| state.tool_goals.get(&id).cloned())
        .or(task.goal_scope);
    task.title = text(&call["input"], &["description", "command"]).unwrap_or(task.title);
    if task.status == "unknown" {
        task.status = "running".into();
    }
    background::upsert(state, task)
}

fn stopped(state: &mut RuntimeTasks, output: &Value) -> bool {
    let Some(id) = text(output, &["task_id"]) else {
        return false;
    };
    let Some(mut task) = state.tasks.get(&format!("task:{id}")).cloned() else {
        return false;
    };
    if is_terminal(&task.status) {
        return false;
    }
    task.status = "stopped".into();
    task.actions.clear();
    background::upsert(state, task)
}
