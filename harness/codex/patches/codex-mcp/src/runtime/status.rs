use std::collections::{HashMap, HashSet};

use codex_protocol::mcp::McpServerConnectionStatus;

use super::{McpConfig, McpRuntime, McpRuntimeContext};
use crate::mcp::{
    McpServerStatusSnapshot, McpSnapshotDetail, collect_published_status, compute_auth_statuses,
    effective_mcp_servers,
};

impl McpRuntime {
    /// 从同一已发布代际读取目录和状态，不为状态查询创建第二套连接。
    pub async fn status_snapshot(
        &self,
        requested: &McpConfig,
        context: &McpRuntimeContext,
        detail: McpSnapshotDetail,
    ) -> (
        McpServerStatusSnapshot,
        HashMap<String, McpServerConnectionStatus>,
    ) {
        let current = self.current.load_full();
        let servers = effective_mcp_servers(requested, current.auth.as_ref());
        let auth = compute_auth_statuses(
            servers.iter(),
            requested.mcp_oauth_credentials_store_mode,
            requested.auth_keyring_backend_kind,
            current.auth.as_ref(),
            context,
        )
        .await;
        let mut statuses = current.connections.connection_statuses().await;
        let matching = statuses
            .keys()
            .filter(|name| {
                current.config.as_ref().is_some_and(|published| {
                    published
                        .mcp_server_catalog
                        .server(name)
                        .is_some_and(|server| {
                            requested.mcp_server_catalog.server(name) == Some(server)
                        })
                })
            })
            .cloned()
            .collect::<HashSet<_>>();
        statuses.retain(|name, _| matching.contains(name));
        let snapshot = collect_published_status(
            &current.connections,
            auth,
            (matching, servers.keys().cloned().collect(), detail),
        )
        .await;
        (snapshot, statuses)
    }
}
