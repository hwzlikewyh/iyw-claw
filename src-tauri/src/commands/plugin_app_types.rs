use serde::{Deserialize, Serialize};

use super::PluginAppResourceMeta;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppOpenRequest {
    pub instance_id: String,
    pub conversation_id: i64,
    #[serde(default)]
    pub display_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppMessageRequest {
    pub instance_id: String,
    pub lease_token: String,
    pub nonce: String,
    pub method: String,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppTeardownRequest {
    pub instance_id: String,
    #[serde(default)]
    pub conversation_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppOpenResponse {
    pub launch: crate::plugin_runtime::app_host::PluginAppLaunch,
    pub html: String,
    pub host_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_meta: Option<PluginAppResourceMeta>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppErrorState {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppMessageResponse {
    pub accepted: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<PluginAppRpcError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppRpcError {
    pub code: i32,
    pub message: String,
}
