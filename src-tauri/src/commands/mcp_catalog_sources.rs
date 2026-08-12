use std::collections::{BTreeMap, BTreeSet};

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app_error::AppCommandError;

use super::mcp_catalog::{self, ManagedMcpCatalog, ManagedMcpCatalogEntry};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedMcpCatalogSource {
    pub kind: String,
    pub owner_id: String,
    pub owner_name: String,
    pub version: String,
    pub component_key: String,
    #[serde(default)]
    pub required_skill_keys: Vec<String>,
    pub template_spec: Value,
}

#[derive(Debug, Clone)]
pub struct PluginConnectorRegistration {
    pub server_id: String,
    pub display_name: String,
    pub description: String,
    pub missing_config: Vec<String>,
    pub source: ManagedMcpCatalogSource,
}

pub struct PluginCatalogMutation {
    pub previous: ManagedMcpCatalog,
    pub connector_ids: BTreeSet<String>,
    pub requires_reconcile: bool,
}

pub(crate) async fn replace_plugin_connectors_unlocked<F>(
    conn: &DatabaseConnection,
    owner_id: &str,
    registrations: Vec<PluginConnectorRegistration>,
    import_legacy: F,
) -> Result<PluginCatalogMutation, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    let mut catalog = mcp_catalog::load_or_import_unlocked(conn, import_legacy).await?;
    let previous = catalog.clone();
    let removed = remove_owner_sources(&mut catalog, owner_id);
    let connector_ids = registrations
        .iter()
        .map(|value| value.server_id.clone())
        .collect();
    for registration in registrations {
        register_connector(&mut catalog, registration, &removed)?;
    }
    let deleted_enabled = remove_orphaned_plugin_entries(&mut catalog, &previous, &removed);
    mcp_catalog::replace_catalog_unlocked(conn, &catalog).await?;
    Ok(PluginCatalogMutation {
        previous,
        connector_ids,
        requires_reconcile: deleted_enabled,
    })
}

pub(crate) async fn restore_catalog_unlocked(
    conn: &DatabaseConnection,
    mutation: &PluginCatalogMutation,
) -> Result<(), AppCommandError> {
    mcp_catalog::replace_catalog_unlocked(conn, &mutation.previous).await
}

fn remove_owner_sources(catalog: &mut ManagedMcpCatalog, owner_id: &str) -> BTreeSet<String> {
    let mut affected = BTreeSet::new();
    for (server_id, entry) in &mut catalog.servers {
        let before = entry.sources.len();
        entry
            .sources
            .retain(|_, source| source.owner_id != owner_id);
        if entry.sources.len() != before {
            affected.insert(server_id.clone());
        }
    }
    affected
}

fn register_connector(
    catalog: &mut ManagedMcpCatalog,
    registration: PluginConnectorRegistration,
    replaced: &BTreeSet<String>,
) -> Result<(), AppCommandError> {
    catalog.tombstones.remove(&registration.server_id);
    let source_id = format!(
        "{}:{}",
        registration.source.owner_id, registration.source.component_key
    );
    if let Some(entry) = catalog.servers.get_mut(&registration.server_id) {
        ensure_shareable(
            entry,
            &registration,
            replaced.contains(&registration.server_id),
        )?;
        entry.missing_config =
            missing_template_config(&registration.source.template_spec, &entry.spec);
        entry.sources.insert(source_id, registration.source);
        entry.display_name = Some(registration.display_name);
        entry.description = Some(registration.description);
        return Ok(());
    }
    let source = registration.source;
    catalog.servers.insert(
        registration.server_id.clone(),
        ManagedMcpCatalogEntry {
            spec: source.template_spec.clone(),
            enabled: false,
            managed: true,
            managed_key: Some(registration.server_id),
            display_name: Some(registration.display_name),
            description: Some(registration.description),
            missing_config: registration.missing_config,
            sources: BTreeMap::from([(source_id, source)]),
        },
    );
    Ok(())
}

pub(crate) fn missing_template_config(template: &Value, current: &Value) -> Vec<String> {
    let mut result = BTreeSet::new();
    collect_missing_config(template, current, &mut result);
    result.into_iter().collect()
}

fn collect_missing_config(template: &Value, current: &Value, result: &mut BTreeSet<String>) {
    match template {
        Value::String(text) => collect_string_config(text, current.as_str(), result),
        Value::Array(values) => {
            let current_values = current.as_array();
            for (index, value) in values.iter().enumerate() {
                let current = current_values
                    .and_then(|items| items.get(index))
                    .unwrap_or(&Value::Null);
                collect_missing_config(value, current, result);
            }
        }
        Value::Object(values) => {
            let current_values = current.as_object();
            for (key, value) in values {
                let current = current_values
                    .and_then(|items| items.get(key))
                    .unwrap_or(&Value::Null);
                collect_missing_config(value, current, result);
            }
        }
        _ => {}
    }
}

fn collect_string_config(template: &str, current: Option<&str>, result: &mut BTreeSet<String>) {
    let mut remaining = template;
    while let Some(start) = remaining.find("${") {
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find('}') else {
            return;
        };
        let name = &after_start[..end];
        let placeholder = &remaining[start..start + end + 3];
        if !name.is_empty()
            && current.map_or(true, |value| {
                value.trim().is_empty() || value.contains(placeholder)
            })
        {
            result.insert(name.to_string());
        }
        remaining = &after_start[end + 1..];
    }
}

fn ensure_shareable(
    entry: &ManagedMcpCatalogEntry,
    registration: &PluginConnectorRegistration,
    replacing_same_owner: bool,
) -> Result<(), AppCommandError> {
    if entry.sources.is_empty() && !replacing_same_owner {
        return Err(AppCommandError::already_exists(format!(
            "Connector '{}' is already owned by a local configuration",
            registration.server_id
        )));
    }
    if entry
        .sources
        .values()
        .any(|source| source.template_spec != registration.source.template_spec)
    {
        return Err(AppCommandError::already_exists(format!(
            "Connector '{}' has a conflicting plugin definition",
            registration.server_id
        )));
    }
    Ok(())
}

fn remove_orphaned_plugin_entries(
    catalog: &mut ManagedMcpCatalog,
    previous: &ManagedMcpCatalog,
    affected: &BTreeSet<String>,
) -> bool {
    let mut deleted_enabled = false;
    for server_id in affected {
        let orphaned = catalog
            .servers
            .get(server_id)
            .is_some_and(|entry| entry.sources.is_empty());
        if orphaned {
            deleted_enabled |= previous
                .servers
                .get(server_id)
                .is_some_and(|entry| entry.enabled);
            catalog.servers.remove(server_id);
            catalog.tombstones.insert(server_id.clone());
        }
    }
    deleted_enabled
}
