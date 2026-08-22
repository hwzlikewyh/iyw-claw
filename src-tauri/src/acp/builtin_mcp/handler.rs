use std::future::Future;
use std::sync::Arc;

use axum::http::request::Parts;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::acp::delegation::companion::{CompanionBridge, CompanionContext, SpawnResult};
use crate::acp::delegation::listener::DelegationListener;

use super::authority::SessionContext;
use super::cancellation::{post_ack_error, run_call_with_cancellation, CallCancellationPolicy};
use super::capability::ResolvedCapability;
use super::delivery::RelayDelivery;
use super::gateway::{self, GatewayAction};
use super::http::AuthenticatedRequest;
use super::receipt::DeliveryReceiptRegistry;
use super::runtime::RuntimeRegistry;

#[derive(Clone)]
pub(super) struct BuiltinMcpHandler {
    listener: Arc<DelegationListener>,
    runtimes: Arc<RuntimeRegistry>,
    receipts: DeliveryReceiptRegistry,
    lifecycle: Arc<Mutex<()>>,
}

impl BuiltinMcpHandler {
    pub(super) fn new(
        listener: Arc<DelegationListener>,
        runtimes: Arc<RuntimeRegistry>,
        receipts: DeliveryReceiptRegistry,
        lifecycle: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            listener,
            runtimes,
            receipts,
            lifecycle,
        }
    }

    fn authenticated(
        extensions: &rmcp::model::Extensions,
    ) -> Result<(SessionContext, Option<RelayDelivery>), ErrorData> {
        let parts = extensions
            .get::<Parts>()
            .ok_or_else(|| ErrorData::invalid_request("missing HTTP request context", None))?;
        let authenticated = parts
            .extensions
            .get::<AuthenticatedRequest>()
            .ok_or_else(|| ErrorData::invalid_request("unauthorized", None))?;
        Ok((
            authenticated.context().clone(),
            parts.extensions.get::<RelayDelivery>().cloned(),
        ))
    }

    async fn call(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let (authority, delivery) = Self::authenticated(&context.extensions)?;
        ensure_active(&authority, &context.ct)?;
        super::policy::require_call(&authority).await?;
        let action = gateway::dispatch(
            request.name.as_ref(),
            request.arguments,
            authority.features(),
            authority.gateway_server_name(),
        )?;
        match action {
            GatewayAction::Return(result) => Ok(result),
            GatewayAction::Invoke(invocation) => {
                let request_id = serde_json::to_value(&context.id)
                    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                self.invoke(authority, delivery, invocation, request_id, context.ct)
                    .await
            }
        }
    }

    async fn invoke(
        &self,
        authority: SessionContext,
        delivery: Option<RelayDelivery>,
        invocation: ResolvedCapability,
        request_id: Value,
        request_cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let ResolvedCapability {
            tool_name,
            arguments,
            delivery_ack,
        } = invocation;
        let rewrite_delegation_guidance = matches!(
            tool_name.as_str(),
            "delegate_to_agent" | "get_delegation_status" | "cancel_delegation"
        );
        let cancellation_policy = CallCancellationPolicy::for_call(&tool_name, &arguments);
        let delivery_ack_committed = if let Some(receipt) = delivery_ack.as_deref() {
            ensure_active(&authority, &request_cancel)?;
            self.receipts
                .acknowledge_required(authority.connection_id(), receipt)
                .await?;
            true
        } else {
            false
        };
        let credential = self
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
        let bridge = self.bridge(&authority, credential.broker_token());
        let request_cancel_after_call = request_cancel.clone();
        let result = run_call_with_cancellation(
            &bridge,
            request_id,
            tool_name,
            arguments,
            request_cancel,
            authority.cancellation().clone(),
            cancellation_policy,
        )
        .await
        .map_err(|error| {
            post_ack_error(
                delivery_ack_committed,
                error,
                "MCP call interrupted after delivery acknowledgement",
            )
        })?;
        let _lifecycle = self.lifecycle.lock().await;
        let final_policy = if delivery_ack_committed {
            CallCancellationPolicy::CompleteWithUnknownEffect
        } else {
            cancellation_policy
        };
        ensure_active_after_call(&authority, &request_cancel_after_call, final_policy)?;
        map_spawn_result(
            result,
            delivery,
            &self.receipts,
            authority.connection_id(),
            rewrite_delegation_guidance,
        )
        .map_err(|error| {
            post_ack_error(
                delivery_ack_committed,
                error,
                "MCP result unavailable after delivery acknowledgement",
            )
        })
    }

    fn bridge(&self, authority: &SessionContext, broker_token: &str) -> CompanionBridge {
        CompanionBridge::in_process(
            CompanionContext {
                parent_connection_id: authority.connection_id().to_string(),
                socket_path: String::new(),
                token: broker_token.to_string(),
                working_dir: authority.cwd().to_path_buf(),
                agent_type: agent_wire_name(authority),
                features: authority.features().companion_features(),
            },
            Arc::clone(&self.listener),
        )
    }
}

