use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::{UpstreamClient, UpstreamError, UpstreamEvent};

const PAGE_SIZE: u32 = 100;

pub(super) async fn recover(
    upstream: &UpstreamClient,
    thread: &str,
) -> Result<Vec<UpstreamEvent>, UpstreamError> {
    let anchor = upstream.recovery_anchor(thread).await;
    let turns = load_turns(upstream, thread, anchor.as_deref()).await?;
    let mut events = Vec::new();
    for turn in turns {
        events.extend(recover_turn(upstream, thread, turn).await?);
    }
    Ok(events)
}

async fn recover_turn(
    upstream: &UpstreamClient,
    thread: &str,
    turn: Value,
) -> Result<Vec<UpstreamEvent>, UpstreamError> {
    let id = turn["id"]
        .as_str()
        .ok_or_else(|| invalid("recovered turn has no id"))?;
    let mut events = vec![event(
        "turn/started",
        json!({ "threadId": thread, "turn": turn }),
    )];
    let items = load_items(upstream, thread, id).await?;
    for item in items {
        let params =
            json!({ "threadId": thread, "turnId": id, "item": item, "_iywRecovered": true });
        events.push(event("item/started", params.clone()));
        if params.pointer("/item/status").and_then(Value::as_str) != Some("inProgress") {
            events.push(event("item/completed", params));
        }
    }
    if turn["status"] != "inProgress" {
        events.push(event(
            "turn/completed",
            json!({ "threadId": thread, "turn": turn }),
        ));
    }
    Ok(events)
}

async fn load_turns(
    upstream: &UpstreamClient,
    thread: &str,
    expected: Option<&str>,
) -> Result<Vec<Value>, UpstreamError> {
    let mut cursor = Value::Null;
    let mut visited = BTreeSet::new();
    let mut recovered = Vec::new();
    loop {
        let page = upstream.history_page(thread, json!({ "method": "thread/turns/list", "params": {
            "threadId": thread, "limit": PAGE_SIZE, "cursor": cursor, "sortDirection": "desc", "itemsView": "notLoaded",
        } })).await?;
        let turns = page["data"]
            .as_array()
            .ok_or_else(|| invalid("turn recovery has no data"))?;
        for turn in turns {
            recovered.push(turn.clone());
            if expected.is_none_or(|id| turn["id"] == id) {
                recovered.reverse();
                return Ok(recovered);
            }
        }
        let Some(next) = page["nextCursor"].as_str() else {
            return if expected.is_some() {
                Err(invalid("active turn is missing from recovery snapshot"))
            } else {
                Ok(recovered)
            };
        };
        if !visited.insert(next.to_string()) {
            return Err(invalid("turn recovery repeated a cursor"));
        }
        cursor = json!(next);
    }
}

pub(super) async fn load_items(
    upstream: &UpstreamClient,
    thread: &str,
    turn: &str,
) -> Result<Vec<Value>, UpstreamError> {
    let mut cursor = Value::Null;
    let mut visited = BTreeSet::new();
    let mut items = Vec::new();
    loop {
        let page = upstream.history_page(thread, json!({ "method": "thread/items/list", "params": {
            "threadId": thread, "turnId": turn, "cursor": cursor, "limit": PAGE_SIZE, "sortDirection": "asc",
        } })).await?;
        let entries = page["data"]
            .as_array()
            .ok_or_else(|| invalid("item recovery has no data"))?;
        for entry in entries {
            let item = entry
                .get("item")
                .filter(|_| entry["turnId"] == turn)
                .ok_or_else(|| invalid("recovery item belongs to another turn"))?;
            items.push(item.clone());
        }
        let Some(next) = page["nextCursor"].as_str() else {
            return Ok(items);
        };
        if !visited.insert(next.to_string()) {
            return Err(invalid("item recovery repeated a cursor"));
        }
        cursor = json!(next);
    }
}

fn event(method: &str, params: Value) -> UpstreamEvent {
    UpstreamEvent::ServerNotification {
        method: method.to_string(),
        params,
    }
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidResponse(message.into())
}
