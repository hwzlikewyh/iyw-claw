use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;

const AUTHORIZATION_TTL_MINUTES: i64 = 10;

#[derive(Clone)]
pub struct AuthorizationRegistry {
    inner: Arc<Mutex<HashMap<String, AuthorizationEntry>>>,
}

#[derive(Clone)]
pub struct AuthorizationEntry {
    pub channel_id: i32,
    pub channel_type: String,
    pub provider_ref: String,
    pub qr_content: String,
    pub expires_at: DateTime<Utc>,
}

impl Default for AuthorizationRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl AuthorizationRegistry {
    pub async fn insert(
        &self,
        channel_id: i32,
        channel_type: &str,
        provider_ref: String,
        qr_content: String,
    ) -> (String, DateTime<Utc>) {
        let authorization_id = format!("ca_{}", uuid::Uuid::new_v4().simple());
        let expires_at = Utc::now() + Duration::minutes(AUTHORIZATION_TTL_MINUTES);
        self.inner.lock().await.insert(
            authorization_id.clone(),
            AuthorizationEntry {
                channel_id,
                channel_type: channel_type.to_string(),
                provider_ref,
                qr_content,
                expires_at,
            },
        );
        (authorization_id, expires_at)
    }

    pub async fn get(&self, authorization_id: &str) -> Option<AuthorizationEntry> {
        let mut entries = self.inner.lock().await;
        entries.retain(|_, entry| entry.expires_at > Utc::now());
        entries.get(authorization_id).cloned()
    }

    pub async fn remove(&self, authorization_id: &str) {
        self.inner.lock().await.remove(authorization_id);
    }
}
