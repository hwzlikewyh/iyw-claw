//! Read selected-server metadata from one published thread runtime.

use codex_protocol::mcp::McpServerConnectionStatus;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::McpRuntime;
use super::McpRuntimeContext;
use crate::McpConfig;
use crate::McpServerStatusSnapshot;
use crate::McpSnapshotDetail;
use crate::mcp::collect_mcp_server_status_snapshot_from_manager;
use crate::mcp::collect_published_status;
use crate::mcp::compute_auth_statuses;
use crate::mcp::effective_mcp_servers;

impl McpRuntime {
    /// 复用同一已发布代际，不为会话状态查询创建第二套连接。
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
        let context = context.clone().with_selected_environments(
            Arc::clone(&current.environment_selections),
            current.ready_environments.clone(),
        );
        let servers = effective_mcp_servers(requested, current.auth.as_ref());
        let auth = compute_auth_statuses(
            servers.iter(),
            requested.mcp_oauth_credentials_store_mode,
            requested.auth_keyring_backend_kind,
            current.auth.as_ref(),
            &context,
        )
        .await;
        let mut statuses = current.connections.connection_statuses().await;
        let matching = statuses
            .keys()
            .filter(|name| match current.config.as_ref() {
                Some(published) => {
                    published
                        .mcp_server_catalog
                        .server(name)
                        .is_some_and(|server| {
                            requested.mcp_server_catalog.server(name) == Some(server)
                        })
                }
                None => false,
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

    /// Reuses the selected connection and catalog without starting unrelated servers.
    /// Configuration and metadata belong to the same captured runtime publication.
    pub async fn server_status_snapshot(
        &self,
        server: &str,
        detail: McpSnapshotDetail,
        runtime_context: &McpRuntimeContext,
    ) -> anyhow::Result<(Arc<McpConfig>, McpServerStatusSnapshot)> {
        let current = self.current.load_full();
        let runtime_context = runtime_context.clone().with_selected_environments(
            Arc::clone(&current.environment_selections),
            current.ready_environments.clone(),
        );
        let config = Arc::clone(
            current
                .config
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("MCP runtime is not configured"))?,
        );
        let mut servers = effective_mcp_servers(&config, current.auth.as_ref());
        servers.retain(|name, _| name == server);
        let auth_statuses = compute_auth_statuses(
            servers.iter(),
            config.mcp_oauth_credentials_store_mode,
            config.auth_keyring_backend_kind,
            current.auth.as_ref(),
            &runtime_context,
        )
        .await;
        let snapshot = collect_mcp_server_status_snapshot_from_manager(
            &current.connections,
            auth_statuses,
            servers.into_keys().collect(),
            detail,
        )
        .await;
        Ok((config, snapshot))
    }
}
