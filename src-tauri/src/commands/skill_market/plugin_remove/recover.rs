use super::types::{InspectInput, RemovalReview};
use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service::{self, PluginInstallationInput};
use sea_orm::DatabaseConnection;

pub async fn recover(
    conn: &DatabaseConnection,
    input: InspectInput,
    emitter: crate::web::event_bridge::EventEmitter,
) -> Result<RemovalReview, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    let mut journal = super::journal::read(conn, &input.market_skill_id, &scope)
        .await?
        .ok_or_else(|| super::invalid("没有可恢复的移除记录"))?;
    if !matches!(
        journal.phase.as_str(),
        "prepared" | "recovery_required" | "restoring"
    ) {
        return Err(super::invalid("此移除记录不需要安装恢复"));
    }
    super::cleanup::require_root(&journal.plan)?;
    let root_id = super::super::types::parse_id(&input.market_skill_id)?;
    if !journal
        .backups
        .iter()
        .any(|backup| backup.market_skill_id == root_id)
    {
        return Err(super::invalid(
            "移除记录没有完整安装快照，请使用原版本安装入口",
        ));
    }
    validate_records(conn, &journal).await?;
    validate_pointers(&journal)?;
    let _catalog = crate::commands::mcp_catalog::lock_operation().await;
    validate_catalog(conn, &journal).await?;
    validate_root_package(&journal)?;
    pause_recovery(conn, &journal, &emitter).await?;
    journal.phase = "restoring".into();
    super::journal::write(conn, &journal).await?;
    restore(conn, &mut journal).await?;
    super::inspect::prepare(conn, &input.market_skill_id)
        .await
        .map(|prepared| prepared.view)
}

async fn pause_recovery(
    conn: &DatabaseConnection,
    journal: &super::journal::Journal,
    emitter: &crate::web::event_bridge::EventEmitter,
) -> Result<(), AppCommandError> {
    let slugs = journal
        .backups
        .iter()
        .map(|backup| backup.slug.clone())
        .collect::<Vec<_>>();
    for slug in &slugs {
        if crate::plugin_runtime::global::plugin_version_has_active_leases(slug, None).await {
            return Err(super::invalid(
                "相关插件仍在使用，等待工具或页面结束后再恢复",
            ));
        }
    }
    for slug in &slugs {
        crate::plugin_runtime::registry::suspend_global(slug);
    }
    crate::commands::plugin_preparation::mutation::stop_plugins_for_update(conn, &slugs, emitter)
        .await;
    Ok(())
}

async fn validate_catalog(
    conn: &DatabaseConnection,
    journal: &super::journal::Journal,
) -> Result<(), AppCommandError> {
    super::catalog::validate(conn, journal.catalog_ref.as_deref()).await
}

async fn validate_records(
    conn: &DatabaseConnection,
    journal: &super::journal::Journal,
) -> Result<(), AppCommandError> {
    for backup in &journal.backups {
        let current =
            plugin_installation_service::find_by_market_skill_id(conn, backup.market_skill_id)
                .await
                .map_err(AppCommandError::db)?;
        let Some(current) = current else {
            if backup.market_skill_id.to_string() == journal.plan.id {
                continue;
            }
            return Err(super::invalid("相关安装已被移除，旧恢复记录未应用"));
        };
        let actual = super::super::plugin_install_data::record_input(&current);
        if !same_record(&actual, backup) {
            return Err(super::invalid(
                "相关插件的版本、参数声明或权限已变化，恢复未覆盖当前安装",
            ));
        }
        if backup.market_skill_id.to_string() == journal.plan.id
            && journal.phase != "restoring"
            && current.installation.updated_at.to_rfc3339() != journal.root_updated_at
            && !matches!(
                current.installation.reconcile_state.as_str(),
                "removing" | "repair_required"
            )
        {
            return Err(super::invalid("插件已重新安装，不能应用旧移除恢复"));
        }
    }
    Ok(())
}

fn same_record(actual: &PluginInstallationInput, backup: &PluginInstallationInput) -> bool {
    let mut candidate = actual.clone();
    candidate.status = backup.status.clone();
    candidate.reconcile_state = backup.reconcile_state.clone();
    for activation in &mut candidate.runtime_state.activations {
        if activation.policy_source == "dependency_removed" {
            if let Some(old) = backup.runtime_state.activations.iter().find(|old| {
                old.component_key == activation.component_key
                    && old.scope == activation.scope
                    && old.workspace_key == activation.workspace_key
                    && old.agent_type == activation.agent_type
            }) {
                *activation = old.clone();
            }
        }
    }
    let mut backup = backup.clone();
    normalize(&mut candidate);
    normalize(&mut backup);
    serde_json::to_value(candidate).ok() == serde_json::to_value(backup).ok()
}

