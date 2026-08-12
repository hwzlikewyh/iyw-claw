use serde_json::Value;

use crate::app_error::AppCommandError;
use crate::commands::mcp_catalog_sources::{
    missing_template_config, ManagedMcpCatalogSource, PluginConnectorRegistration,
};
use crate::db::service::plugin_installation_service::{
    PluginComponentInput, PluginInstallationInput, PluginInstallationRecord,
};
use crate::models::AgentType;

use super::plugin_install_context::PreparedPluginInstall;
use super::plugin_storage::PluginStorageTransaction;

pub(super) fn connector_registrations(
    plugin: &PreparedPluginInstall,
    owner_id: &str,
) -> Result<Vec<PluginConnectorRegistration>, AppCommandError> {
    let mut result = Vec::new();
    for component in &plugin.plugin.manifest.components {
        if component.kind != "connector" {
            continue;
        }
        let raw = plugin
            .plugin
            .connector_specs
            .get(&component.server_key)
            .ok_or_else(|| AppCommandError::configuration_invalid("Plugin connector is missing"))?;
        let spec = crate::commands::mcp::canonicalize_plugin_spec(raw)?;
        result.push(PluginConnectorRegistration {
            server_id: component.server_key.clone(),
            display_name: connector_label(&spec, &component.key),
            description: connector_description(&spec),
            missing_config: missing_template_config(&spec, &spec),
            source: ManagedMcpCatalogSource {
                kind: "plugin".to_string(),
                owner_id: owner_id.to_string(),
                owner_name: plugin.slug.clone(),
                version: plugin.version.clone(),
                component_key: component.key.clone(),
                required_skill_keys: required_skill_keys(plugin, &component.key),
                template_spec: spec,
            },
        });
    }
    Ok(result)
}

fn required_skill_keys(plugin: &PreparedPluginInstall, connector_key: &str) -> Vec<String> {
    plugin
        .plugin
        .manifest
        .bindings
        .iter()
        .filter(|binding| binding.connector_key == connector_key)
        .map(|binding| binding.skill_key.clone())
        .collect()
}

pub(super) fn installation_input(
    plugin: &PreparedPluginInstall,
    storage: &PluginStorageTransaction,
    agent_types: &[AgentType],
) -> Result<PluginInstallationInput, AppCommandError> {
    let manifest_json =
        serde_json::to_string(&plugin.plugin.manifest).map_err(serialization_error)?;
    let agent_types_json = serde_json::to_string(agent_types).map_err(serialization_error)?;
    let components = plugin
        .plugin
        .manifest
        .components
        .iter()
        .map(component_input)
        .collect();
    Ok(PluginInstallationInput {
        market_skill_id: plugin.market_skill_id,
        slug: plugin.slug.clone(),
        version: plugin.version.clone(),
        install_root: storage.version_dir().to_string_lossy().to_string(),
        status: plugin_status(plugin),
        content_sha256: plugin.package.content_sha256.clone(),
        object_sha256: plugin.object_sha256.clone(),
        agent_types_json,
        manifest_json,
        components,
    })
}

pub(super) fn record_input(record: &PluginInstallationRecord) -> PluginInstallationInput {
    let value = &record.installation;
    PluginInstallationInput {
        market_skill_id: value.market_skill_id,
        slug: value.slug.clone(),
        version: value.version.clone(),
        install_root: value.install_root.clone(),
        status: value.status.clone(),
        content_sha256: value.content_sha256.clone(),
        object_sha256: value.object_sha256.clone(),
        agent_types_json: value.agent_types_json.clone(),
        manifest_json: value.manifest_json.clone(),
        components: record
            .components
            .iter()
            .map(|component| PluginComponentInput {
                component_type: component.component_type.clone(),
                component_key: component.component_key.clone(),
                managed_resource_key: component.managed_resource_key.clone(),
                relative_path: component.relative_path.clone(),
                server_key: component.server_key.clone(),
            })
            .collect(),
    }
}

pub(super) fn plugin_owner_id(market_skill_id: i64) -> String {
    format!("market-plugin:{market_skill_id}")
}

fn component_input(value: &super::plugin_types::SkillPluginComponent) -> PluginComponentInput {
    PluginComponentInput {
        component_type: value.kind.clone(),
        component_key: value.key.clone(),
        managed_resource_key: if value.kind == "skill" {
            value.key.clone()
        } else {
            value.server_key.clone()
        },
        relative_path: (!value.path.is_empty()).then(|| value.path.clone()),
        server_key: (!value.server_key.is_empty()).then(|| value.server_key.clone()),
    }
}

fn plugin_status(plugin: &PreparedPluginInstall) -> String {
    if plugin.plugin.manifest.bindings.is_empty() {
        "installed"
    } else {
        "degraded"
    }
    .to_string()
}

fn connector_label(spec: &Value, fallback: &str) -> String {
    spec.get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn connector_description(spec: &Value) -> String {
    spec.get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn serialization_error(error: serde_json::Error) -> AppCommandError {
    AppCommandError::configuration_invalid("Failed to serialize plugin installation state")
        .with_detail(error.to_string())
}
