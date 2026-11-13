use std::collections::BTreeSet;

use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillInstall;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};

use super::plugin_install_context::{MarketInstallPlanExecution, PreparedPluginInstall};

use super::plugin_install_rollback::rollback_storage;
use super::plugin_storage::PluginStorageTransaction;
pub(super) use super::plugin_uninstall::uninstall_plugin;

pub(super) async fn install_market_plan(
    input: MarketInstallPlanExecution<'_>,
) -> Result<(), AppCommandError> {
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    install_market_plan_locked(input).await
}

pub(super) async fn install_market_plan_locked(
    mut input: MarketInstallPlanExecution<'_>,
) -> Result<(), AppCommandError> {
    log_install_start(
        input.root_skill_id,
        input.plugins.len(),
        input.skill_installs.len(),
    );
    if crate::commands::mcp_cli_config::account_scope(input.conn).await? != input.account_scope {
        return Err(AppCommandError::permission_denied(
            "下载期间账号或企业空间发生变化，请重新发起安装",
        ));
    }
    let mut versions = input
        .plugins
        .iter()
        .map(|plugin| {
            (
                plugin.market_skill_id,
                (plugin.slug.clone(), plugin.version.clone()),
            )
        })
        .collect::<super::plugin_constraints::PlannedVersions>();
    versions.extend(input.skill_installs.iter().map(|skill| {
        (
            skill.marker.skill_id,
            (
                skill.marker.slug.clone(),
                skill.marker.installed_version.clone(),
            ),
        )
    }));
    let issues = super::plugin_constraints::issues(input.conn, &versions).await?;
    if !issues.is_empty() {
        return Err(AppCommandError::configuration_invalid(issues.join("；")));
    }
    for plugin in &input.plugins {
        super::plugin_platforms::require_current(&plugin.plugin.manifest.platforms)?;
        super::plugin_client_version::require_current(&plugin.plugin.manifest.min_client_version)?;
        input.skill_installs.extend(plugin_skill_installs(plugin)?);
    }
    if input.plugins.is_empty() {
        return commit_standalone(input).await;
    }
    pause_affected_plugins(input.conn, &input.plugins).await?;
    let conn = input.conn;
    let result = super::plugin_install_commit::commit_plugins(input).await;
    if result.is_err() {
        if let Err(error) = crate::plugin_runtime::registry::reconcile_global(conn).await {
            tracing::error!(error=%error, "[plugin-install] 提交失败后插件注册表恢复未完成");
        }
    }
    result
}

async fn pause_affected_plugins(
    conn: &DatabaseConnection,
    planned: &[PreparedPluginInstall],
) -> Result<(), AppCommandError> {
    crate::plugin_runtime::registry::reconcile_global(conn).await?;
    let Some(snapshot) = crate::plugin_runtime::registry::global_snapshot() else {
        return Ok(());
    };
    let planned_ids = planned
        .iter()
        .map(|plugin| plugin.market_skill_id)
        .collect::<BTreeSet<_>>();
    let affected = snapshot
        .plugins
        .values()
        .filter(|plugin| {
            planned_ids.contains(&plugin.market_skill_id)
                || crate::plugin_runtime::dependencies::inspect(plugin, &snapshot.plugins)
                    .dependencies
                    .iter()
                    .any(|dependency| {
                        dependency
                            .id
                            .parse::<i64>()
                            .map(|id| planned_ids.contains(&id))
                            .unwrap_or(false)
                    })
        })
        .map(|plugin| plugin.slug.clone())
        .collect::<Vec<_>>();
    for slug in &affected {
        if crate::plugin_runtime::global::plugin_version_has_active_leases(slug, None).await {
            return Err(AppCommandError::task_execution_failed(format!(
                "插件 {slug} 仍在使用，请等待当前操作完成后再更新",
            )));
        }
    }
    for slug in &affected {
        crate::plugin_runtime::registry::suspend_global(slug);
        crate::plugin_runtime::global::stop_plugin(slug).await;
    }
    if !affected.is_empty() {
        tracing::info!(plugins=?affected, "[plugin-install] 已暂停受更新影响的插件");
    }
    Ok(())
}

