use rmcp::ErrorData;

use super::authority::SessionContext;
use crate::acp::capability_policy::{require_runtime_agent, Capability};

pub(super) async fn require_call(authority: &SessionContext) -> Result<(), ErrorData> {
    require_runtime_agent(authority.agent_type(), Capability::Mcp, true)
        .await
        .map_err(|error| {
            tracing::warn!(
                connection_id = authority.connection_id(),
                agent = %authority.agent_type(),
                denial_code = error.detail.as_deref().unwrap_or("remote_policy_denied"),
                "[capability-policy] New built-in MCP call denied before dispatch"
            );
            ErrorData::invalid_request(
                "MCP capability is disabled",
                serde_json::to_value(error).ok(),
            )
        })
}
