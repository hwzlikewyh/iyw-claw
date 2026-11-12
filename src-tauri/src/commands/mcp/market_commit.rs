use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::app_error::AppCommandError;
use crate::commands::mcp_catalog::{self, ManagedMcpCatalog, ManagedMcpCatalogEntry};

pub(super) struct Slot {
    previous: Option<ManagedMcpCatalogEntry>,
    tombstone: bool,
    account_scope: String,
}

pub(super) async fn prepare(conn: &DatabaseConnection, id: &str) -> Result<Slot, AppCommandError> {
    let account_scope = account_scope(conn).await?;
    let catalog = mcp_catalog::load_or_import(conn, super::scan_legacy_server_specs).await?;
    let previous = catalog.servers.get(id).cloned();
    if let Some(entry) = &previous {
        if !entry.sources.is_empty() || entry.spec["iyw_market"]["id"].as_str() != Some(id) {
            return Err(AppCommandError::invalid_input(
                "存在同名自定义或插件连接器，请先使用其原有管理入口",
            ));
        }
    }
    Ok(Slot {
        previous,
        tombstone: catalog.tombstones.contains(id),
        account_scope,
    })
}

pub(super) async fn commit(
    conn: &DatabaseConnection,
    input: (&str, Slot, Value),
) -> Result<(), AppCommandError> {
    let (id, slot, spec) = input;
    let _guard = mcp_catalog::lock_operation().await;
    let mut previous = load_for_commit(conn, &spec).await?;
    if let Err(error) = crate::commands::mcp_cli::ensure_idle(id) {
        discard_secret(&previous, &spec);
        return Err(error);
    }
    if previous.servers.get(id) != slot.previous.as_ref()
        || previous.tombstones.contains(id) != slot.tombstone
    {
        discard_secret(&previous, &spec);
        return Err(AppCommandError::new(
            crate::app_error::AppErrorCode::Conflict,
            "连接器在安装期间被其他操作修改，请重新检查后重试",
        ));
    }
    if account_scope(conn).await.ok().as_deref() != Some(slot.account_scope.as_str()) {
        discard_secret(&previous, &spec);
        return Err(AppCommandError::permission_denied(
            "安装期间账号或企业空间发生变化，请在当前空间重新发起安装",
        ));
    }
    let result = apply(conn, id, spec.clone()).await;
    if let Err(error) = result {
        restore(conn, (id, &previous, &spec)).await?;
        return Err(error);
    }
    if account_scope(conn).await.ok().as_deref() != Some(slot.account_scope.as_str()) {
        restore(conn, (id, &previous, &spec)).await?;
        return Err(AppCommandError::permission_denied(
            "安装提交期间账号或企业空间发生变化，原有连接器已恢复，请重新发起安装",
        ));
    }
    if let Some(old) = slot.previous {
        if let Some(entry) = previous.servers.get_mut(id) {
            entry.spec = spec;
        }
        discard_secret(&previous, &old.spec);
    }
    tracing::info!(connector=%id, "[MCP] 市场连接器配置已提交，运行中会话需重连应用");
    Ok(())
}

async fn load_for_commit(
    conn: &DatabaseConnection,
    spec: &Value,
) -> Result<ManagedMcpCatalog, AppCommandError> {
    match mcp_catalog::load_or_import_unlocked(conn, super::scan_legacy_server_specs).await {
        Ok(catalog) => Ok(catalog),
        Err(error) => {
            if let Some(key) = spec["iyw_secret_ref"].as_str() {
                if crate::keyring_store::delete_connector_secret(key).is_err() {
                    tracing::warn!("[MCP] 无法清理尚未提交的连接器凭据");
                }
            }
            Err(error)
        }
    }
}

async fn restore(
    conn: &DatabaseConnection,
    input: (&str, &ManagedMcpCatalog, &Value),
) -> Result<(), AppCommandError> {
    let (id, previous, spec) = input;
    if mcp_catalog::replace_catalog_unlocked(conn, previous)
        .await
        .is_err()
    {
        tracing::error!(connector=%id,"[MCP] 安装失败且目录回退失败，保留凭据以便恢复");
        return Err(AppCommandError::configuration_invalid(
            "连接器配置提交和回退均未完成，请刷新已安装项后恢复",
        ));
    }
    if super::super::mcp_sync::reconcile_all_managed_mcp_unlocked(conn)
        .await
        .is_err()
    {
        tracing::warn!(connector=%id,"[MCP] 旧连接器目录已恢复，运行时同步仍需重试");
        discard_secret(previous, spec);
        return Err(AppCommandError::configuration_invalid(
            "连接器目录已恢复，但部分智能体配置同步未完成，请重新保存连接器设置后重连",
        ));
    }
    discard_secret(previous, spec);
    Ok(())
}

async fn apply(conn: &DatabaseConnection, id: &str, spec: Value) -> Result<(), AppCommandError> {
    mcp_catalog::upsert_server_unlocked(conn, id, spec, super::scan_legacy_server_specs).await?;
    super::super::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await
}

fn discard_secret(catalog: &ManagedMcpCatalog, spec: &Value) {
    let Some(key) = spec["iyw_secret_ref"].as_str() else {
        return;
    };
    if catalog
        .servers
        .values()
        .any(|entry| entry.spec["iyw_secret_ref"].as_str() == Some(key))
    {
        return;
    }
    if crate::keyring_store::delete_connector_secret(key).is_err() {
        tracing::warn!("[MCP] 无法清理未引用的连接器凭据");
    }
}

async fn account_scope(conn: &DatabaseConnection) -> Result<String, AppCommandError> {
    super::super::iyw_account::iyw_account_usage_credentials_core(conn)
        .await?
        .map(|(_, scope)| scope)
        .ok_or_else(|| AppCommandError::permission_denied("请先登录爱原物账号"))
}
