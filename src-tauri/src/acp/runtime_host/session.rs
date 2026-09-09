use std::time::Duration;

use sacp::{Agent, UntypedMessage};

use super::AgentRuntimeHost;
use crate::acp::error::AcpError;

pub(super) const SESSION_HANDOFF_TIMEOUT: Duration = Duration::from_secs(30);
const SESSION_CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

impl AgentRuntimeHost {
    pub(crate) fn supports_session_close(&self) -> bool {
        self.supports_session_close
    }

    pub(crate) async fn close_session(&self, session_id: &str) -> Result<(), AcpError> {
        self.begin_session_close(session_id)?;
        let request = UntypedMessage::new(
            "session/close",
            serde_json::json!({
                "sessionId": session_id,
            }),
        )
        .map_err(|error| AcpError::protocol(error.to_string()))?;
        let result = tokio::time::timeout(
            SESSION_CLOSE_TIMEOUT,
            self.connection.send_request_to(Agent, request).block_task(),
        )
        .await;
        match result {
            Ok(Ok(response)) if response.is_object() => {
                self.closing_sessions
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(session_id);
                tracing::info!(
                    agent = crate::acp::registry::get_agent_meta(self.key.agent_type).name,
                    session_id,
                    "[ACP][host] remote session closed"
                );
                Ok(())
            }
            Ok(Ok(_)) => Err(AcpError::protocol("Invalid session/close response")),
            Ok(Err(error)) => {
                let code = i32::from(error.code);
                tracing::warn!(
                    agent = crate::acp::registry::get_agent_meta(self.key.agent_type).name,
                    session_id,
                    code,
                    "[ACP][host] remote session close rejected"
                );
                Err(AcpError::protocol(format!(
                    "session/close failed with RPC code {code}"
                )))
            }
            Err(_) => Err(AcpError::protocol("session/close timed out")),
        }
    }

    fn begin_session_close(&self, session_id: &str) -> Result<(), AcpError> {
        if !self.supports_session_close {
            return Err(AcpError::protocol("Agent does not support session/close"));
        }
        // RPC 超时或 future 被取消后，禁止再次接管该 Host 上结果未明的会话。
        if !self
            .closing_sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(session_id.to_string())
        {
            return Err(AcpError::protocol(
                "Previous session/close has no confirmed completion",
            ));
        }
        Ok(())
    }
}
