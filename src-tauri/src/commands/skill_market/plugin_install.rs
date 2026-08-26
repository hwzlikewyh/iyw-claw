use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillInstall;
use crate::commands::mcp_catalog_sources::PluginCatalogMutation;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};
use crate::models::AgentType;

use super::plugin_install_context::{MarketInstallPlanExecution, PreparedPluginInstall};
use super::plugin_install_data::{
    connector_registrations, installation_input, installation_uses_legacy_catalog,
    lock_legacy_catalog, lock_legacy_catalog_for_plan, needs_legacy_catalog, plugin_owner_id,
    replace_connectors,
};
use super::plugin_install_rollback::{rollback_plugin_state, rollback_storage, rollback_uninstall};
use super::plugin_storage::{PluginStorageRemoval, PluginStorageTransaction};

struct PluginCommitContext<'a> {
    conn: &'a DatabaseConnection,
    agent_types: &'a [AgentType],
    plugins: &'a [PreparedPluginInstall],
    storage: &'a [PluginStorageTransaction],
    previous: &'a [Option<PluginInstallationRecord>],
}

#[derive(Default)]
struct PluginCommitProgress {
    catalog_mutations: Vec<PluginCatalogMutation>,
    saved_records: Vec<(i64, Option<PluginInstallationRecord>)>,
}

pub(super) async fn install_market_plan(
    input: MarketInstallPlanExecution<'_>,
) -> Result<(), AppCommandError> {
    let MarketInstallPlanExecution {
        conn,
        agent_types,
        root_skill_id,
        mut skill_installs,
        plugins,
    } = input;
    log_install_start(root_skill_id, plugins.len(), skill_installs.len());
    for plugin in &plugins {
        skill_installs.extend(plugin_skill_installs(plugin)?);
    }
    if plugins.is_empty() {
        return install_skills(agent_types, root_skill_id, skill_installs);
    }
    let (mut storage, previous) = stage_storage_with_previous(conn, &plugins).await?;
    let _catalog_guard = lock_legacy_catalog_for_plan(&plugins, &previous).await;
    if let Err(error) = commit_storage(&mut storage, &plugins) {
        rollback_storage(&mut storage);
        return Err(error);
    }
    let mut progress = PluginCommitProgress::default();
    let context = PluginCommitContext {
        conn,
        agent_types,
        plugins: &plugins,
        storage: &storage,
        previous: &previous,
    };
    let result = commit_plugin_state(&context, &mut progress)
        .await
        .and_then(|_| install_skills(agent_types, root_skill_id, skill_installs));
    drop(context);
    if let Err(error) = result {
        tracing::error!(root_skill_id, error = %error, "[plugin-install] install failed; rolling back plugin state");
        rollback_plugin_state(
            conn,
            &mut storage,
            &progress.saved_records,
            &progress.catalog_mutations,
        )
        .await;
        return Err(error);
    }
    finish_storage(storage, root_skill_id);
    Ok(())
}

fn log_install_start(root_skill_id: i64, plugin_count: usize, skill_count: usize) {
    tracing::info!(
        root_skill_id,
        plugin_count,
        standalone_skill_count = skill_count,
        "[plugin-install] starting atomic install plan"
    );
}

fn finish_storage(storage: Vec<PluginStorageTransaction>, root_skill_id: i64) {
    for transaction in storage {
        transaction.finish();
    }
    tracing::info!(root_skill_id, "[plugin-install] atomic install completed");
}

async fn stage_storage_with_previous(
    conn: &DatabaseConnection,
    plugins: &[PreparedPluginInstall],
) -> Result<
    (
        Vec<PluginStorageTransaction>,
        Vec<Option<PluginInstallationRecord>>,
    ),
    AppCommandError,
> {
    let mut storage = stage_plugin_storage(plugins)?;
    match load_previous_records(conn, plugins).await {
        Ok(previous) => Ok((storage, previous)),
        Err(error) => {
            rollback_storage(&mut storage);
            Err(error)
        }
    }
}

