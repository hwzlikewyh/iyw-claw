use std::path::PathBuf;

use serde_json::Value;

use super::registry::PluginDescriptor;
use super::types::{
    ExpectedTool, PluginInvokeError, PluginToolCall, RuntimeKey, RuntimeLaunchSpec,
};

pub(super) fn launch_spec(
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

pub(super) fn component_config<'a>(
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
        .ok_or_else(|| contract_error(format!("Missing {kind} component {key}")))
}

fn expected_tools(
    plugin: &PluginDescriptor,
    connector_key: &str,
) -> Result<Vec<ExpectedTool>, PluginInvokeError> {
    plugin
        .manifest
        .components
        .iter()
        .filter(|component| component.kind == "capability")
        .map(|component| expected_tool(component, connector_key))
        .filter_map(Result::transpose)
        .collect()
}

fn expected_tool(
    component: &crate::commands::skill_market::SkillPluginComponent,
    connector_key: &str,
) -> Result<Option<ExpectedTool>, PluginInvokeError> {
    let config = component
        .config
        .as_ref()
        .ok_or_else(|| contract_error("Capability config is missing"))?;
    if config["connectorKey"].as_str() != Some(connector_key) {
        return Ok(None);
    }
    Ok(Some(ExpectedTool {
        name: config["toolName"].as_str().unwrap_or_default().to_string(),
        schema_path: config["schemaPath"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    }))
}

fn resource_uris(
    plugin: &PluginDescriptor,
    connector_key: &str,
) -> Result<Vec<String>, PluginInvokeError> {
    plugin
        .manifest
        .components
        .iter()
        .filter(|component| component.kind == "app")
        .map(|component| resource_uri(component, connector_key))
        .filter_map(Result::transpose)
        .collect()
}

fn resource_uri(
    component: &crate::commands::skill_market::SkillPluginComponent,
    connector_key: &str,
) -> Result<Option<String>, PluginInvokeError> {
    let config = component
        .config
        .as_ref()
        .ok_or_else(|| contract_error("App config is missing"))?;
    Ok(
        (config["connectorKey"].as_str() == Some(connector_key)).then(|| {
            config["resourceUri"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        }),
    )
}

fn unavailable(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_unavailable", message)
}

fn contract_error(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_contract_mismatch", message)
}