async fn commit_standalone(input: MarketInstallPlanExecution<'_>) -> Result<(), AppCommandError> {
    let skills = super::plugin_install_commit::begin_skills(
        input.agent_types,
        input.root_skill_id,
        input.skill_installs,
    )?;
    let result = async {
        if crate::commands::mcp_cli_config::account_scope(input.conn).await? != input.account_scope
        {
            return Err(AppCommandError::permission_denied(
                "安装期间账号发生变化，已取消",
            ));
        }
        crate::plugin_runtime::registry::reconcile_global(input.conn).await?;
        Ok::<(), AppCommandError>(())
    }
    .await;
    if let Err(error) = result {
        if let Some(skills) = skills {
            skills.rollback().map_err(map_skill_error)?;
        }
        return Err(error);
    }
    if let Some(skills) = skills {
        if let Err(error) = skills.migrate_plugin_skill_policies(input.conn).await {
            skills.rollback().map_err(map_skill_error)?;
            return Err(error);
        }
        skills.finish();
    }
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

pub(super) async fn stage_storage_with_previous(
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
            rollback_storage(&mut storage)?;
            Err(error)
        }
    }
}

fn plugin_skill_installs(
    plugin: &PreparedPluginInstall,
) -> Result<Vec<MarketSkillInstall>, AppCommandError> {
    let mut installs = Vec::new();
    let names = plugin.plugin.manifest.components.iter()
        .filter(|component| component.kind == "skill")
        .map(|component| (
            component.path.rsplit('/').next().unwrap_or(&component.key).to_string(),
            crate::acp::skill_package::plugin_skill_id(&plugin.slug, &component.key),
        ))
        .collect();
    for component in &plugin.plugin.manifest.components {
        if component.kind != "skill" {
            continue;
        }
        let package = plugin.package.plugin_skill_component(&component.path, &names)?;
        let mut marker = plugin.marker.clone();
        marker.slug = crate::acp::skill_package::plugin_skill_id(&plugin.slug, &component.key);
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
                rollback_storage(&mut result)?;
                return Err(error);
            }
        }
    }
    Ok(result)
}

pub(super) fn commit_storage(
    storage: &mut [PluginStorageTransaction],
    plugins: &[PreparedPluginInstall],
    scope: &str,
) -> Result<(), AppCommandError> {
    let plugin_ids = plugins
        .iter()
        .map(|plugin| plugin.market_skill_id)
        .collect::<std::collections::BTreeSet<_>>();
    for (transaction, plugin) in storage.iter_mut().zip(plugins) {
        if plugin.marker.content_sha256 != plugin.package.content_sha256
            || plugin.marker.object_sha256 != plugin.object_sha256
        {
            return Err(AppCommandError::configuration_invalid(
                "Plugin install snapshot does not match the verified artifact",
            ));
        }
        let dependencies = plugin
            .marker
            .dependencies
            .iter()
            .filter(|dependency| plugin_ids.contains(&dependency.skill_id))
            .map(|dependency| dependency.skill_id)
            .collect::<Vec<_>>();
        transaction.commit(plugin, (&dependencies, scope))?;
    }
    Ok(())
}

async fn load_previous_records(
    conn: &DatabaseConnection,
    plugins: &[PreparedPluginInstall],
) -> Result<Vec<Option<PluginInstallationRecord>>, AppCommandError> {
    let mut result = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let previous =
            plugin_installation_service::find_by_market_skill_id(conn, plugin.market_skill_id)
                .await
                .map_err(AppCommandError::db)?;
        if let Some(record) = &previous {
            if let Err(error) = super::plugin_receipt::capture_previous(record).await {
                tracing::warn!(plugin=%record.installation.slug,error=%error,"[plugin-install] 旧版本未能验证为回退快照，保留文件供原安装入口修复");
            }
        }
        result.push(previous);
    }
    Ok(result)
}

pub(super) fn map_skill_error(error: crate::acp::error::AcpError) -> AppCommandError {
    super::install::map_local_install_error(error)
}
