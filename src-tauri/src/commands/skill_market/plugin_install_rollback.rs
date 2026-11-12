use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::mcp_catalog_sources::PluginCatalogMutation;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};

use super::plugin_install_data::record_input;
use super::plugin_storage::{PluginStorageRemoval, PluginStorageTransaction};

pub(super) async fn rollback_plugin_state(
    conn: &DatabaseConnection,
    storage: &mut [PluginStorageTransaction],
    snapshots: (
        &[(i64, Option<PluginInstallationRecord>)],
        &[PluginCatalogMutation],
    ),
) -> Result<(), AppCommandError> {
    let (records, mutations) = snapshots;
    for (id, _) in records {
        stop_committed(*id).await;
    }
    let mut failed = rollback_storage(storage).is_err();
    for (id, record) in records.iter().rev() {
        failed |= restore_record(conn, *id, record.as_ref()).await.is_err();
    }
    for mutation in mutations.iter().rev() {
        if let Err(error) =
            crate::commands::mcp_catalog_sources::restore_catalog_unlocked(conn, mutation).await
        {
            tracing::error!(error=%error,"[plugin-install] catalog rollback failed");
            failed = true;
        }
    }
    if !mutations.is_empty() {
        failed |= crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(conn)
            .await
            .is_err();
    }
    failed |= crate::plugin_runtime::registry::reconcile_global(conn)
        .await
        .is_err();
    if failed {
        mark_failed(conn, records).await;
        return Err(incomplete());
    }
    Ok(())
}

async fn stop_committed(id: i64) {
    let slug = crate::plugin_runtime::registry::global_snapshot().and_then(|snapshot| {
        snapshot
            .plugins
            .values()
            .find(|plugin| plugin.market_skill_id == id)
            .map(|plugin| plugin.slug.clone())
    });
    if let Some(slug) = slug {
        crate::plugin_runtime::registry::suspend_global(&slug);
        crate::plugin_runtime::global::stop_plugin(&slug).await;
    }
}

pub(super) async fn rollback_uninstall(
    conn: &DatabaseConnection,
    record: &PluginInstallationRecord,
    rollback: (Option<&PluginCatalogMutation>, &mut PluginStorageRemoval),
) -> Result<(), AppCommandError> {
    let (mutation, removal) = rollback;
    let mut failed = removal.rollback().is_err();
    failed |= restore_record(conn, record.installation.market_skill_id, Some(record))
        .await
        .is_err();
    if let Some(value) = mutation {
        failed |= crate::commands::mcp_catalog_sources::restore_catalog_unlocked(conn, value)
            .await
            .is_err();
        failed |= crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(conn)
            .await
            .is_err();
    }
    failed |= crate::plugin_runtime::registry::reconcile_global(conn)
        .await
        .is_err();
    if failed {
        mark_failed(
            conn,
            &[(record.installation.market_skill_id, Some(record.clone()))],
        )
        .await;
        return Err(incomplete());
    }
    Ok(())
}

async fn restore_record(
    conn: &DatabaseConnection,
    id: i64,
    record: Option<&PluginInstallationRecord>,
) -> Result<(), AppCommandError> {
    let result = match record {
        Some(value) => plugin_installation_service::restore(conn, record_input(value))
            .await
            .map(|_| ()),
        None => plugin_installation_service::delete_by_market_skill_id(conn, id)
            .await
            .map(|_| ()),
    };
    result.map_err(|error| {
        tracing::error!(market_skill_id=id,error=%error,"[plugin-install] database rollback failed");
        AppCommandError::db(error)
    })
}

pub(super) fn rollback_storage(
    storage: &mut [PluginStorageTransaction],
) -> Result<(), AppCommandError> {
    let mut failure = None;
    for transaction in storage.iter_mut().rev() {
        if let Err(error) = transaction.rollback() {
            tracing::error!(error=%error,detail=?error.detail,"[plugin-install] storage rollback failed; backup retained");
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

pub(super) async fn mark_failed(
    conn: &DatabaseConnection,
    records: &[(i64, Option<PluginInstallationRecord>)],
) {
    for (id, record) in records {
        let slug = record
            .as_ref()
            .map(|value| value.installation.slug.clone())
            .or_else(|| {
                crate::plugin_runtime::registry::global_snapshot().and_then(|snapshot| {
                    snapshot
                        .plugins
                        .values()
                        .find(|plugin| plugin.market_skill_id == *id)
                        .map(|plugin| plugin.slug.clone())
                })
            });
        if let Some(slug) = slug {
            crate::plugin_runtime::registry::suspend_global(&slug);
            crate::plugin_runtime::global::stop_plugin(&slug).await;
        }
        if let Err(error) = plugin_installation_service::mark_repair_required(conn, *id).await {
            tracing::error!(market_skill_id=id,error=%error,"[plugin-install] failed to persist incomplete rollback");
        }
    }
}

pub(super) fn incomplete() -> AppCommandError {
    AppCommandError::task_execution_failed(
        "安装未完成，部分旧记录或技能回退失败。已保留可恢复文件，请检查安装记录后重试",
    )
}
