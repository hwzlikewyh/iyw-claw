use sacp::{Client, ConnectionTo, UntypedMessage};
use serde_json::{json, Value};

use crate::UpstreamClient;

#[derive(Default)]
pub(super) struct AutomaticTurnState {
    generation: i64,
    adopted_turn: Option<String>,
    pub awaiting_prompt: Option<String>,
    pub deferred_prompt: Option<super::BridgeCommand>,
}

impl AutomaticTurnState {
    pub(super) fn generation(&self) -> i64 {
        self.generation
    }

    pub(super) fn owns_turn(&self, turn: &str) -> bool {
        self.adopted_turn.as_deref() == Some(turn) || self.awaiting_prompt.as_deref() == Some(turn)
    }
    pub(super) fn observe_prompt(&mut self, params: &Value, session: Option<&str>) {
        if params["sessionId"].as_str() != session {
            return;
        }
        if let Some(generation) = params
            .pointer("/_meta/iyw/turnGeneration")
            .and_then(Value::as_i64)
        {
            if generation >= 0 {
                self.generation = generation;
            }
        }
    }

    pub(super) async fn begin(
        &mut self,
        cx: &ConnectionTo<Client>,
        upstream: &UpstreamClient,
        identity: (&str, &str),
    ) -> Result<(), sacp::Error> {
        let (thread_id, turn_id) = identity;
        let request = UntypedMessage::new(
            "_iyw/worker/turn_started",
            json!({ "sessionId": thread_id, "turnId": turn_id, "afterGeneration": self.generation }),
        )?;
        match cx.send_request_to(Client, request).block_task().await {
            Ok(response) if response.get("accepted").and_then(Value::as_bool) == Some(true) => {
                self.adopted_turn = Some(turn_id.into());
                self.generation = response["generation"].as_i64().ok_or_else(|| {
                    sacp::util::internal_error("automatic turn has no host generation")
                })?;
                Ok(())
            }
            Ok(response) if response["queuedPrompt"] == true => {
                self.awaiting_prompt = Some(turn_id.into());
                Ok(())
            }
            result => {
                let _ = upstream.interrupt_turn_for_thread(thread_id).await;
                let message = match result {
                    Err(error) => error.to_string(),
                    _ => "host did not adopt the automatic turn".into(),
                };
                Err(sacp::util::internal_error(message))
            }
        }
    }
}

pub(super) fn completed(params: &Value) -> Value {
    let status = match params.pointer("/turn/status").and_then(Value::as_str) {
        Some("completed") => "idle",
        Some("interrupted" | "aborted") => "interrupted",
        _ => "systemError",
    };
    json!({ "_meta": { "codex": { "threadStatus": { "type": status } } } })
}
