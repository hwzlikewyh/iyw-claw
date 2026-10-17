//! 仅控制当前绑定线程的原生后台终端，不使用独立 command/exec 的控制域。
use crate::{UpstreamClient, UpstreamError};
use sacp::{Client, ConnectionTo};
use serde_json::{json, Value};
use std::collections::HashSet;

use super::send_update;

const SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const PAGE_SIZE: usize = 100;
pub(super) const SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

pub(super) async fn list(
    upstream: &UpstreamClient,
    root: &str,
) -> Result<Vec<Value>, UpstreamError> {
    let mut cursor = Value::Null;
    let mut seen = HashSet::new();
    let mut tasks = Vec::new();
    loop {
        let response = upstream
            .request_json_for_thread(
                root,
                json!({
                    "method": "thread/backgroundTerminals/list",
                    "params": {"threadId": root, "cursor": cursor, "limit": PAGE_SIZE},
                }),
            )
            .await?;
        let data = response["data"].as_array().ok_or_else(|| {
            UpstreamError::InvalidResponse("Background terminal snapshot has no data".into())
        })?;
        tasks.extend(data.iter().cloned());
        cursor = response["nextCursor"].clone();
        let Some(next) = cursor.as_str() else {
            break;
        };
        if !seen.insert(next.to_string()) {
            return Err(UpstreamError::InvalidResponse(
                "Background terminal cursor repeated".into(),
            ));
        }
    }
    Ok(tasks)
}

pub(super) async fn publish(
    upstream: &UpstreamClient,
    cx: &ConnectionTo<Client>,
    root: &str,
) -> Result<Vec<Value>, UpstreamError> {
    let result = tokio::time::timeout(SNAPSHOT_TIMEOUT, list(upstream, root)).await;
    let tasks = match result {
        Ok(Ok(tasks)) => tasks,
        result => {
            send_update(
                cx,
                &Some(root.into()),
                "session_info_update",
                json!({"_meta":{"iyw":{"backgroundTasksUncertain":true}}}),
            )
            .map_err(|error| UpstreamError::Io(error.to_string()))?;
            return Err(match result {
                Ok(Err(error)) => error,
                _ => UpstreamError::Io("Background terminal synchronization timed out".into()),
            });
        }
    };
    let tasks: Vec<_> = tasks
        .into_iter()
        .map(|mut task| {
            task["status"] = json!("running");
            task
        })
        .collect();
    send_update(
        cx,
        &Some(root.into()),
        "session_info_update",
        json!({"_meta":{"iyw":{"backgroundTasks":tasks}}}),
    )
    .map_err(|error| UpstreamError::Io(error.to_string()))?;
    Ok(tasks)
}

pub(super) async fn stop(
    upstream: &UpstreamClient,
    params: &Value,
    root: Option<&str>,
) -> Result<Value, UpstreamError> {
    let root = root
        .filter(|root| params["sessionId"].as_str() == Some(*root))
        .ok_or_else(|| {
            UpstreamError::InvalidRequest("Background control session mismatch".into())
        })?;
    let tasks = tokio::time::timeout(SNAPSHOT_TIMEOUT, list(upstream, root))
        .await
        .map_err(|_| {
            UpstreamError::Io("Background task lookup timed out; no stop was sent".into())
        })??;
    let Some(task) = tasks.iter().find(|task| {
        task["itemId"] == params["itemId"]
            && task["processId"] == params["processId"]
            && task["processId"].is_string()
    }) else {
        return Err(UpstreamError::InvalidRequest(
            "Background terminal is no longer owned or running".into(),
        ));
    };
    let result = upstream
        .request_json_for_thread(
            root,
            json!({
                "method": "thread/backgroundTerminals/terminate",
                "params": {"threadId":root,"processId":task["processId"]},
            }),
        )
        .await?;
    Ok(json!({"stopped":result["terminated"] == true}))
}
