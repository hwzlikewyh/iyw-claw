use crate::acp::builtin_mcp::BuiltinMcpClient;
use crate::user_memory::{CompanionHealthReason, CompanionHealthSnapshot, CompanionHealthStatus};

/// Return the current process HTTP MCP capability snapshot without probing a
/// separate executable. The legacy snapshot type remains the settings contract,
/// but `selected_path` is always absent because there is no companion binary.
pub fn builtin_mcp_health(client: Option<&BuiltinMcpClient>) -> CompanionHealthSnapshot {
    let mut health = CompanionHealthSnapshot {
        expected_version: env!("CARGO_PKG_VERSION").to_string(),
        selected_path: None,
        ..CompanionHealthSnapshot::default()
    };
    let Some(client) = client else {
        health.status = CompanionHealthStatus::ProbeFailed;
        health.reason = CompanionHealthReason::JoinFailed;
        health.detail = Some("process HTTP MCP service is not initialized".to_string());
        return health;
    };

    health.detected_version = Some(env!("CARGO_PKG_VERSION").to_string());
    health.advertised_tools = client.advertised_tools().to_vec();
    if client.is_ready() {
        health.status = CompanionHealthStatus::Ready;
        health.reason = CompanionHealthReason::Ready;
    } else {
        health.status = CompanionHealthStatus::ProbeFailed;
        health.reason = CompanionHealthReason::ExitFailed;
        health.detail = Some("process HTTP MCP service is not accepting requests".to_string());
    }
    health
}
