#[path = "router_support.rs"]
mod support;
pub(super) use support::{ResolvedRoute, validate_activation, validate_permission};
use support::{
    app_plugin_call, app_tool_capability, contract_error, resolve_route, unavailable,
};

use std::sync::Arc;

use serde_json::Value;

use super::app_binding::{resolve_app_intent, AppIntentInput};
use super::registry::{PluginDescriptor, PluginRegistry};
use super::runtime_spec::{component_config, launch_spec};
use super::supervisor::PluginRuntimeSupervisor;
use super::types::{
    PluginAppReadRequest, PluginAppToolCall, PluginInvokeError, PluginInvokeResult,
    PluginRouteResult, PluginRoutedResult, PluginToolCall,
};
use rmcp::model::ReadResourceResult;

#[derive(Clone)]
pub struct PluginRouter {
    pub(super) registry: PluginRegistry,
    pub(super) supervisor: Arc<PluginRuntimeSupervisor>,
}

impl PluginRouter {
    pub fn new(registry: PluginRegistry, supervisor: Arc<PluginRuntimeSupervisor>) -> Self {
        Self {
            registry,
            supervisor,
        }
    }

    pub async fn invoke(&self, call: PluginToolCall) -> PluginRouteResult {
        if !call.context.host_gateway_supported {
            return Err(unavailable("Agent session does not support HostGateway"));
        }
        let snapshot = self.registry.snapshot();
        let plugin = snapshot
            .plugins
            .get(&call.context.plugin_slug)
            .filter(|plugin| plugin.available)
            .ok_or_else(|| unavailable("Plugin is not available"))?;
        validate_permission(plugin, &call)?;
        super::dependency_scope::require(plugin, &snapshot.plugins, (&call.context.workspace_key, call.context.agent_type.as_wire().as_ref()))?;
        let route = resolve_route(plugin, &call.context.capability_id)?;
        if !super::native_compat::model_tool_allowed(
            std::path::Path::new(&plugin.install_root),
            &route.connector_key,
            &route.tool_name,
        ) {
            return Err(unavailable("Plugin tool is restricted to its app"));
        }
        validate_activation(
            plugin,
            &route.connector_key,
            &call.context.workspace_key,
            call.context.agent_type,
        )?;
        let spec = launch_spec(plugin, &route.connector_key, &call)?;
        self.verify_dependency_accounts(&call).await?;
        let requested_mode = call
            .arguments
            .get("displayMode")
            .and_then(Value::as_str)
            .map(str::to_string);
        let tool_arguments = call.arguments.clone();
        let result = self
            .supervisor
            .invoke(
                spec,
                super::types::PluginRuntimeCall {
                    tool_name: route.tool_name.clone(),
                    arguments: call.arguments,
                    conversation_id: call.context.conversation_id,
                    cancellation: call.context.cancellation.clone(),
                    authority_cancellation: call.context.authority_cancellation.clone(),
                },
            )
            .await?;
        let launch_payload = serde_json::json!({
            "arguments": tool_arguments,
            "result": result,
        });
        let app = resolve_app_intent(AppIntentInput {
            plugin,
            route: &route,
            context: &call.context,
            requested_mode: requested_mode.as_deref(),
            launch_payload,
        })?;
        Ok(PluginRoutedResult { result, app })
    }

    pub async fn read_app_resource(
        &self,
        request: PluginAppReadRequest,
    ) -> Result<ReadResourceResult, PluginInvokeError> {
        self.read_app_resource_uri(request, None).await
    }

    pub(crate) async fn read_app_resource_uri(
        &self,
        request: PluginAppReadRequest,
        requested_uri: Option<String>,
    ) -> Result<ReadResourceResult, PluginInvokeError> {
        let snapshot = self.registry.snapshot();
        let plugin = snapshot
            .plugins
            .get(&request.plugin_slug)
            .filter(|plugin| plugin.available)
            .ok_or_else(|| unavailable("Plugin is not available"))?;
        if request.plugin_version != plugin.version {
            return Err(unavailable("Plugin app version is no longer current"));
        }
        super::dependency_scope::require(plugin, &snapshot.plugins, (&request.workspace_key, request.agent_type.as_wire().as_ref()))?;
        if request.permission_revision != plugin.permissions_digest {
            return Err(unavailable("Plugin permission revision changed"));
        }
        let app = component_config(plugin, "app", &request.app_key)?;
        let connector = app["connectorKey"]
            .as_str()
            .ok_or_else(|| unavailable("Plugin app connector is missing"))?;
        let uri = app["resourceUri"]
            .as_str()
            .ok_or_else(|| unavailable("Plugin app resource URI is missing"))?;
        let uri = requested_uri.as_deref().unwrap_or(uri);
        if !super::native_resources::allowed(
            std::path::Path::new(&plugin.install_root), connector,
            (app["resourceUri"].as_str().unwrap_or_default(), uri),
        )?
        {
            return Err(unavailable(
                "Plugin app resource is not declared by its connector",
            ));
        }
        validate_activation(
            plugin,
            connector,
            &request.workspace_key,
            request.agent_type,
        )?;
        let call = PluginToolCall {
            context: super::types::PluginCallContext {
                connection_id: String::new(),
                conversation_id: None,
                plugin_slug: request.plugin_slug.clone(),
                capability_id: app["capabilityKey"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                workspace_key: request.workspace_key.clone(),
                workspace_dir: request.workspace_dir,
                agent_type: request.agent_type,
                host_gateway_supported: true,
                cancellation: request.cancellation.clone(),
                authority_cancellation: request.authority_cancellation.clone(),
                permission_revision: request.permission_revision.clone(),
            },
            arguments: serde_json::Map::new(),
        };
        validate_permission(plugin, &call)?;
        let spec = launch_spec(plugin, connector, &call)?;
        self.supervisor
            .read_resource(
                spec,
                uri.to_string(),
                request.cancellation,
                request.authority_cancellation,
            )
            .await
    }

    pub async fn invoke_app_tool(&self, call: PluginAppToolCall) -> PluginInvokeResult {
        let tool_name = call.tool_name.clone();
        let snapshot = self.registry.snapshot();
        let plugin = snapshot
            .plugins
            .get(&call.plugin_slug)
            .filter(|plugin| plugin.available)
            .ok_or_else(|| unavailable("Plugin is not available"))?;
        if plugin.version != call.plugin_version {
            return Err(unavailable("Plugin app version is no longer current"));
        }
        let app = component_config(plugin, "app", &call.app_key)?;
        super::dependency_scope::require(plugin, &snapshot.plugins, (&call.workspace_key, call.agent_type.as_wire().as_ref()))?;
        let connector_key = app["connectorKey"]
            .as_str()
            .ok_or_else(|| contract_error("Plugin app connector is missing"))?;
        let capability_id = app_tool_capability(plugin, connector_key, &tool_name)?;
        let routed_call = app_plugin_call(plugin, call, capability_id);
        validate_permission(plugin, &routed_call)?;
        self.verify_dependency_accounts(&routed_call).await?;
        validate_activation(
            plugin,
            connector_key,
            &routed_call.context.workspace_key,
            routed_call.context.agent_type,
        )?;
        let spec = launch_spec(plugin, connector_key, &routed_call)?;
        self.supervisor
            .invoke(
                spec,
                super::types::PluginRuntimeCall {
                    tool_name,
                    arguments: routed_call.arguments,
                    conversation_id: routed_call.context.conversation_id,
                    cancellation: routed_call.context.cancellation,
                    authority_cancellation: routed_call.context.authority_cancellation,
                },
            )
            .await
    }
}
