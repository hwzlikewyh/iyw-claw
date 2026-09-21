use std::collections::BTreeSet;
use std::path::Path;

use crate::acp::manager::ConnectionManager;
use crate::acp::types::ConfigStaleKind;
use crate::db::AppDatabase;

pub(crate) async fn refresh_running_sessions(
    manager: &ConnectionManager,
    db: &AppDatabase,
    data_dir: &Path,
) {
    let agents = manager
        .list_connections()
        .await
        .into_iter()
        .map(|connection| connection.agent_type)
        .filter(|agent| crate::commands::mcp_sync::is_managed_mcp_target(*agent))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if agents.is_empty() {
        return;
    }
    let stale_count = crate::commands::acp::refresh_config_staleness(
        manager,
        db,
        data_dir,
        &agents,
        ConfigStaleKind::AgentConfig,
    )
    .await;
    tracing::info!(stale_count, "[MCP] 连接器变更后的会话配置检查完成");
}

#[cfg(feature = "tauri-runtime")]
pub(crate) async fn refresh_tauri(app: &tauri::AppHandle, db: &AppDatabase) {
    use tauri::Manager;

    let data_dir = match app.path().app_data_dir() {
        Ok(path) => crate::paths::resolve_effective_data_dir(&path),
        Err(error) => {
            tracing::warn!(error = %error, "[MCP] 无法解析会话配置检查目录");
            return;
        }
    };
    refresh_running_sessions(&app.state::<ConnectionManager>(), db, &data_dir).await;
}
