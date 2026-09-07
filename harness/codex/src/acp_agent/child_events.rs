use serde_json::Value;

use crate::{UpstreamClient, UpstreamError};

/// 当前宿主使用父会话工具卡片展示子代理，子线程只在这里更新生命周期。
pub(super) async fn handle(
    upstream: &UpstreamClient,
    method: &str,
    params: &Value,
) -> Result<(), UpstreamError> {
    let Some(thread) = params.get("threadId").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(turn) = params.pointer("/turn/id").and_then(Value::as_str) else {
        return Ok(());
    };
    match method {
        "turn/started" => upstream.observe_child_turn(thread, turn).await,
        "turn/completed" => {
            if upstream
                .active_turn_for(thread)
                .await
                .is_some_and(|active| active.turn_id == turn)
            {
                upstream.complete_turn_for_thread(thread, turn).await?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
