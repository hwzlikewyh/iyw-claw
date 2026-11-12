use crate::{app_error::AppCommandError, commands::mcp_catalog::ManagedMcpCatalog};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Backup {
    before: ManagedMcpCatalog,
    after: ManagedMcpCatalog,
}
const MAX_CATALOG_BYTES: usize = 4 * 1024 * 1024;

pub(super) fn save(before: ManagedMcpCatalog, owner: &str) -> Result<String, AppCommandError> {
    let after = crate::commands::mcp_catalog_sources::removal_snapshot(&before, owner);
    let raw = serde_json::to_string(&Backup { before, after })
        .map_err(|_| super::invalid("连接器恢复快照无法编码"))?;
    if raw.len() > MAX_CATALOG_BYTES {
        return Err(super::invalid("连接器恢复快照超过安全容量"));
    }
    let reference = format!("plugin-removal-catalog:{}", uuid::Uuid::new_v4());
    crate::keyring_store::set_connector_secret(&reference, &raw)
        .map_err(|_| super::invalid("连接器恢复快照无法写入安全存储"))?;
    Ok(reference)
}

pub(super) async fn validate(
    conn: &DatabaseConnection,
    reference: Option<&str>,
) -> Result<(), AppCommandError> {
    let Some(reference) = reference else {
        return Ok(());
    };
    let backup = load(reference)?;
    let current = crate::commands::mcp_catalog::load_or_import_unlocked(
        conn,
        crate::commands::mcp::scan_legacy_server_specs,
    )
    .await?;
    if current != backup.before && current != backup.after {
        return Err(super::invalid(
            "共用连接器配置已变化，旧恢复快照未覆盖新配置",
        ));
    }
    Ok(())
}

pub(super) async fn restore(
    conn: &DatabaseConnection,
    reference: Option<&str>,
) -> Result<(), AppCommandError> {
    let Some(reference) = reference else {
        return Ok(());
    };
    validate(conn, Some(reference)).await?;
    crate::commands::mcp_catalog::replace_catalog_unlocked(conn, &load(reference)?.before).await?;
    crate::commands::mcp_sync::reconcile_all_managed_mcp_unlocked(conn).await
}

pub(super) fn clear(reference: Option<&str>) -> Result<(), AppCommandError> {
    if let Some(reference) = reference {
        require_ref(reference)?;
        crate::keyring_store::delete_connector_secret(reference)
            .map_err(|_| super::invalid("旧连接器恢复快照清理未完成"))?;
    }
    Ok(())
}

fn load(reference: &str) -> Result<Backup, AppCommandError> {
    require_ref(reference)?;
    let raw = crate::keyring_store::get_connector_secret(reference)
        .ok_or_else(|| super::invalid("连接器恢复快照在安全存储中缺失"))?;
    if raw.len() > MAX_CATALOG_BYTES {
        return Err(super::invalid("连接器恢复快照超过读取限制"));
    }
    serde_json::from_str(&raw).map_err(|_| super::invalid("连接器恢复快照损坏"))
}
fn require_ref(reference: &str) -> Result<(), AppCommandError> {
    if !reference
        .strip_prefix("plugin-removal-catalog:")
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
    {
        return Err(super::invalid("连接器恢复引用无效"));
    }
    Ok(())
}
