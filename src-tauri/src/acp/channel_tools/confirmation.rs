use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingChannelConfirmationState {
    pub confirmation_id: String,
    pub action: String,
    pub channel_id: i32,
    pub channel_name: String,
    pub channel_type: String,
    pub enabled: bool,
    pub local_record_count: u64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ChannelConfirmationSpec {
    pub state: PendingChannelConfirmationState,
    pub resource_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelConfirmationOutcome {
    pub confirmed: bool,
}

pub struct RegisteredChannelConfirmation {
    pub confirmation_id: String,
    pub outcome_rx: oneshot::Receiver<ChannelConfirmationOutcome>,
}

#[async_trait]
pub trait SessionChannelConfirmationAccess: Send + Sync {
    async fn register_channel_confirmation(
        &self,
        parent_connection_id: &str,
        state: PendingChannelConfirmationState,
    ) -> Option<RegisteredChannelConfirmation>;

    async fn cancel_channel_confirmation(&self, parent_connection_id: &str, confirmation_id: &str);

    async fn cancel_channel_confirmations_by_parent(&self, parent_connection_id: &str);
}
