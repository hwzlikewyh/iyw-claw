use serde_json::json;

use super::{UpstreamClient, UpstreamError};
use crate::TurnBinding;

impl UpstreamClient {
    /// 子代理可先启动回合再出现在父工具结果中，发现时必须回读当前回合。
    pub(crate) async fn synchronize_child_turn(&self, thread: &str) -> Result<(), UpstreamError> {
        let response = self
            .history_page(
                thread,
                json!({ "method": "thread/turns/list", "params": {
            "threadId": thread, "limit": 1, "sortDirection": "desc", "itemsView": "notLoaded",
        } }),
            )
            .await?;
        let newest = response
            .get("data")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| invalid("subagent turn snapshot has no data"))?
            .first();
        let binding = self
            .binding_for(thread)
            .await
            .ok_or_else(|| invalid("subagent binding expired"))?;
        let access = super::thread_lifecycle::session_access(&binding);
        let mut harness = self.harness.lock().await;
        if let Some(active) = harness.active_turn_for(thread) {
            let running = newest
                .is_some_and(|turn| turn["id"] == active.turn_id && turn["status"] == "inProgress");
            if running {
                return Ok(());
            }
            harness.complete_turn(access, &active.turn_id)?;
        }
        if let Some(turn) = newest.filter(|turn| turn["status"] == "inProgress") {
            let id = turn["id"]
                .as_str()
                .ok_or_else(|| invalid("subagent turn has no id"))?;
            if !harness.turn_is_retired(thread, id) {
                harness.begin_turn(access, TurnBinding::new(thread, id)?)?;
            }
        }
        Ok(())
    }
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidResponse(message.into())
}
