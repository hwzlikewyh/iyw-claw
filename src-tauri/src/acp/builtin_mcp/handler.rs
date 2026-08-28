use std::future::Future;
use std::sync::Arc;

use axum::http::request::Parts;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::acp::delegation::listener::DelegationListener;

use super::authority::SessionContext;
use super::delivery::RelayDelivery;
use super::diagnostics::{
    gateway_error_stage, invocation_error_stage, log_tools_list, GatewayCallTrace,
};
use super::gateway::{self, GatewayAction};
use super::http::AuthenticatedRequest;
use super::invocation::{
    ensure_active, execute_invocation, InvocationContext, InvocationDependencies,
};
use super::receipt::DeliveryReceiptRegistry;
use super::result::catalog_error;
use super::runtime::RuntimeRegistry;
use super::tool_identity::resolve_gateway_route;

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
        let route = resolve_gateway_route(request.name.as_ref(), authority.gateway_server_name());
        let trace = GatewayCallTrace::new(&authority, &request, route);
        trace.log_received();
        authorize_request(&authority, &context.ct, &trace).await?;
        let Some(route) = route else {
            let error = ErrorData::invalid_params("unknown MCP gateway tool", None);
            trace.log_error("gateway_route", &error);
            return Err(error);
        };
        let action = gateway::dispatch(
            route.tool(),
            request.arguments,
            gateway::GatewaySession {
                connection_id: authority.connection_id(),
                features: authority.features(),
                cwd: authority.cwd(),
                agent_type: authority.agent_type(),
                request_cancel: context.ct.clone(),
                authority_cancel: authority.cancellation().clone(),
            },
        )
        .map_err(|error| {
            trace.log_error(gateway_error_stage(&error), &error);
            error
        })?;
        match action {
            GatewayAction::Return(result) => {
                trace.log_result(&result);
                Ok(result)
            }
            GatewayAction::Invoke(invocation) => {
                let request_id = serde_json::to_value(&context.id)
                    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                let result = execute_invocation(
                    self.invocation_dependencies(),
                    InvocationContext {
                        authority,
                        delivery,
                        invocation,
                        request_id,
                        request_cancel: context.ct,
                    },
                )
                .await;
                match &result {
                    Ok(value) => trace.log_result(value),
                    Err(error) => trace.log_error(invocation_error_stage(error), error),
                }
                result
            }
            GatewayAction::PluginInvoke(invocation) => {
                let result = self.invoke_plugin(invocation).await;
                match &result {
                    Ok(value) => trace.log_result(value),
                    Err(error) => trace.log_error(invocation_error_stage(error), error),
                }
                result
            }
            GatewayAction::PluginControl(request) => {
                let request_id = serde_json::to_value(&context.id)
                    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                let result = self
                    .control_plugin(authority, delivery, request, request_id, context.ct)
                    .await;
                match &result {
                    Ok(value) => trace.log_result(value),
                    Err(error) => trace.log_error(invocation_error_stage(error), error),
                }
                result
            }
        }
    }

    async fn invoke_plugin(
        &self,
        invocation: crate::plugin_runtime::types::PluginToolCall,
    ) -> Result<CallToolResult, ErrorData> {
        let router = crate::plugin_runtime::global::router()
            .ok_or_else(|| ErrorData::internal_error("plugin router is unavailable", None))?;
        let routed = router.invoke(invocation).await.map_err(|error| {
            ErrorData::internal_error(
                error.message,
                Some(json!({
                    "code": error.code,
                    "effectMayHaveOccurred": error.effect_may_have_occurred,
                })),
            )
        })?;
        let mut result = routed.result;
        if let Some(app) = routed.app {
            let ticket = crate::plugin_runtime::global::app_launch_broker().issue(app);
            crate::plugin_runtime::app_launch_broker::attach_ticket(&mut result, ticket);
        }
        Ok(result)
    }

    pub(super) fn invocation_dependencies(&self) -> InvocationDependencies<'_> {
        InvocationDependencies {
            listener: &self.listener,
            runtimes: &self.runtimes,
            receipts: &self.receipts,
            lifecycle: &self.lifecycle,
        }
    }
}

async fn authorize_request(
    authority: &SessionContext,
    request_cancel: &CancellationToken,
    trace: &GatewayCallTrace,
) -> Result<(), ErrorData> {
    ensure_active(authority, request_cancel).map_err(|error| {
        trace.log_error("preflight", &error);
        error
    })?;
    super::policy::require_call(authority)
        .await
        .map_err(|error| {
            trace.log_error("policy", &error);
            error
        })
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
            log_tools_list(&authority, tools.len());
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
