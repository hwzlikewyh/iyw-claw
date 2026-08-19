use std::collections::{hash_map::Entry, HashMap};

use tokio::sync::RwLock;

#[derive(Clone)]
pub(super) struct RuntimeCredential {
    broker_token: String,
}

impl RuntimeCredential {
    pub(super) fn new(broker_token: String) -> Self {
        Self { broker_token }
    }

    pub(super) fn broker_token(&self) -> &str {
        &self.broker_token
    }
}

#[derive(Default)]
pub(super) struct RuntimeRegistry {
    inner: RwLock<HashMap<String, RuntimeCredential>>,
}

impl RuntimeRegistry {
    pub(super) async fn insert_if_absent(
        &self,
        connection_id: String,
        credential: RuntimeCredential,
    ) -> bool {
        match self.inner.write().await.entry(connection_id) {
            Entry::Vacant(entry) => {
                entry.insert(credential);
                true
            }
            Entry::Occupied(_) => false,
        }
    }

    pub(super) async fn get(&self, connection_id: &str) -> Option<RuntimeCredential> {
        self.inner.read().await.get(connection_id).cloned()
    }

    pub(super) async fn remove(&self, connection_id: &str) -> Option<RuntimeCredential> {
        self.inner.write().await.remove(connection_id)
    }

    pub(super) async fn connection_ids(&self) -> Vec<String> {
        self.inner.read().await.keys().cloned().collect()
    }

    pub(super) async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}
