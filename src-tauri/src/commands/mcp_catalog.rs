//! Persistent managed MCP catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

use super::mcp_catalog_persistence::{parse_catalog, persist_catalog};

pub const MANAGED_MCP_CATALOG_KEY: &str = "managed_mcp.catalog.v1";
pub(crate) const MANAGED_MCP_CATALOG_VERSION: u32 = 2;

static MCP_OPERATION_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn persisted_entry_is_managed() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagedMcpCatalogEntry {
    pub spec: Value,
    pub enabled: bool,
    #[serde(default = "persisted_entry_is_managed")]
    pub managed: bool,
    #[serde(default)]
    pub managed_key: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub missing_config: Vec<String>,
    #[serde(default)]
    pub sources: BTreeMap<String, super::mcp_catalog_sources::ManagedMcpCatalogSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagedMcpCatalog {
    pub version: u32,
    pub servers: BTreeMap<String, ManagedMcpCatalogEntry>,
    #[serde(default)]
    pub tombstones: BTreeSet<String>,
}

impl ManagedMcpCatalog {
    fn from_legacy(servers: BTreeMap<String, Value>) -> Self {
        Self {
            version: MANAGED_MCP_CATALOG_VERSION,
            servers: servers
                .into_iter()
                .map(|(id, spec)| {
                    let managed_key = id.clone();
                    (
                        id,
                        ManagedMcpCatalogEntry {
                            spec,
                            enabled: false,
                            managed: false,
                            managed_key: Some(managed_key),
                            display_name: None,
                            description: None,
                            missing_config: Vec::new(),
                            sources: BTreeMap::new(),
                        },
                    )
                })
                .collect(),
            tombstones: BTreeSet::new(),
        }
    }

    fn merge_legacy(&mut self, servers: BTreeMap<String, Value>) -> bool {
        let mut changed = false;
        for (server_id, spec) in servers {
            if self.tombstones.contains(&server_id) || self.servers.contains_key(&server_id) {
                continue;
            }
            let managed_key = server_id.clone();
            self.servers.insert(
                server_id,
                ManagedMcpCatalogEntry {
                    spec,
                    enabled: false,
                    managed: false,
                    managed_key: Some(managed_key),
                    display_name: None,
                    description: None,
                    missing_config: Vec::new(),
                    sources: BTreeMap::new(),
                },
            );
            changed = true;
        }
        changed
    }
}

pub(crate) async fn lock_operation() -> tokio::sync::MutexGuard<'static, ()> {
    MCP_OPERATION_LOCK.lock().await
}

pub async fn load_or_import<F>(
    conn: &DatabaseConnection,
    import_legacy: F,
) -> Result<ManagedMcpCatalog, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    let _guard = lock_operation().await;
    load_or_import_unlocked(conn, import_legacy).await
}

pub async fn upsert_server<F>(
    conn: &DatabaseConnection,
    server_id: &str,
    spec: Value,
    import_legacy: F,
) -> Result<ManagedMcpCatalogEntry, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    let _guard = lock_operation().await;
    upsert_server_unlocked(conn, server_id, spec, import_legacy).await
}

pub(crate) async fn upsert_server_unlocked<F>(
    conn: &DatabaseConnection,
    server_id: &str,
    spec: Value,
    import_legacy: F,
) -> Result<ManagedMcpCatalogEntry, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    let mut catalog = load_or_import_unlocked(conn, import_legacy).await?;
    let entry = if let Some(existing) = catalog.servers.get_mut(server_id) {
        existing.spec = spec;
        existing.managed = true;
        let template = existing
            .sources
            .values()
            .next()
            .map(|source| source.template_spec.clone());
        existing.missing_config = template
            .map(|value| {
                super::mcp_catalog_sources::missing_template_config(&value, &existing.spec)
            })
            .unwrap_or_default();
        existing.clone()
    } else {
        ManagedMcpCatalogEntry {
            spec,
            enabled: false,
            managed: true,
            managed_key: Some(server_id.to_string()),
            display_name: None,
            description: None,
            missing_config: Vec::new(),
            sources: BTreeMap::new(),
        }
    };
    catalog.tombstones.remove(server_id);
    catalog.servers.insert(server_id.to_string(), entry.clone());
    persist_catalog(conn, &catalog).await?;
    Ok(entry)
}

pub async fn set_server_enabled<F>(
    conn: &DatabaseConnection,
    server_id: &str,
    enabled: bool,
    import_legacy: F,
) -> Result<Option<ManagedMcpCatalogEntry>, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    let _guard = lock_operation().await;
    set_server_enabled_unlocked(conn, server_id, enabled, import_legacy).await
}

pub(crate) async fn set_server_enabled_unlocked<F>(
    conn: &DatabaseConnection,
    server_id: &str,
    enabled: bool,
    import_legacy: F,
) -> Result<Option<ManagedMcpCatalogEntry>, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    let mut catalog = load_or_import_unlocked(conn, import_legacy).await?;
    let Some(entry) = catalog.servers.get_mut(server_id) else {
        return Ok(None);
    };
    if enabled && !entry.missing_config.is_empty() {
        return Err(AppCommandError::invalid_input(format!(
            "Connector '{server_id}' still requires configuration"
        )));
    }
    entry.enabled = enabled;
    entry.managed = true;
    let updated = entry.clone();
    catalog.tombstones.remove(server_id);
    persist_catalog(conn, &catalog).await?;
    Ok(Some(updated))
}

pub async fn remove_server<F>(
    conn: &DatabaseConnection,
    server_id: &str,
    import_legacy: F,
) -> Result<Option<ManagedMcpCatalogEntry>, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    let _guard = lock_operation().await;
    remove_server_unlocked(conn, server_id, import_legacy).await
}

pub(crate) async fn remove_server_unlocked<F>(
    conn: &DatabaseConnection,
    server_id: &str,
    import_legacy: F,
) -> Result<Option<ManagedMcpCatalogEntry>, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    let mut catalog = load_or_import_unlocked(conn, import_legacy).await?;
    if catalog
        .servers
        .get(server_id)
        .is_some_and(|entry| !entry.sources.is_empty())
    {
        return Err(AppCommandError::invalid_input(
            "Plugin connectors must be removed by uninstalling their source plugin",
        ));
    }
    let removed = catalog.servers.remove(server_id);
    if removed.is_some() {
        catalog.tombstones.insert(server_id.to_string());
        persist_catalog(conn, &catalog).await?;
    }
    Ok(removed)
}

pub(crate) async fn load_or_import_unlocked<F>(
    conn: &DatabaseConnection,
    import_legacy: F,
) -> Result<ManagedMcpCatalog, AppCommandError>
where
    F: FnOnce() -> Result<BTreeMap<String, Value>, AppCommandError>,
{
    if let Some(raw) = app_metadata_service::get_value(conn, MANAGED_MCP_CATALOG_KEY)
        .await
        .map_err(AppCommandError::db)?
    {
        let (mut catalog, upgraded) = parse_catalog(&raw)?;
        let merged_legacy = catalog.merge_legacy(import_legacy()?);
        if upgraded || merged_legacy {
            persist_catalog(conn, &catalog).await?;
        }
        return Ok(catalog);
    }

    let catalog = ManagedMcpCatalog::from_legacy(import_legacy()?);
    persist_catalog(conn, &catalog).await?;
    Ok(catalog)
}

pub(crate) async fn replace_catalog_unlocked(
    conn: &DatabaseConnection,
    catalog: &ManagedMcpCatalog,
) -> Result<(), AppCommandError> {
    persist_catalog(conn, catalog).await
}
