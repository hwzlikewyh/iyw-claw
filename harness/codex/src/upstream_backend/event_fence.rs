use serde_json::Value;

use super::{UpstreamClient, UpstreamError};
use crate::TurnBinding;

impl UpstreamClient {
    pub(crate) async fn accepts_turn_event(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<bool, UpstreamError> {
        let Some(thread) = params
            .get("threadId")
            .or_else(|| params.get("thread_id"))
            .and_then(Value::as_str)
        else {
            return Ok(!method.starts_with("item/") && !method.starts_with("turn/"));
        };
        let turn = params
            .get("turnId")
            .or_else(|| params.get("turn_id"))
            .or_else(|| params.pointer("/turn/id"))
            .and_then(Value::as_str);
        let Some(turn) = turn else {
            return Ok(true);
        };
        let Some(binding) = self.binding_for(thread).await else {
            return Ok(false);
        };
        let mut harness = self.harness.lock().await;
        if harness.turn_is_retired(thread, turn) {
            return Ok(false);
        }
        let active = harness.active_turn_for(thread);
        if matches!(method, "turn/started" | "turn/completed") && active.is_none() {
            harness.begin_turn(
                super::thread_lifecycle::session_access(&binding),
                TurnBinding::new(thread, turn)?,
            )?;
            return Ok(true);
        }
        Ok(active.is_some_and(|active| {
            active.turn_id == turn && (!active.cancelling || method == "turn/completed")
        }))
    }
}
