use std::sync::Arc;

use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::acp::delegation::companion::{CompanionBridge, CompanionContext, SpawnResult};
use crate::acp::delegation::listener::DelegationListener;

use super::authority::SessionContext;
use super::cancellation::{post_ack_error, run_call_with_cancellation, CallCancellationPolicy};
use super::capability::ResolvedCapability;
use super::delivery::RelayDelivery;
use super::receipt::DeliveryReceiptRegistry;
use super::result::{map_spawn_result, SpawnResultContext};
use super::runtime::RuntimeRegistry;

#[derive(Clone, Copy)]
pub(super) struct InvocationDependencies<'a> {
    pub(super) listener: &'a Arc<DelegationListener>,
    pub(super) runtimes: &'a Arc<RuntimeRegistry>,
    pub(super) receipts: &'a DeliveryReceiptRegistry,
    pub(super) lifecycle: &'a Arc<Mutex<()>>,
}

pub(super) struct InvocationContext {
    pub(super) authority: SessionContext,
    pub(super) delivery: Option<RelayDelivery>,
    pub(super) invocation: ResolvedCapability,
    pub(super) request_id: Value,
    pub(super) request_cancel: CancellationToken,
}

struct InvocationRun {
    request_id: Value,
    tool_name: String,
    arguments: Value,
    request_cancel: CancellationToken,
    cancellation_policy: CallCancellationPolicy,
}

struct InvocationFinish<'a> {
    result: SpawnResult,
    delivery: Option<RelayDelivery>,
    authority: &'a SessionContext,
    request_cancel: &'a CancellationToken,
    cancellation_policy: CallCancellationPolicy,
    delivery_ack_committed: bool,
    rewrite_delegation_guidance: bool,
}

struct PreparedInvocation {
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    bridge: CompanionBridge,
    run: InvocationRun,
    request_cancel_after_call: CancellationToken,
    delivery_ack_committed: bool,
    rewrite_delegation_guidance: bool,
}

struct DeliveryAckContext<'a> {
    receipts: &'a DeliveryReceiptRegistry,
    authority: &'a SessionContext,
    request_cancel: &'a CancellationToken,
    delivery_ack: Option<&'a str>,
}

pub(super) async fn execute_invocation(
    dependencies: InvocationDependencies<'_>,
    context: InvocationContext,
) -> Result<CallToolResult, ErrorData> {
    let prepared = prepare_invocation(dependencies, context).await?;
    let PreparedInvocation {
        authority,
        delivery,
        bridge,
        run,
        request_cancel_after_call,
        delivery_ack_committed,
        rewrite_delegation_guidance,
    } = prepared;
    let cancellation_policy = run.cancellation_policy;
    let result = run_invocation(&bridge, run, authority.cancellation().clone())
        .await
        .map_err(|error| {
            post_ack_error(
                delivery_ack_committed,
                error,
                "MCP call interrupted after delivery acknowledgement",
            )
        })?;
    finish_invocation(
        dependencies,
        InvocationFinish {
            result,
            delivery,
            authority: &authority,
            request_cancel: &request_cancel_after_call,
            cancellation_policy,
            delivery_ack_committed,
            rewrite_delegation_guidance,
        },
    )
    .await
}

async fn prepare_invocation(
    dependencies: InvocationDependencies<'_>,
    context: InvocationContext,
) -> Result<PreparedInvocation, ErrorData> {
    let InvocationContext {
        authority,
        delivery,
        invocation,
        request_id,
        request_cancel,
    } = context;
    let ResolvedCapability {
        tool_name,
        arguments,
        delivery_ack,
    } = invocation;
    let rewrite_delegation_guidance = delegation_tool(&tool_name);
    let cancellation_policy = CallCancellationPolicy::for_call(&tool_name, &arguments);
    let delivery_ack_committed = acknowledge_delivery(DeliveryAckContext {
        receipts: dependencies.receipts,
        authority: &authority,
        request_cancel: &request_cancel,
        delivery_ack: delivery_ack.as_deref(),
    })
    .await?;
    let bridge = bridge_after_ack(dependencies, &authority, delivery_ack_committed).await?;
    let request_cancel_after_call = request_cancel.clone();
    Ok(PreparedInvocation {
        authority,
        delivery,
        bridge,
        run: InvocationRun {
            request_id,
            tool_name,
            arguments,
            request_cancel,
            cancellation_policy,
        },
        request_cancel_after_call,
        delivery_ack_committed,
        rewrite_delegation_guidance,
    })
}

