use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::app_error::AppCommandError;
use crate::commands::mcp_catalog_sources::{
    missing_template_config, ManagedMcpCatalogSource, PluginCatalogMutation,
    PluginConnectorRegistration,
};
use crate::db::service::plugin_installation_service::{
    PluginComponentInput, PluginInstallationInput, PluginInstallationRecord,
};
use crate::models::AgentType;

use super::plugin_install_context::PreparedPluginInstall;
use super::plugin_install_runtime_state::{
    permissions_digest, runtime_state_from_record, runtime_state_input, trust_state,
};
use super::plugin_storage::{PluginStorageRemoval, PluginStorageTransaction};
use super::plugin_types::SkillPluginManifest;

pub(super) fn connector_registrations(
    plugin: &PreparedPluginInstall,
    owner_id: &str,
) -> Result<Vec<PluginConnectorRegistration>, AppCommandError> {
    if plugin.plugin.manifest.schema_version >= 2 {
        return Ok(Vec::new());
    }
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

pub(super) fn needs_legacy_catalog(
    plugin: &PreparedPluginInstall,
    previous: Option<&PluginInstallationRecord>,
) -> bool {
    plugin.plugin.manifest.schema_version < 2
        || previous.is_some_and(installation_uses_legacy_catalog)
}

pub(super) fn installation_uses_legacy_catalog(record: &PluginInstallationRecord) -> bool {
    serde_json::from_str::<SkillPluginManifest>(&record.installation.manifest_json)
        .map_or(true, |manifest| manifest.schema_version < 2)
}

pub(super) async fn lock_legacy_catalog(
    required: bool,
) -> Option<tokio::sync::MutexGuard<'static, ()>> {
    if required {
        Some(crate::commands::mcp_catalog::lock_operation().await)
    } else {
        None
    }
}

pub(super) async fn lock_legacy_catalog_for_plan(
    plugins: &[PreparedPluginInstall],
    previous: &[Option<PluginInstallationRecord>],
) -> Option<tokio::sync::MutexGuard<'static, ()>> {
    let required = plugins
        .iter()
        .zip(previous)
        .any(|(plugin, old)| needs_legacy_catalog(plugin, old.as_ref()));
    lock_legacy_catalog(required).await
}

pub(super) async fn replace_connectors(
    conn: &DatabaseConnection,
    owner_id: &str,
    registrations: Vec<PluginConnectorRegistration>,
) -> Result<PluginCatalogMutation, AppCommandError> {
    crate::commands::mcp_catalog_sources::replace_plugin_connectors_unlocked(
        conn,
        owner_id,
        registrations,
        crate::commands::mcp::scan_legacy_server_specs,
    )
    .await
}

pub(super) async fn require_registry_state(
    conn: &DatabaseConnection,
    market_skill_id: i64,
    expected_present: bool,
) -> Result<(), AppCommandError> {
    let result = crate::plugin_runtime::registry::reconcile_global(conn).await;
    let actual = crate::plugin_runtime::registry::market_skill_state_global(market_skill_id);
    let error = match (result, actual) {
        (Ok(_), None) if !expected_present => return Ok(()),
        (Ok(_), Some((true, true))) if expected_present => return Ok(()),
        (Ok(_), Some((false, _))) if !expected_present => return Ok(()),
        (Ok(_), _) => AppCommandError::task_execution_failed(
            "Plugin registry did not publish the committed state",
        ),
        (Err(error), _) => error,
    };
    if expected_present {
        if let Err(db_error) =
            crate::db::service::plugin_installation_service::mark_repair_required(
                conn,
                market_skill_id,
            )
            .await
        {
            tracing::error!(
                market_skill_id,
                error = %db_error,
                "[plugin-registry] failed to persist repair state"
            );
        }
    }
    Err(error)
}

pub(super) async fn stage_plugin_removal(
    conn: &DatabaseConnection,
    record: &PluginInstallationRecord,
    market_skill_id: i64,
) -> Result<PluginStorageRemoval, AppCommandError> {
    if !crate::plugin_runtime::registry::suspend_global(&record.installation.slug) {
        return Err(AppCommandError::task_execution_failed(
            "Plugin registry could not suspend the plugin",
        ));
    }
    match PluginStorageRemoval::stage(&record.installation.slug) {
        Ok(removal) => Ok(removal),
        Err(error) => {
            let _ = require_registry_state(conn, market_skill_id, true).await;
            Err(error)
        }
    }
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
    let permissions_digest = permissions_digest(&plugin.plugin.manifest)?;
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
        schema_version: plugin.plugin.manifest.schema_version as i32,
        publisher_id: plugin.publisher_id.clone(),
        trust_state: trust_state(plugin).to_string(),
        artifact_signature_key_id: plugin.signature_key_id.clone(),
        permissions_digest: permissions_digest.clone(),
        reconcile_state: "ready".to_string(),
        components,
        runtime_state: runtime_state_input(plugin, permissions_digest)?,
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
        schema_version: value.schema_version,
        publisher_id: value.publisher_id.clone(),
        trust_state: value.trust_state.clone(),
        artifact_signature_key_id: value.artifact_signature_key_id.clone(),
        permissions_digest: value.permissions_digest.clone(),
        reconcile_state: value.reconcile_state.clone(),
        components: record
            .components
            .iter()
            .map(|component| PluginComponentInput {
                component_type: component.component_type.clone(),
                component_key: component.component_key.clone(),
                managed_resource_key: component.managed_resource_key.clone(),
                relative_path: component.relative_path.clone(),
                server_key: component.server_key.clone(),
                component_config_json: component.component_config_json.clone(),
            })
            .collect(),
        runtime_state: runtime_state_from_record(record),
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
        component_config_json: value
            .config
            .as_ref()
            .map(Value::to_string)
            .unwrap_or_default(),
    }
}

fn plugin_status(_plugin: &PreparedPluginInstall) -> String {
    "installed".to_string()
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
