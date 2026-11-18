use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::spool::Spool;

const RETRY_AFTER: Duration = Duration::from_secs(30);
const MAX_ENTRIES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
static CATALOG_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn catalog_refreshed() {
    CATALOG_GENERATION.fetch_add(1, Ordering::Relaxed);
}

#[derive(Hash, Eq, PartialEq)]
pub(super) struct CountKey {
    account: [u8; 32],
    model: String,
    anthropic_route: bool,
    generation: u64,
}

impl CountKey {
    pub(super) fn from_request(body: &Spool, token: &str, path: &str) -> Option<Self> {
        // 常规计数请求留在内存；大包沿原流式路径发送，不为识别模型重读临时文件。
        let Spool::Memory(bytes, _) = body else {
            return None;
        };
        #[derive(Deserialize)]
        struct CountRequest<'a> {
            model: &'a str,
            #[serde(default)]
            stream: bool,
            #[serde(rename = "messages")]
            _messages: serde::de::IgnoredAny,
        }
        let request: CountRequest<'_> = serde_json::from_slice(bytes).ok()?;
        if request.stream || request.model.is_empty() || request.model.len() > MAX_MODEL_BYTES {
            return None;
        }
        Some(Self {
            account: Sha256::digest(token.as_bytes()).into(),
            model: request.model.to_owned(),
            anthropic_route: path.starts_with("/anthropic/"),
            generation: CATALOG_GENERATION.load(Ordering::Relaxed),
        })
    }
}

#[derive(Default)]
pub(super) struct CountCache(Mutex<HashMap<CountKey, Instant>>);

impl CountCache {
    pub(super) fn rejection(&self, key: &CountKey) -> Option<Response> {
        let expires = *self.0.lock().ok()?.get(key)?;
        if expires <= Instant::now() || key.generation != CATALOG_GENERATION.load(Ordering::Relaxed)
        {
            return None;
        }
        let code = "unsupported_protocol_feature";
        let payload = serde_json::json!({"type":"error","error":{"type":"relay_error","code":code,"message":code}});
        Some((axum::http::StatusCode::BAD_REQUEST, axum::Json(payload)).into_response())
    }

    pub(super) fn observe(&self, key: CountKey, response: &reqwest::Response) {
        if response.status() != reqwest::StatusCode::BAD_REQUEST
            || !response
                .headers()
                .get("x-iyw-exact-count-tokens")
                .is_some_and(|v| v == "unsupported")
        {
            return;
        }
        if let Ok(mut entries) = self.0.lock() {
            if key.generation != CATALOG_GENERATION.load(Ordering::Relaxed) {
                return;
            }
            let now = Instant::now();
            entries.retain(|k, expires| k.generation == key.generation && *expires > now);
            if entries.len() < MAX_ENTRIES {
                entries.insert(key, now + RETRY_AFTER);
            }
        }
    }
}
