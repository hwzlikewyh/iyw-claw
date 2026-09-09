use std::time::Duration;

use rmcp::transport::streamable_http_server::SessionManager;

use super::LeaseManager;

const RESET_TIMEOUT: Duration = Duration::from_secs(5);

impl LeaseManager {
    // 仅用于原 Host 已退出、同一连接继续恢复的情况；不撤销仍有效的 authority。
    pub(crate) async fn reset_transport(&self, connection_id: &str) -> Result<(), String> {
        let sessions = {
            let _lifecycle = self.lifecycle.lock().await;
            self.bindings.take_parent(connection_id).await
        };
        let count = sessions.len();
        let close = async {
            for session in sessions {
                self.protocol_sessions
                    .close_session(&session)
                    .await
                    .map_err(|error| format!("MCP protocol session cleanup failed: {error}"))?;
            }
            Ok::<(), String>(())
        };
        tokio::time::timeout(RESET_TIMEOUT, close)
            .await
            .map_err(|_| "MCP protocol session cleanup timed out".to_string())??;
        tracing::info!(target: "builtin_mcp", connection_id, closed_sessions = count,
            "reset HTTP MCP transport for runtime Host recovery");
        Ok(())
    }
}
