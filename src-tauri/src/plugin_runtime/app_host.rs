use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use base64::Engine;
use rand::RngCore;

use super::types::PluginInvokeError;

const LEASE_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_MESSAGE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppLaunch {
    pub instance_id: String,
    pub conversation_id: i64,
    pub tool_call_id: String,
    pub plugin_slug: String,
    pub plugin_version: String,
    pub app_key: String,
    pub resource_uri: String,
    pub display_mode: String,
    pub launch_payload: serde_json::Value,
    pub lease_token: String,
    pub nonce: String,
}

#[derive(Debug, Clone)]
struct Lease {
    launch: PluginAppLaunch,
    nonce: String,
    expires_at: Instant,
}

#[derive(Clone, Default)]
pub struct PluginAppRegistry {
    leases: Arc<RwLock<BTreeMap<String, Lease>>>,
}

impl PluginAppRegistry {
    pub async fn create_persisted(
        &self,
        conn: &sea_orm::DatabaseConnection,
        input: PluginAppLaunchInput,
    ) -> Result<PluginAppLaunch, PluginInvokeError> {
        let launch = self.create(
            input.conversation_id,
            input.tool_call_id.clone(),
            input.plugin_slug.clone(),
            input.plugin_version.clone(),
            input.app_key.clone(),
            input.resource_uri,
            input.display_mode,
            input.launch_payload,
        )?;
        let payload = serde_json::to_string(&launch.launch_payload).map_err(|error| {
            PluginInvokeError::before_effect("plugin_app_invalid", error.to_string())
        })?;
        let stored = crate::db::service::plugin_app_instance_service::upsert(
            conn,
            crate::db::service::plugin_app_instance_service::PluginAppInstanceInput {
                instance_id: launch.instance_id.clone(),
                conversation_id: launch.conversation_id,
                tool_call_id: launch.tool_call_id.clone(),
                plugin_slug: launch.plugin_slug.clone(),
                plugin_version: launch.plugin_version.clone(),
                app_key: launch.app_key.clone(),
                workspace_key: input.workspace_key,
                launch_payload_json: payload,
                state: "active".to_string(),
            },
        )
        .await
        .map_err(|error| {
            PluginInvokeError::before_effect("plugin_app_persist_failed", error.to_string())
        });
        if let Err(error) = stored {
            self.teardown(&launch.instance_id);
            return Err(error);
        }
        Ok(launch)
    }

    pub fn create(
        &self,
        conversation_id: i64,
        tool_call_id: String,
        plugin_slug: String,
        plugin_version: String,
        app_key: String,
        resource_uri: String,
        display_mode: String,
        launch_payload: serde_json::Value,
    ) -> Result<PluginAppLaunch, PluginInvokeError> {
        validate_resource_uri(&resource_uri)?;
        if !matches!(display_mode.as_str(), "inline" | "fullscreen") {
            return Err(PluginInvokeError::before_effect(
                "plugin_app_invalid",
                "Unsupported plugin app display mode",
            ));
        }
        let instance_id = uuid::Uuid::new_v4().to_string();
        let nonce = random_token();
        let launch = PluginAppLaunch {
            instance_id: instance_id.clone(),
            conversation_id,
            tool_call_id,
            plugin_slug,
            plugin_version,
            app_key,
            resource_uri,
            display_mode,
            launch_payload,
            lease_token: random_token(),
            nonce: nonce.clone(),
        };
        let lease = Lease {
            launch: launch.clone(),
            nonce,
            expires_at: Instant::now() + LEASE_TTL,
        };
        self.leases
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(instance_id, lease);
        Ok(launch)
    }

    pub fn authorize_message(
        &self,
        instance_id: &str,
        lease_token: &str,
        nonce: &str,
        method: &str,
        payload_bytes: usize,
    ) -> Result<(), PluginInvokeError> {
        if payload_bytes > MAX_MESSAGE_BYTES {
            return Err(PluginInvokeError::before_effect(
                "plugin_app_message_too_large",
                "Plugin app message exceeds the size limit",
            ));
        }
        let leases = self
            .leases
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let lease = leases.get(instance_id).ok_or_else(|| {
            PluginInvokeError::before_effect("plugin_app_lease_invalid", "Plugin app lease expired")
        })?;
        if lease.expires_at <= Instant::now()
            || lease.launch.lease_token != lease_token
            || lease.nonce != nonce
            || !allowed_method(method)
        {
            return Err(PluginInvokeError::before_effect(
                "plugin_app_message_unauthorized",
                "Plugin app message is not authorized",
            ));
        }
        Ok(())
    }

    pub fn teardown(&self, instance_id: &str) -> bool {
        self.leases
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(instance_id)
            .is_some()
    }

    pub fn reap_expired(&self) -> usize {
        let now = Instant::now();
        let mut leases = self
            .leases
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = leases.len();
        leases.retain(|_, lease| lease.expires_at > now);
        before.saturating_sub(leases.len())
    }
}

pub struct PluginAppLaunchInput {
    pub conversation_id: i64,
    pub tool_call_id: String,
    pub plugin_slug: String,
    pub plugin_version: String,
    pub app_key: String,
    pub resource_uri: String,
    pub display_mode: String,
    pub workspace_key: String,
    pub launch_payload: serde_json::Value,
}

fn validate_resource_uri(value: &str) -> Result<(), PluginInvokeError> {
    let Some(rest) = value.strip_prefix("ui://") else {
        return Err(PluginInvokeError::before_effect(
            "plugin_app_invalid",
            "Plugin app resource URI must use ui://",
        ));
    };
    if rest.is_empty()
        || rest.contains(['?', '#', '%', '\\'])
        || rest
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(PluginInvokeError::before_effect(
            "plugin_app_invalid",
            "Plugin app resource URI is unsafe",
        ));
    }
    Ok(())
}

fn random_token() -> String {
    let mut bytes = [0_u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn allowed_method(method: &str) -> bool {
    matches!(
        method,
        "ui/initialize" | "ui/message" | "ui/resize" | "ui/teardown"
    )
}
