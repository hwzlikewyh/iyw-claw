//! Persistent managed MCP catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

pub const MANAGED_MCP_CATALOG_KEY: &str = "managed_mcp.catalog.v1";
const MANAGED_MCP_CATALOG_VERSION: u32 = 1;

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
                    (
                        id,
                        ManagedMcpCatalogEntry {
                            spec,
                            enabled: false,
                            managed: false,
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
            self.servers.insert(
                server_id,
                ManagedMcpCatalogEntry {
                    spec,
                    enabled: false,
                    managed: false,
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
    let enabled = catalog
        .servers
        .get(server_id)
        .filter(|entry| entry.managed)
        .map(|entry| entry.enabled)
        .unwrap_or(true);
    let entry = ManagedMcpCatalogEntry {
        spec,
        enabled,
        managed: true,
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
        let mut catalog = parse_catalog(&raw)?;
        if catalog.merge_legacy(import_legacy()?) {
            persist_catalog(conn, &catalog).await?;
        }
        return Ok(catalog);
    }

    let catalog = ManagedMcpCatalog::from_legacy(import_legacy()?);
    persist_catalog(conn, &catalog).await?;
    Ok(catalog)
}

fn parse_catalog(raw: &str) -> Result<ManagedMcpCatalog, AppCommandError> {
    let catalog = serde_json::from_str::<ManagedMcpCatalog>(raw).map_err(|error| {
        AppCommandError::configuration_invalid("Managed MCP catalog is invalid")
            .with_detail(error.to_string())
    })?;
    if catalog.version != MANAGED_MCP_CATALOG_VERSION {
        return Err(AppCommandError::configuration_invalid(format!(
            "Unsupported managed MCP catalog version: {}",
            catalog.version
        )));
    }
    Ok(catalog)
}

async fn persist_catalog(
    conn: &DatabaseConnection,
    catalog: &ManagedMcpCatalog,
) -> Result<(), AppCommandError> {
    let raw = serde_json::to_string(catalog).map_err(|error| {
        AppCommandError::configuration_invalid("Failed to serialize managed MCP catalog")
            .with_detail(error.to_string())
    })?;
    app_metadata_service::upsert_value(conn, MANAGED_MCP_CATALOG_KEY, &raw)
        .await
        .map_err(AppCommandError::db)
}
