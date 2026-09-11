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
    let ((server_infos, tools, tools_errors), resources, resource_templates) = tokio::join!(
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
        server_infos,
        tools_by_server,
        tools_errors,
        resources: convert_mcp_resources(resources),
        resource_templates: convert_mcp_resource_templates(resource_templates),
        auth_statuses: auth_statuses_from_entries(&auth),
        server_names,
    }
}
