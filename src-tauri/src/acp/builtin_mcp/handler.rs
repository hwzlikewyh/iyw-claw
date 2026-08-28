use std::future::Future;
use std::sync::Arc;

use axum::http::request::Parts;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::acp::delegation::listener::DelegationListener;

use super::authority::SessionContext;
use super::capability::ResolvedCapability;
use super::delivery::RelayDelivery;
use super::diagnostics::{
    gateway_error_stage, invocation_error_stage, log_tools_list, GatewayCallTrace,
};
use super::gateway::PluginInstallRequest;
use super::gateway::{self, GatewayAction};
use super::http::AuthenticatedRequest;
use super::invocation::{
    agent_wire_name, ensure_active, execute_invocation, InvocationContext,
    InvocationDependencies,
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
            authority.features(),
            authority.cwd(),
            authority.agent_type(),
            context.ct.clone(),
            authority.cancellation().clone(),
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
            GatewayAction::PluginInstallRequest(request) => {
                let request_id = serde_json::to_value(&context.id)
                    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                let result = self
                    .install_plugin(authority, delivery, request, request_id, context.ct)
                    .await;
                match &result {
                    Ok(value) => trace.log_result(value),
                    Err(error) => trace.log_error(invocation_error_stage(error), error),
                }
                result
            }
        }
    }

    async fn install_plugin(
        &self,
        authority: SessionContext,
        delivery: Option<RelayDelivery>,
        request: PluginInstallRequest,
        request_id: Value,
        request_cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        ensure_active(&authority, &request_cancel)?;
        let database = crate::plugin_runtime::global::database()
            .ok_or_else(|| ErrorData::internal_error("plugin database is unavailable", None))?;
        let detail = crate::commands::skill_market::detail_core(
            &database,
            request.skill_id.clone(),
            Some(request.version.clone()),
        )
        .await
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        if detail.skill.current_version.package_type
            != crate::commands::skill_market::SkillPackageType::Plugin
            || !detail
                .skill
                .current_version
                .plugin
                .as_ref()
                .is_some_and(|plugin| plugin.schema_version >= 2)
            || detail.skill.publisher_type != "official"
        {
            return Err(ErrorData::invalid_params(
                "only official v2 plugins can be installed",
                None,
            ));
        }
        let authoritative_name = detail.skill.slug;
        let authoritative_version = detail.skill.current_version.version;
        let permissions = detail
            .skill
            .current_version
            .plugin
            .as_ref()
            .and_then(|plugin| plugin.permissions.as_ref())
            .ok_or_else(|| ErrorData::invalid_params("plugin permissions are missing", None))?;
        if request.plugin_name != authoritative_name {
            return Err(ErrorData::invalid_params(
                "plugin identity does not match the market release",
                None,
            ));
        }
        if authoritative_version != request.version {
            return Err(ErrorData::invalid_params(
                "requested plugin version is unavailable",
                None,
            ));
        }
        if crate::plugin_runtime::registry::global_snapshot().is_some_and(|snapshot| {
            snapshot
                .plugins
                .values()
                .any(|plugin| plugin.slug == authoritative_name && plugin.available)
        }) {
            return Err(ErrorData::invalid_params(
                "plugin is already installed",
                None,
            ));
        }
        let prompt = format!(
            "Install official plugin {} version {}? This downloads executable plugin code. Permissions: {}",
            authoritative_name, authoritative_version, permission_summary(permissions)
        );
        let ask = ResolvedCapability {
            tool_name: "ask_user_question".to_string(),
            arguments: json!({
                "questions": [{
                    "id": "plugin-install-approval",
                    "header": "Install plugin",
                    "question": prompt,
                    "options": [
                        {"label": "Install", "description": "Download and install the verified plugin."},
                        {"label": "Cancel", "description": "Do not download or install anything."}
                    ]
                }]
            }),
            delivery_ack: None,
        };
        let answer = execute_invocation(
            self.invocation_dependencies(),
            InvocationContext {
                authority: authority.clone(),
                delivery,
                invocation: ask,
                request_id,
                request_cancel: request_cancel.clone(),
            },
        )
        .await?;
        if !approved_install(&answer) {
            return Ok(answer);
        }
        ensure_active(&authority, &request_cancel)?;
        let workspace_key = authority.cwd().to_string_lossy().to_string();
        let agent_type = agent_wire_name(&authority).unwrap_or_default();
        crate::commands::skill_market::install_core(
            &database,
            request.skill_id,
            authoritative_version,
            vec![authority.agent_type()],
        )
        .await
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        crate::db::service::plugin_runtime_state_service::approve_plugin(
            &database,
            &authoritative_name,
            &workspace_key,
            &agent_type,
        )
        .await
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        crate::plugin_runtime::registry::reconcile_global(&database)
            .await
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(CallToolResult::structured(json!({
            "installed": true,
            "catalog_digest": crate::plugin_runtime::registry::global_snapshot()
                .map(|snapshot| snapshot.digest.clone())
                .unwrap_or_default(),
        })))
    }

    async fn invoke_plugin(
        &self,
        invocation: crate::plugin_runtime::types::PluginToolCall,
    ) -> Result<CallToolResult, ErrorData> {
        let router = crate::plugin_runtime::global::router()
            .ok_or_else(|| ErrorData::internal_error("plugin router is unavailable", None))?;
        router.invoke(invocation).await.map_err(|error| {
            ErrorData::internal_error(
                error.message,
                Some(json!({
                    "code": error.code,
                    "effectMayHaveOccurred": error.effect_may_have_occurred,
                })),
            )
        })
    }

    fn invocation_dependencies(&self) -> InvocationDependencies<'_> {
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

fn approved_install(result: &CallToolResult) -> bool {
    result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("answers"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|answer| {
            answer
                .get("selected")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|value| value.as_str() == Some("Install"))
        })
}

fn permission_summary(
    permissions: &crate::commands::skill_market::SkillPluginPermissions,
) -> String {
    let workspace = format!(
        "workspace read [{}], write [{}]",
        summarize_values(&permissions.workspace.read),
        summarize_values(&permissions.workspace.write)
    );
    let network = format!(
        "network connect [{}], resource [{}], frame [{}]",
        summarize_values(&permissions.network.connect_domains),
        summarize_values(&permissions.network.resource_domains),
        summarize_values(&permissions.network.frame_domains)
    );
    format!(
        "{workspace}; {network}; host [{}]",
        summarize_values(&permissions.host)
    )
}

fn summarize_values(values: &[String]) -> String {
    const MAX_SUMMARY_CHARS: usize = 1024;
    let mut summary = String::new();
    for (index, value) in values.iter().enumerate() {
        let addition = if index == 0 {
            value.clone()
        } else {
            format!(", {value}")
        };
        if summary.chars().count() + addition.chars().count() > MAX_SUMMARY_CHARS {
            return format!("{summary}, +{} more", values.len().saturating_sub(index));
        }
        summary.push_str(&addition);
    }
    if summary.is_empty() {
        "none".to_string()
    } else {
        summary
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