fn normalize(input: &mut PluginInstallationInput) {
    input.components.sort_by(|a, b| {
        (&a.component_type, &a.component_key).cmp(&(&b.component_type, &b.component_key))
    });
    input.runtime_state.activations.sort_by(|a, b| {
        (&a.component_key, &a.scope, &a.workspace_key, &a.agent_type).cmp(&(
            &b.component_key,
            &b.scope,
            &b.workspace_key,
            &b.agent_type,
        ))
    });
    input.runtime_state.permission_grants.sort_by(|a, b| {
        (&a.scope, &a.workspace_key, &a.permissions_digest).cmp(&(
            &b.scope,
            &b.workspace_key,
            &b.permissions_digest,
        ))
    });
}

fn validate_pointers(journal: &super::journal::Journal) -> Result<(), AppCommandError> {
    for pointer in &journal.pointers {
        let path = super::super::plugin_history::local::root(&pointer.slug)?.join("current.json");
        let current = super::super::plugin_storage_io::read_pointer_bytes(&path)?;
        if current.as_deref() != Some(pointer.previous.as_slice())
            && current.as_deref() != Some(pointer.next.as_slice())
        {
            return Err(super::invalid("相关插件引用发生变化，恢复未覆盖新引用"));
        }
    }
    Ok(())
}

fn validate_root_package(journal: &super::journal::Journal) -> Result<(), AppCommandError> {
    let backup = journal
        .backups
        .iter()
        .find(|backup| backup.market_skill_id.to_string() == journal.plan.id)
        .ok_or_else(|| super::invalid("原插件安装快照缺失"))?;
    let root = super::super::plugin_history::local::root(&journal.plan.slug)?;
    let root = if root.exists() {
        root
    } else {
        let name = journal
            .plan
            .package_trash
            .as_ref()
            .ok_or_else(|| super::invalid("原插件包备份缺失"))?;
        super::super::plugin_storage_io::active_paths()?
            .trash_dir()
            .join("plugins")
            .join(name)
    };
    let version =
        semver::Version::parse(&backup.version).map_err(|_| super::invalid("原插件版本无效"))?;
    let package = crate::acp::skill_package::validate_plugin_directory(
        &root.join("versions").join(version.to_string()),
        &backup.content_sha256,
    )?;
    let manifest = serde_json::from_str(&backup.manifest_json)
        .map_err(|_| super::invalid("原插件清单快照无效"))?;
    super::super::plugin_manifest::validate_plugin_package(
        &package,
        &manifest,
        &backup.slug,
        &backup.version,
    )?;
    Ok(())
}

async fn restore(
    conn: &DatabaseConnection,
    journal: &mut super::journal::Journal,
) -> Result<(), AppCommandError> {
    super::super::plugin_storage::PluginStorageRemoval::recover(
        &journal.plan.slug,
        journal.plan.package_trash.as_deref(),
    )?;
    if let Some(skills) = journal.skills.clone() {
        crate::commands::acp::PluginSkillRemoval::recover(skills)
            .map_err(|error| super::invalid(&error.to_string()))?;
    }
    for pointer in &journal.pointers {
        let path = super::super::plugin_history::local::root(&pointer.slug)?.join("current.json");
        super::super::plugin_storage_io::write_bytes_atomically(&path, &pointer.previous)?;
    }
    super::catalog::restore(conn, journal.catalog_ref.as_deref()).await?;
    for backup in &journal.backups {
        plugin_installation_service::restore(conn, backup.clone())
            .await
            .map_err(AppCommandError::db)?;
    }
    crate::plugin_runtime::registry::reconcile_global(conn).await?;
    journal.phase = "cancelled".into();
    super::journal::write(conn, journal).await?;
    if let Err(error) = super::catalog::clear(journal.catalog_ref.as_deref()) {
        tracing::warn!(code=?error.code,"[plugin-remove] 安装已恢复，旧连接器快照清理延后");
    }
    Ok(())
}
