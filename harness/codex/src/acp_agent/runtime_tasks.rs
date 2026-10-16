use super::{acp_mapping::Update, send_update};
use crate::{UpstreamClient, UpstreamError};
use sacp::{Client, ConnectionTo};
use serde_json::{json, Value};

const GOAL_SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) async fn stop_child(
    upstream: &UpstreamClient,
    params: &Value,
    root: Option<&str>,
) -> Result<Value, UpstreamError> {
    let root = root
        .filter(|root| params["sessionId"].as_str() == Some(*root))
        .ok_or_else(|| {
            UpstreamError::InvalidRequest("Task control does not match the bound session".into())
        })?;
    let child = params["threadId"]
        .as_str()
        .filter(|child| *child != root)
        .ok_or_else(|| {
            UpstreamError::InvalidRequest("Task control requires a child thread".into())
        })?;
    if !upstream.descendant_of(child, root).await {
        return Err(UpstreamError::InvalidRequest(
            "Task is not owned by this session".into(),
        ));
    }
    upstream.interrupt_turn_for_thread(child).await?;
    Ok(json!({"accepted": true}))
}

pub(super) async fn child_update(
    upstream: &UpstreamClient,
    method: &str,
    params: &Value,
) -> Option<Update> {
    let child = params
        .get("threadId")
        .or_else(|| params.get("thread_id"))?
        .as_str()?;
    if method == "item/started" {
        let mut notification = spawned(params.get("item")?, child)?;
        notification.params["_meta"]["iyw"]["runtimeTask"]["rootParent"] = json!(false);
        notification.params["_meta"]["iyw"]["runtimeTask"]["parentTurnId"] =
            params.get("turnId").cloned().unwrap_or(Value::Null);
        return Some(notification);
    }
    let mut task = json!({"id": child, "parentId": upstream.parent_thread_for(child).await, "turnId": params.pointer("/turn/id")});
    match method {
        "turn/started" => task["status"] = json!("running"),
        "turn/completed" => {
            task["status"] = json!(
                match params.pointer("/turn/status").and_then(Value::as_str) {
                    Some("completed") => "completed",
                    Some("interrupted" | "aborted") => "stopped",
                    _ => "failed",
                }
            );
            task["summary"] = params
                .pointer("/turn/error/message")
                .cloned()
                .unwrap_or(Value::Null);
        }
        "item/agentMessage/delta" => task["delta"] = params.get("delta")?.clone(),
        _ => return None,
    }
    Some(update(task))
}

pub(super) fn spawned(item: &Value, parent: &str) -> Option<Update> {
    if item["type"] != "subAgentActivity" || item["kind"] != "started" {
        return None;
    }
    Some(update(json!({
        "id": item.get("agentThreadId")?, "title": item.get("agentPath"),
        "model": item.get("model"), "status": "starting", "parentId": parent, "rootParent": true,
    })))
}

pub(super) async fn publish_goal(
    upstream: &UpstreamClient,
    cx: &ConnectionTo<Client>,
    id: &str,
) -> Result<bool, sacp::Error> {
    let snapshot = tokio::time::timeout(
        GOAL_SNAPSHOT_TIMEOUT,
        upstream.request_json_for_thread(
            id,
            json!({"method":"thread/goal/get", "params":{"threadId":id}}),
        ),
    )
    .await;
    if let Ok(Ok(response)) = snapshot {
        send_update(
            cx,
            &Some(id.to_string()),
            "session_info_update",
            json!({"_meta":{"codex":{"goal":response.get("goal")}}}),
        )?;
        return Ok(true);
    }
    eprintln!("[星河][worker] goal snapshot unavailable; instance identity remains unverified");
    Ok(false)
}

fn update(task: Value) -> Update {
    Update {
        method: "session_info_update",
        params: json!({"_meta":{"iyw":{"runtimeTask":task}}}),
    }
}

pub(super) async fn interaction_state(
    upstream: &UpstreamClient,
    cx: &ConnectionTo<Client>,
    context: (&Option<String>, &str, bool),
) -> Result<(), sacp::Error> {
    let (session, child, blocked) = context;
    let Some(root) = session.as_deref() else {
        return Ok(());
    };
    if !upstream.descendant_of(child, root).await {
        return Ok(());
    }
    let status = if blocked {
        "blocked"
    } else if upstream.active_turn_for(child).await.is_some() {
        "running"
    } else {
        "unknown"
    };
    let task = update(json!({"id":child, "status":status}));
    send_update(cx, session, task.method, task.params)
}
