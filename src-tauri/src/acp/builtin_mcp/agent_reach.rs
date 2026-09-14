use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::authority::SessionContext;
use crate::commands::internet_tools::internet_tools_agent_reach_doctor;

const MAX_DIAGNOSTIC_CHARS: usize = 2048;

pub(super) async fn status(
    authority: &SessionContext,
    cancel: &CancellationToken,
) -> Result<CallToolResult, ErrorData> {
    tracing::info!(
        connection_id = authority.connection_id(),
        "[internet-tools] MCP channel diagnosis started"
    );
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled()),
        _ = authority.cancellation().cancelled() => return Err(cancelled()),
        result = internet_tools_agent_reach_doctor() => result,
    };
    match result {
        Ok(mut channels) => {
            for channel in &mut channels {
                channel.message = diagnostic(&channel.message);
            }
            tracing::info!(
                count = channels.len(),
                "[internet-tools] MCP channel diagnosis completed"
            );
            Ok(CallToolResult::structured(json!({"channels": channels})))
        }
        Err(error) => {
            let message = diagnostic(&error);
            tracing::warn!(error = %message, "[internet-tools] MCP channel diagnosis failed");
            Ok(CallToolResult::error(vec![Content::text(message)]))
        }
    }
}

fn diagnostic(text: &str) -> String {
    crate::acp::stderr_tail::sanitize_diagnostic(text)
        .chars()
        .take(MAX_DIAGNOSTIC_CHARS)
        .collect()
}

fn cancelled() -> ErrorData {
    ErrorData::invalid_request("Agent Reach status request cancelled", None)
}
