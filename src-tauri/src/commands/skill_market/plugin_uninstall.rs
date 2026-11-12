use super::plugin_install_data::{
    installation_uses_legacy_catalog, lock_legacy_catalog, plugin_owner_id, replace_connectors,
    require_registry_state, stage_plugin_removal,
};
use super::plugin_install_rollback::rollback_uninstall;
use crate::app_error::AppCommandError;
use crate::commands::mcp_catalog_sources::PluginCatalogMutation;
use crate::db::service::plugin_installation_service;
use sea_orm::DatabaseConnection;

pub(super) async fn uninstall_plugin(
    conn: &DatabaseConnection,
    market_skill_id: i64,
    policy: super::RemovalPolicy,
) -> Result<bool, AppCommandError> {
    let Some(previous) =
        plugin_installation_service::find_by_market_skill_id(conn, market_skill_id)
            .await
            .map_err(AppCommandError::db)?
    else {
        return Ok(false);
    };
    if matches!(policy, super::RemovalPolicy::User) {
        let issues = super::plugin_constraints::removal_issues(conn, market_skill_id).await?;
        if !issues.is_empty() {
            return Err(AppCommandError::configuration_invalid(issues.join("；")));
        }
    }
    tracing::info!(
        market_skill_id,
        slug = %previous.installation.slug,
        version = %previous.installation.version,
        "[plugin-install] starting plugin uninstall"
    );
    let mut removal = stage_plugin_removal(conn, &previous, market_skill_id).await?;
    let uses_catalog = installation_uses_legacy_catalog(&previous);
    let _catalog_guard = lock_legacy_catalog(uses_catalog).await;
    let mutation = remove_catalog(conn, &previous, &mut removal).await?;
    if let Err(error) = delete_plugin_state(conn, market_skill_id, mutation.as_ref()).await {
        rollback_uninstall(conn, &previous, (mutation.as_ref(), &mut removal)).await?;
        return Err(error);
    }
    if let Err(error) = crate::commands::acp::uninstall_market_skill_by_id(market_skill_id) {
        rollback_uninstall(conn, &previous, (mutation.as_ref(), &mut removal)).await?;
        return Err(map_skill_error(error));
    }
    removal.finish();
    tracing::info!(
        market_skill_id,
        "[plugin-install] plugin uninstall completed"
    );
    Ok(true)
}

async fn remove_catalog(
    conn: &DatabaseConnection,
    record: &plugin_installation_service::PluginInstallationRecord,
    removal: &mut super::plugin_storage::PluginStorageRemoval,
) -> Result<Option<PluginCatalogMutation>, AppCommandError> {
    if !installation_uses_legacy_catalog(record) {
        return Ok(None);
    }
    let id = record.installation.market_skill_id;
    match replace_connectors(conn, &plugin_owner_id(id), Vec::new()).await {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            rollback_uninstall(conn, record, (None, removal)).await?;
            Err(error)
        }
    }
}

pub(super) async fn delete_plugin_state(
    conn: &DatabaseConnection,
    market_skill_id: i64,
    mutation: Option<&PluginCatalogMutation>,
) -> Result<(), AppCommandError> {
    plugin_installation_service::delete_by_market_skill_id(conn, market_skill_id)
        .await
        .map_err(AppCommandError::db)?;
    if mutation.is_some_and(|value| value.requires_reconcile) {
        crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await?;
    }
    require_registry_state(conn, market_skill_id, false).await?;
    Ok(())
}

fn map_skill_error(error: crate::acp::error::AcpError) -> AppCommandError {
    super::install::map_local_install_error(error)
}
