use super::PluginMcpClient;
use crate::plugin_runtime::types::PluginInvokeError;
use rmcp::model::{CallToolRequestParams, CallToolResult};

impl PluginMcpClient {
    pub async fn call_tool(
        &self,
        name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<CallToolResult, PluginInvokeError> {
        self.call_tool_scoped(name, arguments, None).await
    }

    pub async fn call_tool_scoped(
        &self,
        name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        conversation_id: Option<i64>,
    ) -> Result<CallToolResult, PluginInvokeError> {
        let mut request = CallToolRequestParams::new(name).with_arguments(arguments);
        if let Some(id) = conversation_id.filter(|id| *id > 0) {
            let mut meta = rmcp::model::Meta::new();
            meta.insert(
                "iyw/conversationId".into(),
                serde_json::json!(id.to_string()),
            );
            request.meta = Some(meta);
        }
        self.peer.call_tool(request).await.map_err(|error| {
            PluginInvokeError::after_dispatch("plugin_call_failed", error.to_string())
        })
    }
}