impl ServerHandler for BuiltinMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("iyw-claw", env!("CARGO_PKG_VERSION")))
            .with_instructions(super::service::SERVER_INSTRUCTIONS)
    }

    fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        async move {
            let (authority, _) = Self::authenticated(&context.extensions)?;
            ensure_active(&authority, &context.ct)?;
            let tools = gateway::tools().map_err(catalog_error)?;
            Ok(ListToolsResult::with_all_items(tools))
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, ErrorData>> + MaybeSendFuture + '_ {
        self.call(request, context)
    }
}

fn ensure_active(
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

fn authority_revoked() -> ErrorData {
    ErrorData::invalid_request("MCP authority revoked", None)
}

fn agent_wire_name(authority: &SessionContext) -> Option<String> {
    serde_json::to_value(authority.agent_type())
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
}

fn map_spawn_result(
    result: SpawnResult,
    delivery: Option<RelayDelivery>,
    receipts: &DeliveryReceiptRegistry,
    parent_connection_id: &str,
    rewrite_delegation_guidance: bool,
) -> Result<CallToolResult, ErrorData> {
    let Some(response) = result.response else {
        return Err(ErrorData::invalid_request("MCP request cancelled", None));
    };
    if let Some(error) = response.error {
        return Err(map_json_rpc_error(
            error.code,
            error.message,
            error.data,
            rewrite_delegation_guidance,
        ));
    }
    let value = response
        .result
        .ok_or_else(|| ErrorData::internal_error("missing MCP tool result", None))?;
    let mut mapped: CallToolResult = serde_json::from_value(value)
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
    if rewrite_delegation_guidance {
        super::capability_response::rewrite_result(&mut mapped);
    }
    if let Some(callback) = result.after_relay {
        if !receipts.attach(&mut mapped, delivery, parent_connection_id, callback) {
            tracing::warn!(
                target: "builtin_mcp",
                "HTTP MCP delivery receipt unavailable; feedback delivery rejected"
            );
            return Err(ErrorData::internal_error(
                "feedback delivery receipt capacity reached; retry the request",
                None,
            ));
        }
    }
    Ok(mapped)
}

fn map_json_rpc_error(
    code: i64,
    message: String,
    data: Option<Value>,
    rewrite_delegation_guidance: bool,
) -> ErrorData {
    let (message, data) = if rewrite_delegation_guidance {
        super::capability_response::rewrite_error(message, data)
    } else {
        (message, data)
    };
    match code {
        -32601 => ErrorData::invalid_request(message, data),
        -32602 => ErrorData::invalid_params(message, data),
        _ => ErrorData::internal_error(message, data),
    }
}

fn catalog_error(error: serde_json::Error) -> ErrorData {
    tracing::error!(
        target: "builtin_mcp",
        error = %error,
        "failed to build HTTP MCP gateway catalog"
    );
    ErrorData::internal_error("failed to build MCP gateway catalog", None)
}
