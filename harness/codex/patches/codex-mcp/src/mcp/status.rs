use std::collections::{HashMap, HashSet};

use super::{
    McpAuthStatusEntry, McpConnectionSet, McpServerStatusSnapshot, McpSnapshotDetail,
    auth_statuses_from_entries, convert_mcp_resource_templates, convert_mcp_resources,
    protocol_tool_from_rmcp_tool,
};

pub(crate) async fn collect_published_status(
    connections: &McpConnectionSet,
    auth: HashMap<String, McpAuthStatusEntry>,
    selection: (HashSet<String>, Vec<String>, McpSnapshotDetail),
) -> McpServerStatusSnapshot {
    let (matching, server_names, detail) = selection;
    let include = |name: &str| matching.contains(name) && connections.has_ready_client(name);
    let ((server_infos, tools, tools_errors, tool_list_meta), resources, resource_templates) = tokio::join!(
        connections.inspect_catalog(&matching),
        async {
            if detail.include_resources() {
                connections.list_all_resources(include).await
            } else {
                HashMap::new()
            }
        },
        async {
            if detail.include_resources() {
                connections.list_all_resource_templates(include).await
            } else {
                HashMap::new()
            }
        },
    );
    let mut tools_by_server = HashMap::<String, HashMap<String, codex_protocol::mcp::Tool>>::new();
    for info in tools {
        if let Some(tool) = protocol_tool_from_rmcp_tool(info.tool.name.as_ref(), &info.tool) {
            tools_by_server
                .entry(info.server_name)
                .or_default()
                .insert(tool.name.clone(), tool);
        }
    }
    McpServerStatusSnapshot {
        server_capabilities: connections.list_available_server_capabilities(include),
        tool_list_meta,
        server_infos,
        tools_by_server,
        tools_errors,
        resources: convert_mcp_resources(resources),
        resource_templates: convert_mcp_resource_templates(resource_templates),
        auth_statuses: published_auth_statuses(connections, &auth).await,
        server_names,
    }
}

async fn published_auth_statuses(
    connections: &McpConnectionSet,
    auth: &HashMap<String, McpAuthStatusEntry>,
) -> HashMap<String, codex_protocol::mcp::McpAuthStatus> {
    use codex_config::McpServerAuth;
    use codex_protocol::mcp::McpAuthStatus;

    let mut statuses = auth_statuses_from_entries(auth);
    // 已失败的连接不能仅凭本地凭据仍存在就显示为已登录。
    for name in connections.authentication_failed_servers().await {
        if statuses.get(&name) == Some(&McpAuthStatus::OAuth)
            || auth.get(&name).and_then(|entry| entry.config.as_ref())
                .is_some_and(|config| matches!(config.auth, McpServerAuth::EmaAuth))
        {
            statuses.insert(name, McpAuthStatus::NotLoggedIn);
        }
    }
    statuses
}