pub(super) fn ensure_active(
    authority: &SessionContext,
    request_cancel: &CancellationToken,
) -> Result<(), ErrorData> {
    if request_cancel.is_cancelled() {
        return Err(ErrorData::invalid_request("MCP request cancelled", None));
    }
    if authority.cancellation().is_cancelled() {
        return Err(authority_revoked());
    }
    Ok(())
}

async fn acknowledge_delivery(context: DeliveryAckContext<'_>) -> Result<bool, ErrorData> {
    let Some(receipt) = context.delivery_ack else {
        return Ok(false);
    };
    ensure_active(context.authority, context.request_cancel)?;
    context
        .receipts
        .acknowledge_required(context.authority.connection_id(), receipt)
        .await?;
    Ok(true)
}

async fn bridge_after_ack(
    dependencies: InvocationDependencies<'_>,
    authority: &SessionContext,
    delivery_ack_committed: bool,
) -> Result<CompanionBridge, ErrorData> {
    let credential = dependencies
        .runtimes
        .get(authority.connection_id())
        .await
        .ok_or_else(|| {
            post_ack_error(
                delivery_ack_committed,
                authority_revoked(),
                "MCP authority revoked after delivery acknowledgement",
            )
        })?;
    Ok(build_bridge(
        dependencies.listener,
        authority,
        credential.broker_token(),
    ))
}

async fn run_invocation(
    bridge: &CompanionBridge,
    request: InvocationRun,
    authority_cancel: CancellationToken,
) -> Result<SpawnResult, ErrorData> {
    run_call_with_cancellation(
        bridge,
        request.request_id,
        request.tool_name,
        request.arguments,
        request.request_cancel,
        authority_cancel,
        request.cancellation_policy,
    )
    .await
}

async fn finish_invocation(
    dependencies: InvocationDependencies<'_>,
    context: InvocationFinish<'_>,
) -> Result<CallToolResult, ErrorData> {
    let _lifecycle = dependencies.lifecycle.lock().await;
    let final_policy = if context.delivery_ack_committed {
        CallCancellationPolicy::CompleteWithUnknownEffect
    } else {
        context.cancellation_policy
    };
    ensure_active_after_call(context.authority, context.request_cancel, final_policy)?;
    map_spawn_result(SpawnResultContext {
        result: context.result,
        delivery: context.delivery,
        receipts: dependencies.receipts,
        parent_connection_id: context.authority.connection_id(),
        rewrite_delegation_guidance: context.rewrite_delegation_guidance,
    })
    .map_err(|error| {
        post_ack_error(
            context.delivery_ack_committed,
            error,
            "MCP result unavailable after delivery acknowledgement",
        )
    })
}

fn ensure_active_after_call(
    authority: &SessionContext,
    request_cancel: &CancellationToken,
    policy: CallCancellationPolicy,
) -> Result<(), ErrorData> {
    if request_cancel.is_cancelled() {
        return Err(policy.error_after_call("MCP request cancelled after call"));
    }
    if authority.cancellation().is_cancelled() {
        return Err(policy.error_after_call("MCP authority revoked after call"));
    }
    Ok(())
}

fn build_bridge(
    listener: &Arc<DelegationListener>,
    authority: &SessionContext,
    broker_token: &str,
) -> CompanionBridge {
    CompanionBridge::in_process(
        CompanionContext {
            parent_connection_id: authority.connection_id().to_string(),
            socket_path: String::new(),
            token: broker_token.to_string(),
            working_dir: authority.cwd().to_path_buf(),
            agent_type: agent_wire_name(authority),
            features: authority.features().companion_features(),
        },
        Arc::clone(listener),
    )
}

fn authority_revoked() -> ErrorData {
    ErrorData::invalid_request("MCP authority revoked", None)
}

fn delegation_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "delegate_to_agent" | "get_delegation_status" | "cancel_delegation"
    )
}

fn agent_wire_name(authority: &SessionContext) -> Option<String> {
    serde_json::to_value(authority.agent_type())
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
}
