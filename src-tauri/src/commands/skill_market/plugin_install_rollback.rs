use sea_orm::DatabaseConnection;

use crate::commands::mcp_catalog_sources::PluginCatalogMutation;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};

use super::plugin_install_data::record_input;
use super::plugin_storage::{PluginStorageRemoval, PluginStorageTransaction};

pub(super) async fn rollback_plugin_state(
    conn: &DatabaseConnection,
    storage: &mut [PluginStorageTransaction],
    records: &[(i64, Option<PluginInstallationRecord>)],
    mutations: &[PluginCatalogMutation],
) {
    for (market_skill_id, record) in records.iter().rev() {
        restore_record(conn, *market_skill_id, record.as_ref()).await;
    }
    for mutation in mutations.iter().rev() {
        if let Err(error) =
            crate::commands::mcp_catalog_sources::restore_catalog_unlocked(conn, mutation).await
        {
            tracing::error!(error = %error, "[plugin-install] catalog rollback failed");
        }
    }
    rollback_storage(storage);
    if !mutations.is_empty() {
        let _ = crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await;
    }
}

pub(super) async fn rollback_uninstall(
    conn: &DatabaseConnection,
    record: &PluginInstallationRecord,
    mutation: Option<&PluginCatalogMutation>,
    removal: &mut PluginStorageRemoval,
) {
    restore_record(conn, record.installation.market_skill_id, Some(record)).await;
    if let Some(value) = mutation {
        let _ = crate::commands::mcp_catalog_sources::restore_catalog_unlocked(conn, value).await;
    }
    removal.rollback();
    if mutation.is_some() {
        let _ = crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await;
    }
}

async fn restore_record(
    conn: &DatabaseConnection,
    market_skill_id: i64,
    record: Option<&PluginInstallationRecord>,
) {
    let result = match record {
        Some(value) => plugin_installation_service::replace(conn, record_input(value))
            .await
            .map(|_| ()),
        None => plugin_installation_service::delete_by_market_skill_id(conn, market_skill_id)
            .await
            .map(|_| ()),
    };
    if let Err(error) = result {
        tracing::error!(market_skill_id, error = %error, "[plugin-install] database rollback failed");
    }
}

pub(super) fn rollback_storage(storage: &mut [PluginStorageTransaction]) {
    for transaction in storage.iter_mut().rev() {
        transaction.rollback();
    }
}
