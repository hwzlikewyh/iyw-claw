//! 内置星河直接消费启动参数；受管连接器仅覆盖自己拥有的配置。

use sea_orm::DatabaseConnection;

use crate::acp::error::AcpError;
use crate::commands::mcp_catalog::{
    ManagedMcpCatalog, ManagedMcpCatalogEntry, MANAGED_MCP_CATALOG_KEY,
};
use crate::db::service::app_metadata_service;

pub(crate) async fn project_preferences(
    conn: &DatabaseConnection,
    preferences: &str,
) -> Result<String, AcpError> {
    let _guard = crate::commands::mcp_catalog::lock_operation().await;
    let raw = app_metadata_service::get_value(conn, MANAGED_MCP_CATALOG_KEY)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "[MCP] 无法读取星河连接器目录");
            AcpError::protocol(error.to_string())
        })?;
    let Some(raw) = raw else {
        return Ok(preferences.to_string());
    };
    let (catalog, _) =
        crate::commands::mcp_catalog_persistence::parse_catalog(&raw).map_err(|error| {
            tracing::error!(code = ?error.code, "[MCP] 星河连接器目录解析失败");
            AcpError::protocol("星河连接器目录损坏，请检查能力市场设置")
        })?;
    if catalog.servers.values().all(|entry| !entry.managed) && catalog.tombstones.is_empty() {
        return Ok(preferences.to_string());
    }
    let mut config: toml::Table =
        toml::from_str(preferences).map_err(|_| AcpError::protocol("星河配置必须是有效的 TOML"))?;
    merge_catalog(&mut config, &catalog)?;
    toml::to_string(&config).map_err(|_| AcpError::protocol("无法生成星河连接器启动参数"))
}

fn merge_catalog(config: &mut toml::Table, catalog: &ManagedMcpCatalog) -> Result<(), AcpError> {
    remove_legacy_managed_servers(config, catalog);
    let had_servers = config.contains_key("mcp_servers");
    let servers = config
        .entry("mcp_servers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or_else(|| AcpError::protocol("星河 mcp_servers 必须是配置表"))?;
    let mut enabled_count = 0;
    let mut removed_count = 0;
    for (name, entry) in catalog.servers.iter().filter(|(_, entry)| entry.managed) {
        if entry.enabled {
            let server = project_server(name, entry)?;
            servers.insert(name.clone(), server);
            enabled_count += 1;
        } else {
            removed_count += usize::from(servers.remove(name).is_some());
        }
    }
    for name in &catalog.tombstones {
        removed_count += usize::from(servers.remove(name).is_some());
    }
    tracing::debug!(
        enabled_count,
        removed_count,
        total_count = servers.len(),
        "[MCP] 星河连接器已合入启动参数"
    );
    if servers.is_empty() && !had_servers {
        config.remove("mcp_servers");
    }
    Ok(())
}

fn remove_legacy_managed_servers(config: &mut toml::Table, catalog: &ManagedMcpCatalog) {
    let Some(legacy) = config
        .get_mut("mcp")
        .and_then(|value| value.get_mut("servers"))
        .and_then(toml::Value::as_table_mut)
    else {
        return;
    };
    legacy.retain(|name, _| {
        !catalog.servers.get(name).is_some_and(|entry| entry.managed)
            && !catalog.tombstones.contains(name)
    });
}

fn project_server(name: &str, entry: &ManagedMcpCatalogEntry) -> Result<toml::Value, AcpError> {
    if !entry.missing_config.is_empty() {
        tracing::error!(
            server = name,
            missing_field_count = entry.missing_config.len(),
            "[MCP] 已启用的星河连接器缺少配置"
        );
        return Err(AcpError::protocol(
            "已启用的星河连接器缺少必填配置，请返回能力市场补充",
        ));
    }
    if entry.spec.get("type").and_then(serde_json::Value::as_str) == Some("sse") {
        tracing::error!(
            server = name,
            transport = "sse",
            "[MCP] 星河连接器传输不受支持"
        );
        return Err(AcpError::protocol(
            "内置星河不支持 SSE 连接器，请在能力市场改用 Streamable HTTP 或 stdio",
        ));
    }
    // 复用原生转换，保留 cwd、超时、工具过滤和环境认证等扩展字段。
    super::canonical_to_codex_entry(&entry.spec).map_err(|error| {
        tracing::error!(server = name, code = ?error.code, "[MCP] 星河连接器参数投影失败");
        AcpError::protocol("星河连接器参数无效，请检查能力市场配置")
    })
}
