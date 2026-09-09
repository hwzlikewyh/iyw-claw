use serde_json::{json, Value};

use crate::{Capability, CapabilitySet, UpstreamClient, UpstreamError};

use super::prompt_mapping;

pub(super) async fn steer(
    upstream: &UpstreamClient,
    params: &Value,
    capabilities: CapabilitySet,
) -> Result<Value, UpstreamError> {
    if !capabilities.contains(Capability::Steering) {
        return Ok(json!({ "outcome": "unsupported" }));
    }
    let mut request = prompt_mapping::turn_start_request(params, capabilities)?;
    let thread_id = params["sessionId"]
        .as_str()
        .ok_or_else(|| UpstreamError::InvalidRequest("steering request has no sessionId".into()))?;
    let Some(turn) = upstream.active_turn_for(thread_id).await else {
        return Ok(json!({ "outcome": "promptRequired" }));
    };
    request["method"] = json!("turn/steer");
    request["params"]["expectedTurnId"] = json!(turn.turn_id);
    match upstream.steer_turn_for_thread(thread_id, request).await {
        Ok(response) if response.get("turnId").and_then(Value::as_str) == Some(&turn.turn_id) => {
            Ok(json!({ "outcome": "injected" }))
        }
        Ok(_) => Err(UpstreamError::InvalidResponse(
            "steering acknowledgement does not match the active turn".into(),
        )),
        Err(error) if input_was_not_submitted(&error) => Ok(json!({ "outcome": "promptRequired" })),
        Err(error) => Err(error),
    }
}

fn input_was_not_submitted(error: &UpstreamError) -> bool {
    // 锁定的上游仅对明确未提交的输入返回这些错误；传输异常不得自动重发。
    matches!(error, UpstreamError::Io(message) if matches!(
        message.as_str(),
        "no active turn to steer" | "cannot steer a review turn" | "cannot steer a compact turn"
    ))
}
