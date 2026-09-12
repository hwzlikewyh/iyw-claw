use serde_json::{json, Value};

use crate::UpstreamClient;

use super::{acp_mapping, goal_commands, to_sacp_error, PendingPrompt};

pub(super) async fn started(
    upstream: &UpstreamClient,
    params: &Value,
    pending: &mut Option<PendingPrompt>,
) -> Result<(), sacp::Error> {
    let Some(pending) = pending.as_mut().filter(|pending| pending.goal_run) else {
        return Ok(());
    };
    let turn_id = params
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .ok_or_else(|| to_sacp_error("turn/started has no turn id"))?;
    if pending.last_completed_turn.as_deref() == Some(turn_id) {
        return Ok(());
    }
    upstream
        .observe_goal_turn(&pending.thread_id, turn_id)
        .await
        .map_err(to_sacp_error)?;
    pending.turn_id = turn_id.to_string();
    Ok(())
}

pub(super) async fn keep_pending(
    upstream: &UpstreamClient,
    params: &Value,
    pending: &mut PendingPrompt,
) -> Result<bool, sacp::Error> {
    if !pending.goal_run {
        return Ok(false);
    }
    if let Some(failure) = acp_mapping::prompt_failure(params) {
        goal_commands::pause(upstream, &pending.thread_id)
            .await
            .map_err(|error| {
                to_sacp_error(format!(
                    "{failure}; could not pause goal continuation: {error}"
                ))
            })?;
        return Ok(false);
    }
    if !goal_commands::is_active(upstream, &pending.thread_id)
        .await
        .map_err(to_sacp_error)?
    {
        return Ok(false);
    }
    pending.last_completed_turn = Some(std::mem::take(&mut pending.turn_id));
    Ok(true)
}

pub(super) fn observe_status(method: &str, params: &Value, pending: &mut Option<PendingPrompt>) {
    if !matches!(method, "thread/goal/updated" | "thread/goal/cleared") {
        return;
    }
    let active = params.pointer("/goal/status").and_then(Value::as_str) == Some("active");
    if active {
        if let Some(pending) = pending.as_mut() {
            pending.goal_run = true;
        }
        return;
    }
    if pending
        .as_ref()
        .is_some_and(|pending| pending.goal_run && pending.turn_id.is_empty())
    {
        if let Some(pending) = pending.take() {
            let _ = pending.responder.respond(json!({"stopReason": "end_turn"}));
        }
    }
}
