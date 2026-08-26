use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use super::registry::{PluginDescriptor, PluginRegistry};
use super::supervisor::PluginRuntimeSupervisor;
use super::types::{
    ExpectedTool, PluginInvokeError, PluginInvokeResult, PluginToolCall, RuntimeKey,
    RuntimeLaunchSpec,
};

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

    pub async fn invoke(&self, call: PluginToolCall) -> PluginInvokeResult {
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
        validate_activation(plugin, &route.connector_key, &call.context.workspace_key)?;
        let spec = launch_spec(plugin, &route.connector_key, &call)?;
        self.supervisor
            .invoke(
                spec,
                route.tool_name,
                call.arguments,
                call.context.cancellation,
            )
            .await
    }
}

struct ResolvedRoute {
    connector_key: String,
    tool_name: String,
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
) -> Result<(), PluginInvokeError> {
    let enabled = plugin.activations.iter().any(|activation| {
        activation.component_key == connector_key
            && activation.routing_mode == "host_gateway"
            && activation.requested_enabled
            && (activation.scope == "global"
                || activation.workspace_key.is_empty()
                || activation.workspace_key == workspace_key)
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

fn launch_spec(
    plugin: &PluginDescriptor,
    connector_key: &str,
    call: &PluginToolCall,
) -> Result<RuntimeLaunchSpec, PluginInvokeError> {
    let connector = component_config(plugin, "connector", connector_key)?;
    if connector["routing"]["mode"].as_str() != Some("host_gateway") {
        return Err(unavailable("Plugin connector is not HostGateway routed"));
    }
    let runtime_key = connector["runtimeKey"].as_str().unwrap_or_default();
    let runtime = component_config(plugin, "runtime", runtime_key)?;
    let paths = crate::acp::agent_storage::AgentStoragePaths::active()
        .ok_or_else(|| unavailable("Plugin storage is unavailable"))?;
    Ok(RuntimeLaunchSpec {
        key: RuntimeKey {
            plugin_slug: plugin.slug.clone(),
            plugin_version: plugin.version.clone(),
            connector_key: connector_key.to_string(),
            workspace_key: call.context.workspace_key.clone(),
        },
        runtime_kind: runtime["kind"].as_str().unwrap_or_default().to_string(),
        entrypoint: runtime["entrypoint"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        install_root: PathBuf::from(&plugin.install_root),
        plugin_data_dir: paths.root().join("plugin-data").join(&plugin.slug),
        workspace_dir: call.context.workspace_dir.clone(),
        permission_revision: plugin.permissions_digest.clone(),
        expected_tools: expected_tools(plugin, connector_key)?,
        resource_uris: resource_uris(plugin, connector_key)?,
    })
}

fn component_config<'a>(
    plugin: &'a PluginDescriptor,
    kind: &str,
    key: &str,
) -> Result<&'a Value, PluginInvokeError> {
    plugin
        .manifest
        .components
        .iter()
        .find(|component| component.kind == kind && component.key == key)
        .and_then(|component| component.config.as_ref())
        .ok_or_else(|| {
            PluginInvokeError::before_effect(
                "plugin_contract_mismatch",
                format!("Missing {kind} component {key}"),
            )
        })
}

fn expected_tools(
    plugin: &PluginDescriptor,
    connector_key: &str,
) -> Result<Vec<ExpectedTool>, PluginInvokeError> {
    let mut result = Vec::new();
    for component in &plugin.manifest.components {
        if component.kind != "capability" {
            continue;
        }
        let config = component.config.as_ref().ok_or_else(|| {
            PluginInvokeError::before_effect("plugin_contract_mismatch", "Capability config")
        })?;
        if config["connectorKey"].as_str() == Some(connector_key) {
            result.push(ExpectedTool {
                name: config["toolName"].as_str().unwrap_or_default().to_string(),
                schema_path: config["schemaPath"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            });
        }
    }
    Ok(result)
}

fn resource_uris(
    plugin: &PluginDescriptor,
    connector_key: &str,
) -> Result<Vec<String>, PluginInvokeError> {
    let mut result = Vec::new();
    for component in &plugin.manifest.components {
        if component.kind != "app" {
            continue;
        }
        let config = component.config.as_ref().ok_or_else(|| {
            PluginInvokeError::before_effect("plugin_contract_mismatch", "App config")
        })?;
        if config["connectorKey"].as_str() == Some(connector_key) {
            result.push(
                config["resourceUri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
        }
    }
    Ok(result)
}

fn unavailable(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_unavailable", message)
}
