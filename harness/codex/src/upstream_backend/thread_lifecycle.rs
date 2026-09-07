use serde_json::{json, Value};

use super::{
    ensure_method, reject_unmanaged_thread_overrides, thread_id_from_response, UpstreamClient,
    UpstreamError,
};
use crate::upstream_mcp::ThreadLaunchOptions;
use crate::{SessionAccess, SessionBinding, SessionOwner};

impl UpstreamClient {
    pub(crate) async fn fork_configured_thread(
        &self,
        request: Value,
        options: ThreadLaunchOptions,
    ) -> Result<Value, UpstreamError> {
        ensure_method(&request, "thread/fork")?;
        reject_unmanaged_thread_overrides(&request)?;
        let source_id = super::thread_id_from_params(&request)?;
        let binding = self
            .binding_for(&source_id)
            .await
            .ok_or_else(|| UpstreamError::InvalidRequest("fork source is not bound".into()))?;
        let access = session_access(&binding);
        self.harness.lock().await.ensure_can_begin_turn(access)?;
        let owner = SessionOwner::new(
            binding.connection_id.clone(),
            binding.conversation_id,
            binding.generation,
        )
        .map_err(|error| UpstreamError::InvalidRequest(error.to_string()))?;
        let capabilities = options.capabilities;
        let settings = options.fork_settings();
        self.harness
            .lock()
            .await
            .validate_session_capabilities(capabilities)?;
        let mut request = request;
        options.apply(&mut request);
        let response = self.send(request).await?;
        let forked = thread_id_from_response(&response)?;
        if forked == source_id {
            return Err(UpstreamError::InvalidResponse(
                "fork returned its source thread id".into(),
            ));
        }
        self.harness.lock().await.bind_session(
            SessionBinding::new(owner, forked.clone(), self.runtime_fingerprint.clone())
                .map_err(|error| UpstreamError::InvalidResponse(error.to_string()))?,
            capabilities,
        )?;
        if let Some(mut settings) = settings {
            settings["threadId"] = json!(forked);
            if let Err(error) = self
                .request_json_for_thread(
                    &forked,
                    json!({ "method": "thread/settings/update", "params": settings }),
                )
                .await
            {
                if let Some(binding) = self.binding_for(&forked).await {
                    let _ = self
                        .harness
                        .lock()
                        .await
                        .retire_session(session_access(&binding));
                }
                let _ = self
                    .send(
                        json!({ "method": "thread/unsubscribe", "params": { "threadId": forked } }),
                    )
                    .await;
                return Err(error);
            }
        }
        self.retire_fork_source(&binding).await?;
        Ok(response)
    }

    async fn retire_fork_source(&self, binding: &SessionBinding) -> Result<(), UpstreamError> {
        let source_id = &binding.external_id;
        self.retire_descendants(source_id).await?;
        self.harness
            .lock()
            .await
            .retire_session(session_access(binding))?;
        // 原线程保留在磁盘用于历史记录；解除本 worker 的订阅。
        if self
            .send(json!({ "method": "thread/unsubscribe", "params": { "threadId": source_id } }))
            .await
            .is_err()
        {
            // 分叉已持久化并绑定；不能伪装为分叉失败后让调用方再次创建分叉。
            eprintln!("[星河][worker] source thread unsubscribe failed after fork; source binding retired");
        }
        Ok(())
    }
}

pub(super) fn session_access(binding: &SessionBinding) -> SessionAccess<'_> {
    SessionAccess {
        external_id: &binding.external_id,
        connection_id: &binding.connection_id,
        generation: binding.generation,
        runtime_fingerprint: &binding.runtime_fingerprint,
    }
}
