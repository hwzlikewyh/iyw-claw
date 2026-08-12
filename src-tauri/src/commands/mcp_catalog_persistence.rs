use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

use super::mcp_catalog::{ManagedMcpCatalog, MANAGED_MCP_CATALOG_KEY, MANAGED_MCP_CATALOG_VERSION};

pub(crate) fn parse_catalog(raw: &str) -> Result<(ManagedMcpCatalog, bool), AppCommandError> {
    let catalog = serde_json::from_str::<ManagedMcpCatalog>(raw).map_err(|error| {
        AppCommandError::configuration_invalid("Managed MCP catalog is invalid")
            .with_detail(error.to_string())
    })?;
    if !matches!(catalog.version, 1 | MANAGED_MCP_CATALOG_VERSION) {
        return Err(AppCommandError::configuration_invalid(format!(
            "Unsupported managed MCP catalog version: {}",
            catalog.version
        )));
    }
    let mut upgraded = catalog.version != MANAGED_MCP_CATALOG_VERSION;
    let mut catalog = ManagedMcpCatalog {
        version: MANAGED_MCP_CATALOG_VERSION,
        ..catalog
    };
    for (server_id, entry) in &mut catalog.servers {
        if entry.managed_key.is_none() {
            entry.managed_key = Some(server_id.clone());
            upgraded = true;
        }
    }
    Ok((catalog, upgraded))
}

pub(crate) async fn persist_catalog(
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
