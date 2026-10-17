use super::{background, is_terminal, preview, text, BackgroundInfo, RuntimeTask, RuntimeTasks};
use crate::models::AgentType;
use serde_json::Value;

pub(super) fn observe(state: &mut RuntimeTasks, value: &Value) -> bool {
    let Some(item) = text(value, &["itemId"]) else {
        return false;
    };
    let id = format!("terminal:{item}");
    let Some(native) = text(value, &["processId"]) else {
        return false;
    };
    let mut task = terminal(state, id, native);
    task.title = text(value, &["description", "command"]).unwrap_or(task.title);
    if let Some(command) = text(value, &["command"]) {
        task.background
            .get_or_insert_with(BackgroundInfo::default)
            .command = Some(command);
    }
    if let Some(status) = text(value, &["status"]) {
        if !is_terminal(&task.status) {
            task.status = status;
        }
    }
    task.summary = text(value, &["summary"]).or(task.summary);
    if let Some(output) = text(value, &["output"]) {
        task.output = Some(preview(&output));
    }
    if let Some(delta) = text(value, &["delta"]) {
        task.output = Some(preview(&(task.output.unwrap_or_default() + &delta)));
    }
    task.actions = if task.status == "running" {
        vec!["stop".into()]
    } else {
        vec![]
    };
    background::upsert(state, task)
}

fn terminal(state: &RuntimeTasks, id: String, native: String) -> RuntimeTask {
    state
        .tasks
        .get(&id)
        .cloned()
        .unwrap_or_else(|| RuntimeTask {
            id,
            native_id: native,
            kind: "background".into(),
            status: "running".into(),
            agent_type: Some(AgentType::Codex),
            background: Some(BackgroundInfo {
                task_type: "terminal".into(),
                ..Default::default()
            }),
            ..Default::default()
        })
}

pub(super) fn reconcile(state: &mut RuntimeTasks, value: &Value) -> bool {
    let Some(items) = value.as_array() else {
        return false;
    };
    let mut changed = false;
    for item in items {
        changed |= observe(state, item);
    }
    let missing: Vec<_> = state
        .tasks
        .values()
        .filter(|task| {
            task.id.starts_with("terminal:")
                && !is_terminal(&task.status)
                && !items
                    .iter()
                    .any(|item| item["itemId"].as_str() == task.id.strip_prefix("terminal:"))
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

pub(super) fn unavailable(state: &mut RuntimeTasks) -> bool {
    let tasks: Vec<_> = state
        .tasks
        .values()
        .filter(|task| {
            task.id.starts_with("terminal:")
                && !is_terminal(&task.status)
                && state.live_background.contains(&task.id)
        })
        .cloned()
        .collect();
    let mut changed = false;
    for mut task in tasks {
        task.status = "unknown".into();
        task.actions.clear();
        changed |= background::upsert(state, task);
    }
    changed
}
