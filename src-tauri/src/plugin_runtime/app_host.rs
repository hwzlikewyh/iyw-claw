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
    pub fn renew(&self, input: PluginAppLeaseInput) -> Result<PluginAppLaunch, PluginInvokeError> {
        validate_resource_uri(&input.resource_uri)?;
        if !matches!(input.display_mode.as_str(), "inline" | "fullscreen") {
            return Err(PluginInvokeError::before_effect(
                "plugin_app_invalid",
                "Unsupported plugin app display mode",
            ));
        }
        let nonce = random_token();
        let launch = PluginAppLaunch {
            instance_id: input.instance_id.clone(),
            conversation_id: input.conversation_id,
            tool_call_id: input.tool_call_id,
            plugin_slug: input.plugin_slug,
            plugin_version: input.plugin_version,
            app_key: input.app_key,
            resource_uri: input.resource_uri,
            display_mode: input.display_mode,
            launch_payload: input.launch_payload,
            lease_token: random_token(),
            nonce: nonce.clone(),
        };
        self.leases
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                input.instance_id,
                Lease {
                    launch: launch.clone(),
                    nonce,
                    expires_at: Instant::now() + LEASE_TTL,
                },
            );
        Ok(launch)
    }

    pub async fn create_persisted(
        &self,
        conn: &sea_orm::DatabaseConnection,
        input: PluginAppLaunchInput,
    ) -> Result<PluginAppLaunch, PluginInvokeError> {
        let workspace_key = input.workspace_key.clone();
        let launch = self.renew(PluginAppLeaseInput {
            instance_id: uuid::Uuid::new_v4().to_string(),
            conversation_id: input.conversation_id,
            tool_call_id: input.tool_call_id,
            plugin_slug: input.plugin_slug,
            plugin_version: input.plugin_version,
            app_key: input.app_key,
            resource_uri: input.resource_uri,
            display_mode: input.display_mode,
            launch_payload: input.launch_payload,
        })?;
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
                workspace_key,
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

    pub fn teardown_plugin(&self, plugin_slug: &str) -> usize {
        self.teardown_plugin_version(plugin_slug, None)
    }

    pub fn teardown_plugin_version(
        &self,
        plugin_slug: &str,
        plugin_version: Option<&str>,
    ) -> usize {
        let mut leases = self
            .leases
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = leases.len();
        leases.retain(|_, lease| {
            lease.launch.plugin_slug != plugin_slug
                || plugin_version.is_some_and(|version| lease.launch.plugin_version != version)
        });
        before.saturating_sub(leases.len())
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

pub struct PluginAppLeaseInput {
    pub instance_id: String,
    pub conversation_id: i64,
    pub tool_call_id: String,
    pub plugin_slug: String,
    pub plugin_version: String,
    pub app_key: String,
    pub resource_uri: String,
    pub display_mode: String,
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
        "initialize"
            | "ui/initialize"
            | "ui/open-link"
            | "ui/message"
            | "ui/request-display-mode"
            | "ui/update-model-context"
            | "ui/resource-teardown"
            | "tools/call"
            | "resources/list"
            | "resources/read"
            | "notifications/message"
            | "ping"
            | "ui/notifications/initialized"
            | "ui/notifications/size-changed"
            | "ui/notifications/sandbox-proxy-ready"
            | "ui/notifications/request-teardown"
    )
}
