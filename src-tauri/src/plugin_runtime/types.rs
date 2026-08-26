use std::path::PathBuf;

use rmcp::model::CallToolResult;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeKey {
    pub plugin_slug: String,
    pub plugin_version: String,
    pub connector_key: String,
    pub workspace_key: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeLaunchSpec {
    pub key: RuntimeKey,
    pub runtime_kind: String,
    pub entrypoint: String,
    pub install_root: PathBuf,
    pub plugin_data_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub permission_revision: String,
    pub expected_tools: Vec<ExpectedTool>,
    pub resource_uris: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExpectedTool {
    pub name: String,
    pub schema_path: String,
}

#[derive(Clone)]
pub struct PluginCallContext {
    pub plugin_slug: String,
    pub capability_id: String,
    pub workspace_key: String,
    pub workspace_dir: PathBuf,
    pub agent_type: crate::models::AgentType,
    pub host_gateway_supported: bool,
    pub cancellation: CancellationToken,
    pub authority_cancellation: CancellationToken,
    pub permission_revision: String,
}

pub struct PluginToolCall {
    pub context: PluginCallContext,
    pub arguments: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone)]
pub struct PluginInvokeError {
    pub code: &'static str,
    pub message: String,
    pub effect_may_have_occurred: bool,
}

impl PluginInvokeError {
    pub fn before_effect(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            effect_may_have_occurred: false,
        }
    }

    pub fn after_dispatch(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            effect_may_have_occurred: true,
        }
    }
}

pub type PluginInvokeResult = Result<CallToolResult, PluginInvokeError>;
