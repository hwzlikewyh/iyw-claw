use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::UpstreamClient;

type Reply = oneshot::Sender<Result<Value, String>>;

#[derive(Default)]
pub(super) struct InputBoundary {
    pending: Vec<(String, Reply)>,
}

impl InputBoundary {
    pub(super) async fn register(
        &mut self,
        upstream: &UpstreamClient,
        bound: Option<&str>,
        params: &Value,
        reply: Reply,
    ) {
        let Some(thread) = params["sessionId"].as_str().filter(|id| Some(*id) == bound) else {
            let _ = reply.send(Err("input session does not match the active binding".into()));
            return;
        };
        let Some(interrupt) = params["interrupt"].as_bool() else {
            let _ = reply.send(Err("invalid input boundary request".into()));
            return;
        };
        let Some(turn) = upstream.active_turn_for(thread).await else {
            let _ = reply.send(Ok(json!({"ready":true})));
            return;
        };
        if interrupt && !turn.cancelling {
            if let Err(error) = upstream.interrupt_bound_turn(thread, &turn.turn_id).await {
                let _ = reply.send(Err(error.to_string()));
                return;
            }
        }
        self.pending.push((thread.to_owned(), reply));
    }

    pub(super) async fn release_finished(
        &mut self,
        upstream: &UpstreamClient,
        bound: Option<&str>,
    ) {
        if self.pending.is_empty() {
            return;
        }
        let active = match bound {
            Some(thread) => upstream.active_turn_for(thread).await,
            None => None,
        };
        let mut waiting = Vec::new();
        for (thread, reply) in self.pending.drain(..) {
            if Some(thread.as_str()) != bound {
                let _ = reply.send(Err("input session changed".into()));
            } else if active.is_some() {
                if !reply.is_closed() {
                    waiting.push((thread, reply));
                }
            } else {
                let _ = reply.send(Ok(json!({"ready":true})));
            }
        }
        self.pending = waiting;
    }
}
