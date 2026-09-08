use serde_json::{json, Value};

use crate::{Capability, CapabilitySet, UpstreamClient, UpstreamError};

use super::prompt_mapping;

const INVALID_REQUEST_CODE: i64 = -32600;

pub(super) async fn handle(
    upstream: &UpstreamClient,
    params: &Value,
    session: (Option<&str>, CapabilitySet),
) -> Result<Value, UpstreamError> {
    let (bound_session, capabilities) = session;
    if !capabilities.contains(Capability::Steering) {
        return Ok(json!({ "outcome": "unsupported" }));
    }
    let id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| UpstreamError::InvalidRequest("steering has no sessionId".into()))?;
    if bound_session != Some(id) {
        return Err(UpstreamError::InvalidRequest(
            "steering session does not match the bound session".into(),
        ));
    }
    let mut request = prompt_mapping::turn_start_request(params, capabilities)?;
    let Some(turn) = upstream
        .active_turn_for(id)
        .await
        .filter(|turn| !turn.cancelling)
    else {
        // 没有活动回合时由应用 outbox 派发下一轮，避免创建脱离队列的回合。
        return Ok(json!({ "outcome": "promptRequired" }));
    };
    request["method"] = json!("turn/steer");
    request["params"]["expectedTurnId"] = json!(turn.turn_id);
    match upstream.steer_turn_for_thread(id, request).await {
        Ok(response)
            if response.get("turnId").and_then(Value::as_str) == Some(turn.turn_id.as_str()) =>
        {
            Ok(json!({ "outcome": "injected" }))
        }
        Ok(_) => Err(UpstreamError::InvalidResponse(
            "steering response does not match the active turn".into(),
        )),
        Err(error) if was_not_submitted(&error) => Ok(json!({ "outcome": "promptRequired" })),
        Err(error) => Err(error),
    }
}

fn was_not_submitted(error: &UpstreamError) -> bool {
    // 仅识别锁定上游明确未接受输入的 RPC 回复；传输失败不能自动重放。
    matches!(error, UpstreamError::Rpc { code: INVALID_REQUEST_CODE, message }
        if matches!(message.as_str(), "no active turn to steer"
            | "cannot steer a review turn" | "cannot steer a compact turn"))
}
