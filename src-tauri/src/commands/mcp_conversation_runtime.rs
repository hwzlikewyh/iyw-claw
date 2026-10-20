use std::collections::BTreeMap;

use crate::acp::error::AcpError;
use crate::models::AgentType;
use sea_orm::DatabaseConnection;

pub(crate) async fn project_xinghe(
    conn: &DatabaseConnection,
    conversation_id: Option<i32>,
    runtime_env: &mut BTreeMap<String, String>,
) -> Result<(), AcpError> {
    let selected = super::mcp_conversation::specs(conn, (conversation_id, AgentType::Codex))
        .await
        .map_err(|error| AcpError::protocol(error.message))?;
    let catalog = super::mcp_catalog::load_or_import(conn, super::mcp::scan_legacy_server_specs)
        .await
        .map_err(|error| AcpError::protocol(error.message))?;
    let raw = runtime_env
        .get("CODEX_CONFIG")
        .map(String::as_str)
        .unwrap_or("{}");
    let mut config: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(raw).map_err(|_| AcpError::protocol("星河 MCP 配置格式无效"))?;
    let servers = config
        .entry("mcp_servers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| AcpError::protocol("星河 MCP 配置必须是 table"))?;
    for name in catalog
        .servers
        .iter()
        .filter(|(_, entry)| entry.managed)
        .map(|(name, _)| name)
        .chain(catalog.tombstones.iter())
    {
        servers.remove(name);
    }
    for (name, spec) in selected {
        let entry = super::mcp::canonical_to_codex_entry(&spec)
            .map_err(|error| AcpError::protocol(error.message))?;
        servers.insert(
            name,
            serde_json::to_value(entry)
                .map_err(|_| AcpError::protocol("无法转换对话连接器配置"))?,
        );
    }
    runtime_env.insert(
        "CODEX_CONFIG".into(),
        serde_json::to_string(&config).map_err(|_| AcpError::protocol("无法生成对话连接器配置"))?,
    );
    Ok(())
}
