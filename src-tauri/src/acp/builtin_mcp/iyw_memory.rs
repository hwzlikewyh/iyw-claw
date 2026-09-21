use std::sync::atomic::Ordering;

use rmcp::model::{CallToolResult, Tool};
use rmcp::ErrorData;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::authority::SessionContext;
use super::delivery::RelayDelivery;
use super::gateway::{self, MemoryGroupRequest};
use super::invocation::{execute_invocation, InvocationContext, InvocationDependencies};

pub(super) fn parameters_schema() -> Value {
    scoped_parameters_schema(None)
}

fn scoped_parameters_schema(features: Option<&super::features::FeatureSnapshot>) -> Value {
    let mut variants = Vec::new();
    for (operation, name) in [
        ("recall", "memory_recall"),
        ("append", "append_user_memory"),
        ("propose", "propose_user_memory"),
        ("retire", "retire_user_memory"),
        ("documents.read", "read_user_memory_documents"),
        ("candidates.list", "list_user_memory_candidates"),
    ] {
        if features.is_some_and(|features| !features.should_list(name)) {
            continue;
        }
        let mut schema = super::interaction_tools::embedded_tool(name)["inputSchema"].clone();
        schema["description"] = json!(format!("Complete parameters for operation={operation}."));
        variants.push(schema);
    }
    variants.push(json!({
        "type": "object",
        "description": "Other operations only: use the exact schema returned by read_iyw_capability.",
        "additionalProperties": true
    }));
    json!({
        "type": "object",
        "description": "Choose the branch matching operation. Put its fields here without another wrapper. Common-operation schemas are complete and require no metadata read; runtime validates the selected operation exactly.",
        "anyOf": variants
    })
}

pub(super) fn project_tool(
    mut tool: Tool,
    features: &super::features::FeatureSnapshot,
) -> Option<Tool> {
    let available = super::gateway_tools::MEMORY_CAPABILITIES
        .iter()
        .filter(|(_, id)| {
            super::capability_registry::tool_name_for_capability_id(id)
                .is_some_and(|name| features.should_list(name))
        })
        .collect::<Vec<_>>();
    if available.is_empty() {
        return None;
    }
    let operations = available
        .iter()
        .map(|(operation, _)| *operation)
        .collect::<Vec<_>>();
    let mapping = available
        .iter()
        .map(|(operation, id)| format!("{operation}={id}"))
        .collect::<Vec<_>>()
        .join("; ");
    let mut schema = (*tool.input_schema).clone();
    let properties = schema.get_mut("properties")?.as_object_mut()?;
    properties.insert("operation".into(), json!({"type":"string","enum":operations,
        "description":format!("Only these operations are enabled in this session. Common operations have inline schemas; read other mapped schemas once: {mapping}")}));
    properties.insert(
        "parameters".into(),
        scoped_parameters_schema(Some(features)),
    );
    tool.input_schema = std::sync::Arc::new(schema);
    Some(tool)
}

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
