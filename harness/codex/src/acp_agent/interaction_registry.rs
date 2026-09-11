use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sacp::{Client, ConnectionTo, UntypedMessage};
use serde_json::json;

#[derive(Clone, Default)]
pub(super) struct InteractionRegistry {
    requests: Arc<Mutex<HashMap<String, RegisteredInteraction>>>,
    form_gate: Arc<tokio::sync::Mutex<()>>,
}

pub(super) struct RegisteredInteraction {
    pub session: String,
    pub key: String,
    pub thread: Option<String>,
    pub turn: Option<String>,
}

pub(super) struct InteractionLease {
    registry: InteractionRegistry,
    id: String,
}

impl InteractionRegistry {
    pub(super) fn cancel_session(
        &self,
        cx: &ConnectionTo<Client>,
        session: &str,
    ) -> Result<(), sacp::Error> {
        let ids: Vec<_> = self
            .requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|(_, request)| request.session == session)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.resolved(cx, &id)?;
        }
        Ok(())
    }
    pub(super) fn form_gate(&self) -> Arc<tokio::sync::Mutex<()>> {
        self.form_gate.clone()
    }

    pub(super) fn register(&self, id: String, wire: RegisteredInteraction) -> InteractionLease {
        self.requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id.clone(), wire);
        InteractionLease {
            registry: self.clone(),
            id,
        }
    }

    pub(super) fn resolved(&self, cx: &ConnectionTo<Client>, id: &str) -> Result<(), sacp::Error> {
        let request = self
            .requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(id);
        if let Some(RegisteredInteraction { session, key, .. }) = request {
            cx.send_notification_to(
                Client,
                UntypedMessage::new(
                    "_iyw/elicitation/cancel",
                    json!({
                        "sessionId": session, "requestKey": key,
                    }),
                )?,
            )?;
        }
        Ok(())
    }

    pub(super) fn complete_turn(
        &self,
        cx: &ConnectionTo<Client>,
        params: &serde_json::Value,
    ) -> Result<(), sacp::Error> {
        let (Some(thread), Some(turn)) = (
            params["threadId"].as_str(),
            params
                .pointer("/turn/id")
                .and_then(serde_json::Value::as_str),
        ) else {
            return Ok(());
        };
        let ids: Vec<_> = self
            .requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|(_, request)| {
                request.thread.as_deref() == Some(thread) && request.turn.as_deref() == Some(turn)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.resolved(cx, &id)?;
        }
        Ok(())
    }
}

impl InteractionLease {
    pub(super) fn is_active(&self) -> bool {
        self.registry
            .requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains_key(&self.id)
    }
}

impl Drop for InteractionLease {
    fn drop(&mut self) {
        self.registry
            .requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.id);
    }
}
