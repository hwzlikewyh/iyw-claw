use super::*;

pub(crate) struct ResolvedRoute {
    pub(crate) capability_key: String,
    pub(crate) connector_key: String,
    pub(crate) tool_name: String,
}

pub(super) fn validate_permission(
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

pub(super) fn validate_activation(
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

pub(super) fn resolve_route(
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

pub(super) fn app_tool_capability(
    plugin: &PluginDescriptor,
    connector_key: &str,
    tool_name: &str,
) -> Result<String, PluginInvokeError> {
    if !crate::plugin_runtime::native_compat::app_tool_allowed(
        std::path::Path::new(&plugin.install_root),
        connector_key,
        tool_name,
    ) {
        return Err(unavailable("Plugin tool is restricted to the model"));
    }
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

pub(super) fn app_plugin_call(
    plugin: &PluginDescriptor,
    call: PluginAppToolCall,
    capability_id: String,
) -> PluginToolCall {
    PluginToolCall {
        context: crate::plugin_runtime::types::PluginCallContext {
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

pub(super) fn contract_error(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_contract_mismatch", message)
}

pub(super) fn unavailable(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_unavailable", message)
}
