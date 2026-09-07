use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;

use crate::acp::session_state::{LiveContentBlock, SessionState};
use crate::acp::types::AcpEvent;
use crate::web::event_bridge::{emit_with_state_gated, EventEmitter};

pub(super) async fn apply(state: &Arc<RwLock<SessionState>>, emitter: &EventEmitter, raw: &Value) {
    let Some(expected_generation) = raw["generation"].as_i64() else {
        return;
    };
    let Ok(content) =
        serde_json::from_value::<Vec<LiveContentBlock>>(raw["completedContent"].clone())
    else {
        tracing::warn!("[星河][worker] invalid completed content snapshot");
        return;
    };
    if content.iter().any(|block| {
        !matches!(
            block,
            LiveContentBlock::Text { .. }
                | LiveContentBlock::Thinking { .. }
                | LiveContentBlock::ToolCallRef { .. }
        )
    }) {
        tracing::warn!("[星河][worker] completed snapshot contains host-owned content");
        return;
    }
    loop {
        let (generation, seq, merged) = {
            let snapshot = state.read().await;
            if !snapshot.turn_in_flight || snapshot.turn_generation != expected_generation {
                return;
            }
            let previous = snapshot
                .live_message
                .as_ref()
                .map(|live| live.content.as_slice())
                .unwrap_or_default();
            (
                snapshot.turn_generation,
                snapshot.event_seq,
                merge(&content, previous),
            )
        };
        if emit_with_state_gated(
            state,
            emitter,
            AcpEvent::ContentRecovered { content: merged },
            |snapshot| {
                snapshot.turn_in_flight
                    && snapshot.turn_generation == generation
                    && snapshot.event_seq == seq
            },
        )
        .await
        {
            acknowledge(state, raw).await;
            return;
        }
        if state.read().await.turn_generation != generation {
            return;
        }
    }
}

pub(super) async fn acknowledge(state: &Arc<RwLock<SessionState>>, raw: &Value) {
    let Some(generation) = raw["generation"].as_i64() else {
        return;
    };
    let mut snapshot = state.write().await;
    if snapshot.turn_generation == generation {
        snapshot.worker_content_recovered =
            raw["turnId"].as_str().map(|turn| (generation, turn.into()));
        snapshot.native_background_notify.notify_waiters();
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sacp::JsonRpcRequest)]
#[request(method = "_iyw/worker/content_barrier", response = Value)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentBarrierRequest {
    session_id: sacp::schema::SessionId,
    turn_id: String,
    generation: i64,
}

impl crate::acp::runtime_host_router::SessionRequestRouter {
    pub(super) fn content_barrier(
        &self,
        request: ContentBarrierRequest,
        reply: sacp::Responder<Value>,
        connection: sacp::ConnectionTo<sacp::Agent>,
    ) -> Result<(), sacp::Error> {
        let route = self.resolve(&request.session_id);
        connection.spawn(async move {
            let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                let route = route.ok_or("worker content route expired")?;
                if route.worker_database.is_none() {
                    return Err("route is not a worker");
                }
                loop {
                    let snapshot = route.state.read().await;
                    if snapshot.worker_content_recovered.as_ref().is_some_and(
                        |(generation, turn)| {
                            *generation == request.generation && *turn == request.turn_id
                        },
                    ) {
                        return Ok(());
                    }
                    if !snapshot.turn_in_flight || snapshot.turn_generation != request.generation {
                        return Err("worker content turn expired");
                    }
                    let wake = snapshot.native_background_notify.clone().notified_owned();
                    drop(snapshot);
                    wake.await;
                }
            })
            .await;
            match result {
                Ok(Ok(())) => reply.respond(serde_json::json!({ "applied": true })),
                Ok(Err(error)) => reply.respond_with_error(sacp::util::internal_error(error)),
                Err(_) => reply.respond_with_error(sacp::util::internal_error(
                    "worker content application timed out",
                )),
            }
        })
    }
}

/// 宿主合成卡片和插话按上一条已知工具锚点保留，上游内容使用权威顺序。
fn merge(
    authoritative: &[LiveContentBlock],
    previous: &[LiveContentBlock],
) -> Vec<LiveContentBlock> {
    let tool_ids: HashSet<&str> = authoritative.iter().filter_map(tool_id).collect();
    let mut extras: HashMap<Option<String>, Vec<LiveContentBlock>> = HashMap::new();
    let mut anchor = None;
    for block in previous {
        if let Some(id) = tool_id(block).filter(|id| tool_ids.contains(id)) {
            anchor = Some(id.to_string());
        } else if matches!(
            block,
            LiveContentBlock::ToolCallRef { .. }
                | LiveContentBlock::UserInput { .. }
                | LiveContentBlock::Plan { .. }
        ) {
            extras
                .entry(anchor.clone())
                .or_default()
                .push(block.clone());
        }
    }
    let mut result = extras.remove(&None).unwrap_or_default();
    for block in authoritative {
        result.push(block.clone());
        if let Some(id) = tool_id(block) {
            result.extend(extras.remove(&Some(id.into())).unwrap_or_default());
        }
    }
    result
}

fn tool_id(block: &LiveContentBlock) -> Option<&str> {
    match block {
        LiveContentBlock::ToolCallRef { tool_call_id } => Some(tool_call_id),
        _ => None,
    }
}
