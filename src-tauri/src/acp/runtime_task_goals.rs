use super::{is_terminal, text, RuntimeTask, RuntimeTasks};
use serde_json::Value;

pub(super) fn sdk(state: &mut RuntimeTasks, message: &Value) -> bool {
    let Some(value) = message.get("value") else {
        return false;
    };
    if value.is_null() {
        return observe(state, value);
    }
    let Some(objective) = text(value, &["condition"]) else {
        return false;
    };
    observe(
        state,
        &serde_json::json!({"objective": objective, "status": "active", "lastReason":value.get("last_reason")}),
    )
}

pub(super) fn observe(state: &mut RuntimeTasks, value: &Value) -> bool {
    state.goal_observed = true;
    if value.is_null() {
        let Some(mut task) = state.tasks.get("goal").cloned() else {
            return false;
        };
        task.status = "cleared".into();
        task.actions.clear();
        return state.upsert(task);
    }
    let Some(objective) = text(value, &["objective"]) else {
        return false;
    };
    let status = text(value, &["status"]).unwrap_or_else(|| "unknown".into());
    let native_id =
        text(value, &["goalId", "goal_id", "id"]).unwrap_or_else(|| "unverified".into());
    archive_previous(state, &native_id);
    let actions = state
        .goal_actions
        .iter()
        .filter(|action| match action.as_str() {
            "pause" => status == "active",
            "resume" => matches!(status.as_str(), "paused" | "blocked"),
            "clear" => !is_terminal(&status),
            _ => false,
        })
        .cloned()
        .collect();
    state.upsert(RuntimeTask {
        id: "goal".into(),
        kind: "goal".into(),
        native_id,
        title: objective.clone(),
        objective: Some(objective),
        status,
        actions,
        summary: text(value, &["lastReason", "last_reason"]),
        ..Default::default()
    })
}

fn archive_previous(state: &mut RuntimeTasks, native_id: &str) {
    if let Some(mut previous) = state
        .tasks
        .get("goal")
        .filter(|previous| {
            native_id != "unverified"
                && previous.native_id != "unverified"
                && previous.native_id != native_id
        })
        .cloned()
    {
        previous.id = format!("goal-history:{}", previous.native_id);
        previous.actions.clear();
        if !is_terminal(&previous.status) {
            previous.status = "cleared".into();
        }
        state.upsert(previous);
    }
}