pub(super) async fn uninstall_plugin(
    conn: &DatabaseConnection,
    market_skill_id: i64,
) -> Result<bool, AppCommandError> {
    let Some(previous) =
        plugin_installation_service::find_by_market_skill_id(conn, market_skill_id)
            .await
            .map_err(AppCommandError::db)?
    else {
        return Ok(false);
    };
    tracing::info!(
        market_skill_id,
        slug = %previous.installation.slug,
        version = %previous.installation.version,
        "[plugin-install] starting plugin uninstall"
    );
    let mut removal = PluginStorageRemoval::stage(&previous.installation.slug)?;
    let owner_id = plugin_owner_id(market_skill_id);
    let uses_catalog = installation_uses_legacy_catalog(&previous);
    let _catalog_guard = lock_legacy_catalog(uses_catalog).await;
    let mutation = if uses_catalog {
        match replace_connectors(conn, &owner_id, Vec::new()).await {
            Ok(value) => Some(value),
            Err(error) => {
                removal.rollback();
                return Err(error);
            }
        }
    } else {
        None
    };
    if let Err(error) = delete_plugin_state(conn, market_skill_id, mutation.as_ref()).await {
        rollback_uninstall(conn, &previous, mutation.as_ref(), &mut removal).await;
        return Err(error);
    }
    if let Err(error) = crate::commands::acp::uninstall_market_skill_by_id(market_skill_id) {
        rollback_uninstall(conn, &previous, mutation.as_ref(), &mut removal).await;
        return Err(map_skill_error(error));
    }
    removal.finish();
    tracing::info!(
        market_skill_id,
        "[plugin-install] plugin uninstall completed"
    );
    Ok(true)
}

fn plugin_skill_installs(
    plugin: &PreparedPluginInstall,
) -> Result<Vec<MarketSkillInstall>, AppCommandError> {
    let mut installs = Vec::new();
    for component in &plugin.plugin.manifest.components {
        if component.kind != "skill" {
            continue;
        }
        let package = plugin.package.skill_component(&component.path)?;
        let mut marker = plugin.marker.clone();
        marker.slug = component.key.clone();
        marker.content_sha256 = package.content_sha256.clone();
        marker.plugin_slug = Some(plugin.slug.clone());
        marker.plugin_component_key = Some(component.key.clone());
        installs.push(MarketSkillInstall { marker, package });
    }
    Ok(installs)
}

fn stage_plugin_storage(
    plugins: &[PreparedPluginInstall],
) -> Result<Vec<PluginStorageTransaction>, AppCommandError> {
    let mut result = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        match PluginStorageTransaction::stage(&plugin.package, &plugin.slug, &plugin.version) {
            Ok(transaction) => result.push(transaction),
            Err(error) => {
                rollback_storage(&mut result);
                return Err(error);
            }
        }
    }
    Ok(result)
}

fn commit_storage(
    storage: &mut [PluginStorageTransaction],
    plugins: &[PreparedPluginInstall],
) -> Result<(), AppCommandError> {
    for (transaction, plugin) in storage.iter_mut().zip(plugins) {
        transaction.commit(
            &plugin.version,
            &plugin.package.content_sha256,
            &plugin.object_sha256,
        )?;
    }
    Ok(())
}

async fn load_previous_records(
    conn: &DatabaseConnection,
    plugins: &[PreparedPluginInstall],
) -> Result<Vec<Option<PluginInstallationRecord>>, AppCommandError> {
    let mut result = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        result.push(
            plugin_installation_service::find_by_market_skill_id(conn, plugin.market_skill_id)
                .await
                .map_err(AppCommandError::db)?,
        );
    }
    Ok(result)
}

async fn commit_plugin_state(
    context: &PluginCommitContext<'_>,
    progress: &mut PluginCommitProgress,
) -> Result<(), AppCommandError> {
    for ((plugin, transaction), old) in context
        .plugins
        .iter()
        .zip(context.storage)
        .zip(context.previous)
    {
        if needs_legacy_catalog(plugin, old.as_ref()) {
            let owner_id = plugin_owner_id(plugin.market_skill_id);
            let registrations = connector_registrations(plugin, &owner_id)?;
            let mutation = replace_connectors(context.conn, &owner_id, registrations).await?;
            let requires_reconcile = mutation.requires_reconcile;
            progress.catalog_mutations.push(mutation);
            if requires_reconcile {
                crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(context.conn).await?;
            }
        }
        progress
            .saved_records
            .push((plugin.market_skill_id, old.clone()));
        let input = installation_input(plugin, transaction, context.agent_types)?;
        plugin_installation_service::replace(context.conn, input)
            .await
            .map_err(AppCommandError::db)?;
        tracing::info!(
            market_skill_id = plugin.market_skill_id,
            slug = %plugin.slug,
            version = %plugin.version,
            "[plugin-install] plugin catalog and ownership persisted"
        );
    }
    Ok(())
}

fn install_skills(
    agent_types: &[AgentType],
    root_skill_id: i64,
    installs: Vec<MarketSkillInstall>,
) -> Result<(), AppCommandError> {
    if installs.is_empty() {
        return Ok(());
    }
    crate::commands::acp::install_market_skills(agent_types, root_skill_id, installs)
        .map(|_| ())
        .map_err(map_skill_error)
}

async fn delete_plugin_state(
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
    Ok(())
}

fn map_skill_error(error: crate::acp::error::AcpError) -> AppCommandError {
    super::install::map_local_install_error(error)
}
