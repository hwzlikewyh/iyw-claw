use std::sync::atomic::Ordering;

use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::authority::SessionContext;
use super::delivery::RelayDelivery;
use super::gateway::{self, MemoryGroupRequest};
use super::invocation::{execute_invocation, InvocationContext, InvocationDependencies};

pub(super) async fn invoke(
    dependencies: InvocationDependencies<'_>,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request: MemoryGroupRequest,
    request_id: Value,
    request_cancel: CancellationToken,
) -> Result<CallToolResult, ErrorData> {
    if let Some(result) = ensure_policy(
        dependencies,
        &authority,
        &request.operation,
        &request_id,
        request_cancel.clone(),
    )
    .await?
    {
        return Ok(result);
    }
    let result = execute_invocation(
        dependencies,
        InvocationContext {
            authority: authority.clone(),
            delivery,
            invocation: request.invocation,
            request_id,
            request_cancel,
        },
    )
    .await?;
    if request.operation == "policy.read" && result.is_error != Some(true) {
        record_policy_loaded(&authority);
    }
    Ok(result)
}

async fn ensure_policy(
    dependencies: InvocationDependencies<'_>,
    authority: &SessionContext,
    operation: &str,
    request_id: &Value,
    request_cancel: CancellationToken,
) -> Result<Option<CallToolResult>, ErrorData> {
    if operation == "policy.read" || policy_loaded(authority) {
        return Ok(None);
    }
    let result = execute_invocation(
        dependencies,
        InvocationContext {
            authority: authority.clone(),
            delivery: None,
            invocation: gateway::policy_invocation(authority)?,
            request_id: preflight_request_id(request_id),
            request_cancel,
        },
    )
    .await?;
    if result.is_error == Some(true) {
        return Ok(Some(result));
    }
    record_policy_loaded(authority);
    Ok(None)
}

fn policy_loaded(authority: &SessionContext) -> bool {
    authority.memory_policy_state().load(Ordering::Acquire)
        == authority.memory_turn_tracker().active_nonce().unwrap_or(0)
}

fn preflight_request_id(request_id: &Value) -> Value {
    json!({"request": request_id, "phase": "memory_policy_preflight"})
}

fn record_policy_loaded(authority: &SessionContext) {
    if let Some(nonce) = authority.memory_turn_tracker().active_nonce() {
        authority
            .memory_turn_tracker()
            .record_call(crate::acp::memory_turn::MemoryCapabilityCall::Policy);
        authority
            .memory_policy_state()
            .store(nonce, Ordering::Release);
        tracing::info!(
            target: "builtin_mcp",
            turn_nonce = nonce,
            revision = crate::user_memory::MEMORY_POLICY_REVISION,
            digest = crate::user_memory::memory_policy_digest(),
            source = "gateway_memory_group",
            "memory policy loaded for current turn"
        );
    }
}
