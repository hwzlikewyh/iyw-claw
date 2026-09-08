use sacp::{Client, ConnectionTo};
use serde_json::{json, Value};

use super::{event_recovery, item_mapping, send_update, to_sacp_error};
use crate::UpstreamClient;

const SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub(super) async fn reconcile(
    upstream: &UpstreamClient,
    cx: &ConnectionTo<Client>,
    context: (&Value, &mut item_mapping::ItemProjection, i64),
) -> Result<bool, sacp::Error> {
    let (params, projection, generation) = context;
    let thread = params["threadId"]
        .as_str()
        .ok_or_else(|| to_sacp_error("completed snapshot has no thread"))?;
    let turn = params
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .ok_or_else(|| to_sacp_error("completed snapshot has no turn"))?;
    let Some(items) = load_items(upstream, cx, (thread, turn, params, generation)).await? else {
        return Ok(false);
    };
    let session = Some(thread.to_string());
    send_tools(cx, projection, (thread, turn, &items))?;
    let content = items.iter().flat_map(content_blocks).collect::<Vec<_>>();
    send_update(
        cx,
        &session,
        "session_info_update",
        json!({ "_meta": { "iyw": {
        "completedContent": content, "turnId": turn, "generation": generation,
    } } }),
    )?;
    barrier(cx, (thread, turn, generation)).await?;
    Ok(true)
}

async fn load_items(
    upstream: &UpstreamClient,
    cx: &ConnectionTo<Client>,
    identity: (&str, &str, &Value, i64),
) -> Result<Option<Vec<Value>>, sacp::Error> {
    let (thread, turn, params, generation) = identity;
    match tokio::time::timeout(
        SNAPSHOT_TIMEOUT,
        event_recovery::load_items(upstream, thread, turn),
    )
    .await
    {
        Ok(Ok(items)) if includes_final_message(&items, params) => Ok(Some(items)),
        result => {
            let reason = match &result {
                Ok(Err(_)) => "history_read_failed",
                Ok(Ok(_)) => "final_message_mismatch",
                Err(_) => "history_read_timeout",
            };
            eprintln!(
                "[星河][worker] completed content recovery failed: {}",
                match result {
                    Ok(Err(error)) => error.to_string(),
                    Ok(Ok(_)) => "snapshot does not contain the final message".into(),
                    _ => "snapshot timed out".into(),
                }
            );
            send_update(
                cx,
                &Some(thread.into()),
                "session_info_update",
                json!({ "_meta": { "iyw": {
                "recoveryError": "本轮已结束，但未能核对完整输出，请从历史记录检查结果。",
                "recoveryReason": reason,
                "turnId": turn, "generation": generation,
            } } }),
            )?;
            barrier(cx, (thread, turn, generation)).await?;
            Ok(None)
        }
    }
}

async fn barrier(
    cx: &ConnectionTo<Client>,
    identity: (&str, &str, i64),
) -> Result<(), sacp::Error> {
    let (thread, turn, generation) = identity;
    let request = sacp::UntypedMessage::new(
        "_iyw/worker/content_barrier",
        json!({
            "sessionId": thread, "turnId": turn, "generation": generation,
        }),
    )?;
    let response = cx.send_request_to(Client, request).block_task().await?;
    if response["applied"] != true {
        return Err(to_sacp_error("host did not apply the completed snapshot"));
    }
    Ok(())
}

fn send_tools(
    cx: &ConnectionTo<Client>,
    projection: &mut item_mapping::ItemProjection,
    identity: (&str, &str, &[Value]),
) -> Result<(), sacp::Error> {
    let (thread, turn, items) = identity;
    for item in items {
        if !item_mapping::is_tool_item(item) {
            continue;
        }
        let snapshot =
            json!({ "threadId": thread, "turnId": turn, "item": item, "_iywRecovered": true });
        let running = item["status"] == "inProgress";
        if running || !projection.was_started(item["id"].as_str().unwrap_or_default()) {
            if let Some(update) = projection.map("item/started", &snapshot) {
                send_update(cx, &Some(thread.into()), update.method, update.params)?;
            }
        }
        if !running {
            if let Some(update) = projection.map("item/completed", &snapshot) {
                send_update(cx, &Some(thread.into()), update.method, update.params)?;
            }
        }
    }
    Ok(())
}

fn includes_final_message(items: &[Value], params: &Value) -> bool {
    params
        .pointer("/turn/items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "agentMessage")
        .all(|last| {
            // 同一回合的旧格式历史会重建 item-N ID，不能与实时消息 UUID 比较。
            // 核对最后一条回答的完整内容及阶段，避免把完整输出误判为丢失。
            items
                .iter()
                .rev()
                .find(|item| item["type"] == "agentMessage")
                .is_some_and(|item| item["text"] == last["text"] && item["phase"] == last["phase"])
        })
}

fn content_blocks(item: &Value) -> Vec<Value> {
    match item["type"].as_str() {
        Some("agentMessage" | "plan") => item["text"]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(|text| vec![json!({ "kind": "text", "text": text })])
            .unwrap_or_default(),
        Some("reasoning") => ["summary", "content"]
            .into_iter()
            .flat_map(|kind| {
                item[kind]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(|text| json!({ "kind": "thinking", "text": text }))
            })
            .collect(),
        _ if item_mapping::is_tool_item(item) => {
            vec![json!({ "kind": "tool_call_ref", "tool_call_id": item["id"] })]
        }
        _ => Vec::new(),
    }
}
