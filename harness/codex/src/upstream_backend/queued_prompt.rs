use serde_json::Value;

use super::{UpstreamClient, UpstreamError};
use crate::{Capability, TurnBinding};

impl UpstreamClient {
    /// 上游 turn/start 可原子地开始或 steer；只用于宿主已入队输入接管自动回合。
    pub(crate) async fn submit_queued_prompt(
        &self,
        thread: &str,
        request: Value,
    ) -> Result<Value, UpstreamError> {
        super::ensure_method(&request, "turn/start")?;
        super::reject_unmanaged_turn_overrides(&request)?;
        let binding = self.binding_for(thread).await.ok_or_else(|| {
            UpstreamError::InvalidRequest("queued prompt session is not bound".into())
        })?;
        let access = super::thread_lifecycle::session_access(&binding);
        self.harness
            .lock()
            .await
            .validate_capability(access, Capability::Steering)?;
        let response = self.send_scoped(access, request).await?;
        let turn = super::turn_id_from_response(&response)?;
        let mut harness = self.harness.lock().await;
        if let Some(active) = harness.active_turn_for(thread) {
            if active.turn_id == turn {
                return Ok(response);
            }
            // 返回新的 ID 证明旧自动回合已经结束；输入只在上面提交过一次。
            harness.complete_turn(access, &active.turn_id)?;
        }
        harness.begin_turn(access, TurnBinding::new(thread, turn)?)?;
        Ok(response)
    }
}
