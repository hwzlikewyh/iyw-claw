use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use codex_protocol::mcp::McpServerInfo;

use super::McpConnectionSet;
use crate::rmcp_client::{prepare_codex_apps_tools_for_model, prepare_regular_mcp_tools_for_model};
use crate::tools::ToolInfo;
use crate::tools::filter_tools;

impl McpConnectionSet {
    pub(crate) async fn inspect_catalog(
        &self,
        matching: &HashSet<String>,
    ) -> (
        HashMap<String, McpServerInfo>,
        Vec<ToolInfo>,
        HashMap<String, String>,
        HashMap<String, Arc<serde_json::Value>>,
    ) {
        let mut infos = HashMap::new();
        let mut tools = Vec::new();
        let mut errors = HashMap::new();
        let mut tool_list_meta = HashMap::new();
        for (name, view) in self
            .servers
            .iter()
            .filter(|(name, _)| matching.contains(*name))
        {
            let client = &view.connection.client;
            if let Some(ready) = client.ready_client() {
                infos.insert(name.clone(), ready.server_info.clone());
                let (catalog, meta) = ready.listed_catalog().await;
                if let Some(meta) = meta {
                    tool_list_meta.insert(name.clone(), meta);
                }
                let catalog = filter_tools(catalog, &view.tool_filter);
                tools.extend(if client.is_codex_apps_mcp_server {
                    prepare_codex_apps_tools_for_model(catalog, &self.tool_plugin_context)
                } else {
                    prepare_regular_mcp_tools_for_model(catalog, &self.tool_plugin_context)
                });
            } else if let Some(Err(error)) = client.client.peek() {
                errors.insert(name.clone(), error.to_string());
            }
        }
        (infos, tools, errors, tool_list_meta)
    }

    pub(crate) fn has_ready_client(&self, name: &str) -> bool {
        self.servers
            .get(name)
            .is_some_and(|view| view.connection.client.ready_client().is_some())
    }
}
