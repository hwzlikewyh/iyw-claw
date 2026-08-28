use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rmcp::model::{CallToolResult, Meta};

use super::types::{PluginAppIntent, PluginInvokeError};

pub const TICKET_META_KEY: &str = "iyw-claw.plugin-app-ticket";
const TICKET_TTL: Duration = Duration::from_secs(5 * 60);

struct PendingLaunch {
    intent: PluginAppIntent,
    expires_at: Instant,
}

#[derive(Clone, Default)]
pub struct PluginAppLaunchBroker {
    pending: Arc<Mutex<BTreeMap<String, PendingLaunch>>>,
}

impl PluginAppLaunchBroker {
    pub fn issue(&self, intent: PluginAppIntent) -> String {
        let ticket = uuid::Uuid::new_v4().to_string();
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.retain(|_, launch| launch.expires_at > Instant::now());
        pending.insert(
            ticket.clone(),
            PendingLaunch {
                intent,
                expires_at: Instant::now() + TICKET_TTL,
            },
        );
        ticket
    }

    pub fn claim(
        &self,
        ticket: &str,
        connection_id: &str,
        workspace_key: &str,
    ) -> Result<PluginAppIntent, PluginInvokeError> {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(ticket)
            .ok_or_else(|| invalid_ticket("Plugin app ticket is missing or expired"))?;
        if pending.expires_at <= Instant::now()
            || pending.intent.connection_id != connection_id
            || pending.intent.workspace_key != workspace_key
        {
            return Err(invalid_ticket("Plugin app ticket identity does not match"));
        }
        Ok(pending.intent)
    }

    pub fn cancel_plugin(&self, plugin_slug: &str) -> usize {
        self.cancel_plugin_version(plugin_slug, None)
    }

    pub fn cancel_plugin_version(&self, plugin_slug: &str, plugin_version: Option<&str>) -> usize {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = pending.len();
        pending.retain(|_, launch| {
            launch.intent.plugin_slug != plugin_slug
                || plugin_version.is_some_and(|version| launch.intent.plugin_version != version)
        });
        before.saturating_sub(pending.len())
    }
}

pub fn attach_ticket(result: &mut CallToolResult, ticket: String) {
    let mut meta = result.meta.take().unwrap_or_else(Meta::new);
    meta.insert(TICKET_META_KEY.to_string(), serde_json::json!(ticket));
    result.meta = Some(meta);
}

fn invalid_ticket(message: impl Into<String>) -> PluginInvokeError {
    PluginInvokeError::before_effect("plugin_app_ticket_invalid", message)
}
