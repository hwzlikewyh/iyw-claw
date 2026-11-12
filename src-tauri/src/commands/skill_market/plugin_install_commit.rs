use super::plugin_install::map_skill_error;
use super::plugin_install::{commit_storage, stage_storage_with_previous};
use super::plugin_install_context::{MarketInstallPlanExecution, PreparedPluginInstall};
use super::plugin_install_data::{
    connector_registrations, installation_input, lock_legacy_catalog_for_plan,
    needs_legacy_catalog, plugin_owner_id, replace_connectors, require_registry_state,
};
use super::plugin_install_rollback::rollback_plugin_state;
use super::plugin_storage::PluginStorageTransaction;
use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillInstall;
use crate::commands::mcp_catalog_sources::PluginCatalogMutation;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};
use crate::models::AgentType;
use sea_orm::DatabaseConnection;

struct PluginCommitContext<'a> {
    input: &'a MarketInstallPlanExecution<'a>,
    storage: &'a [PluginStorageTransaction],
    previous: &'a [Option<PluginInstallationRecord>],
}

#[derive(Default)]
struct PluginCommitProgress {
    catalog_mutations: Vec<PluginCatalogMutation>,
    saved_records: Vec<(i64, Option<PluginInstallationRecord>)>,
    skills: Option<crate::commands::acp::MarketSkillInstallTransaction>,
}

impl PluginCommitProgress {
    fn new(
        plugins: &[PreparedPluginInstall],
        previous: &[Option<PluginInstallationRecord>],
    ) -> Self {
        Self {
            saved_records: plugins
                .iter()
                .zip(previous)
                .map(|(plugin, old)| (plugin.market_skill_id, old.clone()))
                .collect(),
            ..Default::default()
        }
    }
}

pub(super) async fn commit_plugins(
    mut input: MarketInstallPlanExecution<'_>,
) -> Result<(), AppCommandError> {
    let skills = std::mem::take(&mut input.skill_installs);
    let (mut storage, previous) = stage_storage_with_previous(input.conn, &input.plugins).await?;
    let _catalog_guard = lock_legacy_catalog_for_plan(&input.plugins, &previous).await;
    let mut progress = PluginCommitProgress::new(&input.plugins, &previous);
    if let Err(error) = commit_storage(&mut storage, &input.plugins, &input.account_scope) {
        return rollback_failure(input.conn, (&mut storage, &mut progress), error).await;
    }
    let context = PluginCommitContext {
        input: &input,
        storage: &storage,
        previous: &previous,
    };
    let result = apply_commit(&context, &mut progress, skills).await;
    let result = result.and_then(|()| {
        storage
            .iter_mut()
            .try_for_each(PluginStorageTransaction::publish_receipt)
    });
    if let Err(error) = result {
        return rollback_failure(input.conn, (&mut storage, &mut progress), error).await;
    }
    if let Some(skills) = progress.skills.as_ref() {
        if let Err(error) = skills.migrate_plugin_skill_policies(input.conn).await {
            return rollback_failure(input.conn, (&mut storage, &mut progress), error).await;
        }
    }
    if let Some(skills) = progress.skills.take() {
        skills.finish();
    }
    finish_storage(storage, input.root_skill_id);
    stop_previous(&input.plugins, &previous).await;
    Ok(())
}

async fn apply_commit(
    context: &PluginCommitContext<'_>,
    progress: &mut PluginCommitProgress,
    skills: Vec<MarketSkillInstall>,
) -> Result<(), AppCommandError> {
    let input = context.input;
    commit_plugin_state(context, progress).await?;
    progress.skills = begin_skills(input.agent_types, input.root_skill_id, skills)?;
    if crate::commands::mcp_cli_config::account_scope(input.conn).await? != input.account_scope {
        return Err(AppCommandError::permission_denied(
            "安装期间账号或企业空间发生变化，插件安装已取消",
        ));
    }
    let versions = input
        .plugins
        .iter()
        .map(|plugin| (plugin.market_skill_id, plugin.version.clone()))
        .collect::<Vec<_>>();
    plugin_installation_service::finish_installations(input.conn, &versions)
        .await
        .map_err(AppCommandError::db)?;
    for plugin in &input.plugins {
        require_registry_state(input.conn, plugin.market_skill_id, true).await?;
    }
    Ok(())
}

async fn rollback_failure(
    conn: &DatabaseConnection,
    state: (&mut [PluginStorageTransaction], &mut PluginCommitProgress),
    error: AppCommandError,
) -> Result<(), AppCommandError> {
    let (storage, progress) = state;
    tracing::error!(error=%error,code=?error.code,"[plugin-install] 提交失败，开始回退插件及技能");
    let skills = progress
        .skills
        .take()
        .map(|skills| skills.rollback())
        .transpose();
    let plugin = rollback_plugin_state(
        conn,
        storage,
        (&progress.saved_records, &progress.catalog_mutations),
    )
    .await;
    if skills.is_err() || plugin.is_err() {
        super::plugin_install_rollback::mark_failed(conn, &progress.saved_records).await;
        return Err(super::plugin_install_rollback::incomplete());
    }
    Err(error)
}

async fn stop_previous(
    plugins: &[PreparedPluginInstall],
    previous: &[Option<PluginInstallationRecord>],
) {
    for (plugin, old) in plugins.iter().zip(previous) {
        if let Some(old) = old {
            crate::plugin_runtime::global::stop_plugin_version(
                &plugin.slug,
                Some(&old.installation.version),
            )
            .await;
        }
    }
}

async fn commit_plugin_state(
    context: &PluginCommitContext<'_>,
    progress: &mut PluginCommitProgress,
) -> Result<(), AppCommandError> {
    for ((plugin, transaction), old) in context
        .input
        .plugins
        .iter()
        .zip(context.storage)
        .zip(context.previous)
    {
        if needs_legacy_catalog(plugin, old.as_ref()) {
            let owner_id = plugin_owner_id(plugin.market_skill_id);
            let registrations = connector_registrations(plugin, &owner_id)?;
            let mutation = replace_connectors(context.input.conn, &owner_id, registrations).await?;
            let requires_reconcile = mutation.requires_reconcile;
            progress.catalog_mutations.push(mutation);
            if requires_reconcile {
                crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(context.input.conn)
                    .await?;
            }
        }
        let input = installation_input(plugin, transaction, context.input.agent_types)?;
        if transaction.previous_scope_matches(&context.input.account_scope)
            && old.as_ref().is_some_and(|record| {
                serde_json::from_str::<super::SkillPluginManifest>(
                    &record.installation.manifest_json,
                )
                .is_ok()
            })
        {
            plugin_installation_service::replace(context.input.conn, input).await
        } else {
            plugin_installation_service::restore(context.input.conn, input).await
        }
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

pub(super) fn begin_skills(
    agent_types: &[AgentType],
    root_skill_id: i64,
    installs: Vec<MarketSkillInstall>,
) -> Result<Option<crate::commands::acp::MarketSkillInstallTransaction>, AppCommandError> {
    if installs.is_empty() {
        return Ok(None);
    }
    crate::commands::acp::MarketSkillInstallTransaction::begin(
        (agent_types, root_skill_id),
        installs,
    )
    .map(Some)
    .map_err(map_skill_error)
}

fn finish_storage(storage: Vec<PluginStorageTransaction>, root_skill_id: i64) {
    for transaction in storage {
        transaction.finish();
    }
    tracing::info!(root_skill_id, "[plugin-install] atomic install completed");
}
