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
    registry: PluginRegistry,
    supervisor: Arc<PluginRuntimeSupervisor>,
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
        let route = resolve_route(plugin, &call.context.capability_id)?;
        validate_activation(
            plugin,
            &route.connector_key,
            &call.context.workspace_key,
            call.context.agent_type,
        )?;
        let spec = launch_spec(plugin, &route.connector_key, &call)?;
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
                route.tool_name.clone(),
                call.arguments,
                call.context.cancellation.clone(),
                call.context.authority_cancellation.clone(),
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
        let snapshot = self.registry.snapshot();
        let plugin = snapshot
            .plugins
            .get(&request.plugin_slug)
            .filter(|plugin| plugin.available)
            .ok_or_else(|| unavailable("Plugin is not available"))?;
        if request.plugin_version != plugin.version {
            return Err(unavailable("Plugin app version is no longer current"));
        }
        if request.permission_revision != plugin.permissions_digest {
            return Err(unavailable("Plugin permission revision changed"));
        }
        let app = component_config(plugin, "app", &request.app_key)?;
        let connector = app["connectorKey"]
            .as_str()
            .ok_or_else(|| unavailable("Plugin app connector is missing"))?;
        validate_activation(
            plugin,
            connector,
            &request.workspace_key,
            request.agent_type,
        )?;
        let call = PluginToolCall {
            context: super::types::PluginCallContext {
                connection_id: String::new(),
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
                app["resourceUri"]
                    .as_str()
                    .ok_or_else(|| unavailable("Plugin app resource URI is missing"))?
                    .to_string(),
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
        let connector_key = app["connectorKey"]
            .as_str()
            .ok_or_else(|| contract_error("Plugin app connector is missing"))?;
        let capability_id = app_tool_capability(plugin, connector_key, &tool_name)?;
        let routed_call = app_plugin_call(plugin, call, capability_id);
        validate_permission(plugin, &routed_call)?;
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
                tool_name,
                routed_call.arguments,
                routed_call.context.cancellation,
                routed_call.context.authority_cancellation,
            )
            .await
    }
}

pub(super) struct ResolvedRoute {
    pub capability_key: String,
    pub connector_key: String,
    pub tool_name: String,
}

fn validate_permission(
    plugin: &PluginDescriptor,
    call: &PluginToolCall,
) -> Result<(), PluginInvokeError> {
    if call.context.permission_revision != plugin.permissions_digest {
        return Err(unavailable("Plugin permission revision changed"));
    }
    let granted = plugin.permission_grants.iter().any(|grant| {
        grant.permissions_digest == plugin.permissions_digest
            && grant.grant_state == "granted"
            && (grant.scope == "global" || grant.workspace_key == call.context.workspace_key)
    });
    if !granted {
        return Err(unavailable("Plugin permissions are not granted"));
    }
    Ok(())
}

fn validate_activation(
    plugin: &PluginDescriptor,
    connector_key: &str,
    workspace_key: &str,
    agent_type: crate::models::AgentType,
) -> Result<(), PluginInvokeError> {
    let agent_type = agent_type.as_wire();
    let enabled = plugin.activations.iter().any(|activation| {
        activation.component_key == connector_key
            && activation.routing_mode == "host_gateway"
            && activation.requested_enabled
            && (activation.agent_type.is_empty() || activation.agent_type == agent_type.as_ref())
            && (activation.scope == "global"
                || (activation.scope == "workspace" && activation.workspace_key == workspace_key))
    });
    if !enabled {
        return Err(unavailable("Plugin connector is disabled"));
    }
    Ok(())
}

fn resolve_route(
    plugin: &PluginDescriptor,
    capability_id: &str,
) -> Result<ResolvedRoute, PluginInvokeError> {
    for component in &plugin.manifest.components {
        if component.kind != "capability" {
            continue;
        }
        let config = component.config.as_ref().ok_or_else(|| {
            PluginInvokeError::before_effect(
                "plugin_contract_mismatch",
                "Capability config is missing",
            )
        })?;
        if config["id"].as_str() == Some(capability_id) {
            return Ok(ResolvedRoute {
                capability_key: component.key.clone(),
                connector_key: config["connectorKey"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                tool_name: config["toolName"].as_str().unwrap_or_default().to_string(),
            });
        }
    }
    Err(unavailable("Plugin capability is unavailable"))
}

fn app_tool_capability(
    plugin: &PluginDescriptor,
    connector_key: &str,
    tool_name: &str,
) -> Result<String, PluginInvokeError> {
    plugin
        .manifest
        .components
        .iter()
        .filter(|component| component.kind == "capability")
        .filter_map(|component| component.config.as_ref())
        .find(|config| {
            config["connectorKey"].as_str() == Some(connector_key)
                && config["toolName"].as_str() == Some(tool_name)
        })
        .and_then(|config| config["id"].as_str())
        .map(str::to_string)
        .ok_or_else(|| unavailable("Plugin app tool is not declared by its connector"))
}

fn app_plugin_call(
    plugin: &PluginDescriptor,
    call: PluginAppToolCall,
    capability_id: String,
) -> PluginToolCall {
    PluginToolCall {
        context: super::types::PluginCallContext {
            connection_id: String::new(),
            plugin_slug: plugin.slug.clone(),
            capability_id,
            workspace_key: call.workspace_key,
            workspace_dir: call.workspace_dir,
            agent_type: call.agent_type,
            host_gateway_supported: true,
            cancellation: tokio_util::sync::CancellationToken::new(),
            authority_cancellation: tokio_util::sync::CancellationToken::new(),
            permission_revision: plugin.permissions_digest.clone(),
        },
        arguments: call.arguments,
    }
}

fn contract_error(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_contract_mismatch", message)
}

fn unavailable(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_unavailable", message)
}
