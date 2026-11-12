use super::{journal::Journal, types::Prepared};
use crate::app_error::AppCommandError;
use crate::commands::{acp::PluginSkillRemoval, mcp_catalog_sources::PluginCatalogMutation};
use crate::db::service::plugin_installation_service;
use sea_orm::DatabaseConnection;

pub(super) async fn commit(
    conn: &DatabaseConnection,
    prepared: &Prepared,
    journal: &mut Journal,
) -> Result<(), AppCommandError> {
    let root = &prepared.root;
    let removal =
        super::super::plugin_storage::PluginStorageRemoval::reserve(&root.installation.slug)?;
    let uses_catalog = super::super::plugin_install_data::installation_uses_legacy_catalog(root);
    let _catalog = super::super::plugin_install_data::lock_legacy_catalog(uses_catalog).await;
    journal.plan.package_trash = removal.cleanup_name();
    let mut state = CommitState {
        removal,
        catalog: None,
        skills: None,
    };
    if let Err(error) = apply_changes(conn, prepared, (journal, &mut state, uses_catalog)).await {
        return rollback(conn, prepared, (journal, &mut state, error)).await;
    }
    if let Some(skills) = state.skills.take() {
        skills.finish();
    }
    if let Err(error) = crate::plugin_runtime::registry::reconcile_global(conn).await {
        tracing::warn!(code=?error.code,"[plugin-remove] 移除后相关目录刷新待继续");
    }
    Ok(())
}

struct CommitState {
    removal: super::super::plugin_storage::PluginStorageRemoval,
    catalog: Option<PluginCatalogMutation>,
    skills: Option<PluginSkillRemoval>,
}

async fn apply_changes(
    conn: &DatabaseConnection,
    prepared: &Prepared,
    context: (&mut Journal, &mut CommitState, bool),
) -> Result<(), AppCommandError> {
    let (journal, state, uses_catalog) = context;
    let root = &prepared.root;
    let id = root.installation.market_skill_id;
    prepare_journal(conn, prepared, (journal, state, uses_catalog)).await?;
    deactivate(conn, prepared, false).await?;
    let mut root_state = super::super::plugin_install_data::record_input(root);
    root_state.reconcile_state = "removing".into();
    plugin_installation_service::restore(conn, root_state)
        .await
        .map_err(AppCommandError::db)?;
    state.removal.move_package()?;
    if uses_catalog {
        state.catalog = Some(
            super::super::plugin_install_data::replace_connectors(
                conn,
                &super::super::plugin_install_data::plugin_owner_id(id),
                Vec::new(),
            )
            .await?,
        );
    }
    if let Some(skills) = &mut state.skills {
        skills
            .remove(id, &root.installation.slug)
            .map_err(map_skill)?;
    }
    write_pointers(prepared, false)?;
    super::super::plugin_uninstall::delete_plugin_state(conn, id, state.catalog.as_ref()).await?;
    if crate::commands::mcp_cli_config::account_scope(conn).await? != prepared.scope {
        return Err(super::invalid("移除提交期间账号范围变化，已开始恢复原安装"));
    }
    journal.phase = "committed".into();
    super::journal::write(conn, journal).await
}

async fn rollback(
    conn: &DatabaseConnection,
    prepared: &Prepared,
    context: (&mut Journal, &mut CommitState, AppCommandError),
) -> Result<(), AppCommandError> {
    let (journal, state, error) = context;
    let root = &prepared.root;
    let id = root.installation.market_skill_id;
    let mut failed = state
        .skills
        .take()
        .map(|value| value.rollback())
        .transpose()
        .is_err();
    failed |= write_pointers(prepared, true).is_err();
    failed |= deactivate(conn, prepared, true).await.is_err();
    failed |= super::super::plugin_install_rollback::rollback_uninstall(
        conn,
        root,
        (state.catalog.as_ref(), &mut state.removal),
    )
    .await
    .is_err();
    journal.phase = if failed {
        "recovery_required"
    } else {
        "cancelled"
    }
    .into();
    if super::journal::write(conn, journal).await.is_err() {
        failed = true;
    }
    if failed {
        mark_failed(conn, prepared).await;
        return Err(super::invalid(
            "移除失败且部分旧状态恢复未完成，已保留包与记录，请先恢复安装记录",
        ));
    }
    return Err(error);
}

fn write_pointers(prepared: &Prepared, restore: bool) -> Result<(), AppCommandError> {
    for change in &prepared.pointers {
        let current = super::super::plugin_storage_io::read_pointer_bytes(&change.path)?;
        let (expected, next) = if restore {
            (&change.next, &change.previous)
        } else {
            (&change.previous, &change.next)
        };
        if current.as_deref() == Some(next.as_slice()) {
            continue;
        }
        if current.as_deref() != Some(expected.as_slice()) {
            return Err(super::invalid("共用组件的引用已变化，未覆盖其它操作"));
        }
        super::super::plugin_storage_io::write_bytes_atomically(&change.path, next)?;
    }
    Ok(())
}

async fn deactivate(
    conn: &DatabaseConnection,
    prepared: &Prepared,
    restore: bool,
) -> Result<(), AppCommandError> {
    for record in &prepared.records {
        let id = record.installation.market_skill_id.to_string();
        let affected = prepared.view.affected.iter().any(|value| value.id == id)
            || prepared
                .view
                .retained
                .iter()
                .any(|value| value.id == id && !value.shared);
        if !affected {
            continue;
        }
        let mut input = super::super::plugin_install_data::record_input(record);
        if !restore {
            input.reconcile_state = "removal_pending".into();
            for activation in &mut input.runtime_state.activations {
                activation.requested_enabled = false;
                activation.policy_source = "dependency_removed".into();
            }
        }
        plugin_installation_service::restore(conn, input)
            .await
            .map_err(AppCommandError::db)?;
    }
    Ok(())
}

fn map_skill(error: crate::acp::error::AcpError) -> AppCommandError {
    super::super::install::map_local_install_error(error)
}

async fn prepare_journal(
    conn: &DatabaseConnection,
    prepared: &Prepared,
    context: (&mut Journal, &mut CommitState, bool),
) -> Result<(), AppCommandError> {
    let (journal, state, uses_catalog) = context;
    let root = &prepared.root;
    let id = root.installation.market_skill_id;
    if uses_catalog {
        let before = crate::commands::mcp_catalog::load_or_import_unlocked(
            conn,
            crate::commands::mcp::scan_legacy_server_specs,
        )
        .await?;
        journal.catalog_ref = Some(super::catalog::save(
            before,
            &super::super::plugin_install_data::plugin_owner_id(id),
        )?);
    }
    state.skills = Some(PluginSkillRemoval::stage(id, &root.installation.slug).map_err(map_skill)?);
    journal.skills = state
        .skills
        .as_ref()
        .map(|value| value.record())
        .transpose()
        .map_err(map_skill)?;
    super::journal::write(conn, journal).await
}

async fn mark_failed(conn: &DatabaseConnection, prepared: &Prepared) {
    let id = prepared.root.installation.market_skill_id;
    let records = prepared
        .records
        .iter()
        .filter(|record| {
            record.installation.market_skill_id == id
                || prepared
                    .view
                    .affected
                    .iter()
                    .any(|value| value.id == record.installation.market_skill_id.to_string())
                || prepared.view.retained.iter().any(|value| {
                    value.id == record.installation.market_skill_id.to_string() && !value.shared
                })
        })
        .map(|record| (record.installation.market_skill_id, Some(record.clone())))
        .collect::<Vec<_>>();
    super::super::plugin_install_rollback::mark_failed(conn, &records).await;
}
