use std::sync::Arc;

use crate::acp::channel_tools::confirmation::{
    ChannelConfirmationOutcome, PendingChannelConfirmationState, RegisteredChannelConfirmation,
    SessionChannelConfirmationAccess,
};
use crate::acp::manager::{ConnectionManager, PendingChannelConfirmationEntry};
use crate::acp::types::AcpEvent;
use crate::web::event_bridge::emit_with_state;

impl ConnectionManager {
    pub async fn register_channel_confirmation(
        &self,
        conn_id: &str,
        state_value: PendingChannelConfirmationState,
    ) -> Option<RegisteredChannelConfirmation> {
        let (state, emitter) = self.get_state_and_emitter(conn_id).await?;
        let confirmation_id = state_value.confirmation_id.clone();
        let (sender, outcome_rx) = tokio::sync::oneshot::channel();
        {
            let mut registry = self.pending_channel_confirmations.lock().await;
            if registry
                .values()
                .any(|entry| entry.parent_connection_id == conn_id)
            {
                return None;
            }
            registry.insert(
                confirmation_id.clone(),
                PendingChannelConfirmationEntry {
                    parent_connection_id: conn_id.to_string(),
                    sender,
                },
            );
        }
        emit_with_state(
            &state,
            &emitter,
            AcpEvent::ChannelConfirmationRequested {
                confirmation: state_value,
            },
        )
        .await;
        if !self
            .pending_channel_confirmations
            .lock()
            .await
            .contains_key(&confirmation_id)
        {
            emit_with_state(
                &state,
                &emitter,
                AcpEvent::ChannelConfirmationResolved {
                    confirmation_id: confirmation_id.clone(),
                },
            )
            .await;
            return None;
        }
        Some(RegisteredChannelConfirmation {
            confirmation_id,
            outcome_rx,
        })
    }

    pub async fn respond_channel_confirmation(
        &self,
        connection_id: &str,
        confirmation_id: &str,
        confirmed: bool,
    ) {
        let entry = {
            let mut registry = self.pending_channel_confirmations.lock().await;
            let belongs_to_connection = registry
                .get(confirmation_id)
                .is_some_and(|entry| entry.parent_connection_id == connection_id);
            belongs_to_connection
                .then(|| registry.remove(confirmation_id))
                .flatten()
        };
        let Some(entry) = entry else { return };
        let _ = entry.sender.send(ChannelConfirmationOutcome { confirmed });
        self.emit_channel_confirmation_resolved(&entry.parent_connection_id, confirmation_id)
            .await;
    }

    pub async fn cancel_channel_confirmation(&self, confirmation_id: &str) {
        let entry = self
            .pending_channel_confirmations
            .lock()
            .await
            .remove(confirmation_id);
        let Some(entry) = entry else { return };
        self.emit_channel_confirmation_resolved(&entry.parent_connection_id, confirmation_id)
            .await;
    }

    pub async fn cancel_channel_confirmations_by_parent(&self, conn_id: &str) {
        let ids = {
            let mut registry = self.pending_channel_confirmations.lock().await;
            let ids = registry
                .iter()
                .filter(|(_, entry)| entry.parent_connection_id == conn_id)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            for id in &ids {
                registry.remove(id);
            }
            ids
        };
        for id in ids {
            self.emit_channel_confirmation_resolved(conn_id, &id).await;
        }
    }

    async fn emit_channel_confirmation_resolved(&self, conn_id: &str, confirmation_id: &str) {
        if let Some((state, emitter)) = self.get_state_and_emitter(conn_id).await {
            emit_with_state(
                &state,
                &emitter,
                AcpEvent::ChannelConfirmationResolved {
                    confirmation_id: confirmation_id.to_string(),
                },
            )
            .await;
        }
    }
}

#[derive(Clone)]
pub struct ConnectionManagerChannelConfirmationLookup {
    pub manager: Arc<ConnectionManager>,
}

#[async_trait::async_trait]
impl SessionChannelConfirmationAccess for ConnectionManagerChannelConfirmationLookup {
    async fn register_channel_confirmation(
        &self,
        parent_connection_id: &str,
        state: PendingChannelConfirmationState,
    ) -> Option<RegisteredChannelConfirmation> {
        self.manager
            .register_channel_confirmation(parent_connection_id, state)
            .await
    }

    async fn cancel_channel_confirmation(&self, _: &str, confirmation_id: &str) {
        self.manager
            .cancel_channel_confirmation(confirmation_id)
            .await;
    }

    async fn cancel_channel_confirmations_by_parent(&self, parent_connection_id: &str) {
        self.manager
            .cancel_channel_confirmations_by_parent(parent_connection_id)
            .await;
    }
}
