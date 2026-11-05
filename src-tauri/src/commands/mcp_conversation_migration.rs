use std::collections::BTreeSet;

use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;

pub(crate) async fn migrate_selected(
    conn: &DatabaseConnection,
    newly_selected: &BTreeSet<String>,
    selected: &BTreeSet<String>,
) -> Result<Vec<String>, AppCommandError> {
    let mut catalog = crate::commands::mcp_catalog::load_or_import_unlocked(
        conn,
        crate::commands::mcp::scan_legacy_server_specs,
    )
    .await?;
    let original = catalog.clone();
    let mut migrated = Vec::new();
    for id in newly_selected {
        migrate_entry(&mut catalog, id, &mut migrated)?;
    }
    for id in selected {
        migrate_entry(&mut catalog, id, &mut migrated)?;
    }
    if !migrated.is_empty() {
        crate::commands::mcp_catalog::replace_catalog_unlocked(conn, &catalog).await?;
        if let Err(error) =
            crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await
        {
            if let Err(rollback) =
                crate::commands::mcp_catalog::replace_catalog_unlocked(conn, &original).await
            {
                tracing::error!(error = %rollback, "[MCP] connector migration catalog rollback failed");
                let error_detail = error.detail.clone().unwrap_or_default();
                return Err(error.with_detail(format!(
                    "{}; catalog rollback failed: {}",
                    error_detail, rollback.message
                )));
            }
            if let Err(rollback) =
                crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await
            {
                tracing::error!(error = %rollback, "[MCP] connector migration projection rollback failed");
            }
            return Err(error);
        }
    }
    Ok(migrated)
}

fn migrate_entry(
    catalog: &mut crate::commands::mcp_catalog::ManagedMcpCatalog,
    id: &str,
    migrated: &mut Vec<String>,
) -> Result<(), AppCommandError> {
    let Some(entry) = catalog.servers.get_mut(id) else {
        return Err(AppCommandError::not_found("连接器已卸载"));
    };
    let needs_migration =
        !entry.managed || (entry.enabled && entry.spec.get("iyw_market").is_none());
    if !needs_migration {
        return Ok(());
    }
    if !entry.managed && !crate::commands::mcp::legacy_server_spec_matches(id, &entry.spec)? {
        return Err(AppCommandError::invalid_input(
            "同名连接器在不同 Agent 中配置不一致或已不再检测到；请先在连接器设置中重命名并统一配置",
        ));
    }
    entry.managed = true;
    entry.enabled = false;
    migrated.push(id.to_string());
    Ok(())
}
