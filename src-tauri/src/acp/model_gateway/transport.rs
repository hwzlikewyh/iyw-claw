use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{assets::Asset, Runtime};

const POLICY_TTL: Duration = Duration::from_secs(30);
const POLICY_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_POLICIES: usize = 128;

#[derive(Default)]
pub(super) struct TransportCache(Mutex<HashMap<[u8; 32], (Instant, bool)>>);

impl TransportCache {
    fn get(&self, key: &[u8; 32]) -> Option<bool> {
        self.0
            .try_lock()
            .ok()?
            .get(key)
            .filter(|value| value.0.elapsed() < POLICY_TTL)
            .map(|value| value.1)
    }

    fn insert(&self, key: [u8; 32], allowed: bool) {
        let Ok(mut cache) = self.0.try_lock() else {
            return;
        };
        if cache.len() >= MAX_POLICIES {
            if let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(key, _)| *key)
            {
                cache.remove(&oldest);
            }
        }
        cache.insert(key, (Instant::now(), allowed));
    }
}

#[derive(Deserialize)]
struct Resources {
    url_transport: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

impl Runtime {
    pub(super) async fn local_url_transport(
        &self,
        token: &str,
        (model, path): (&str, &str),
        ids: &[String],
    ) -> bool {
        let endpoint = endpoint(path);
        if endpoint.is_empty() {
            return false;
        }
        let key: [u8; 32] =
            Sha256::digest(format!("{token}\0{model}\0{endpoint}").as_bytes()).into();
        let cached = self.transport.get(&key);
        if let Some(allowed) = cached {
            if !allowed || ids.is_empty() {
                return allowed;
            }
        }
        let Ok(_permit) = self.images.try_acquire() else {
            return cached.unwrap_or(false);
        };
        // 冷启动将策略与缺失 ID 合为一次请求；热命中完全在本机完成。
        let Some(allowed) = self.image_resources(token, (model, endpoint), ids).await else {
            // 批量解析暂时失败不等于渠道不支持 URL；复用未过期策略和已解析的本地资源。
            let retained = self.transport.get(&key);
            tracing::debug!(target: "model_gateway", model, endpoint, resource_count=ids.len(),
                cached_url_transport=retained, "optional image resolution unavailable; retaining valid local policy");
            return retained.unwrap_or(false);
        };
        self.transport.insert(key, allowed);
        if cached != Some(allowed) {
            tracing::info!(target: "model_gateway", model, endpoint, url_transport=allowed,
                resource_count=ids.len(), "local image transport policy confirmed");
        }
        allowed
    }

    pub(super) async fn image_resources(
        &self,
        token: &str,
        (model, endpoint): (&str, &str),
        ids: &[String],
    ) -> Option<bool> {
        if ids.len() > super::limits::MAX_IMAGES {
            return None;
        }
        let payload = serde_json::json!({"operation":if ids.is_empty() {"transport"} else {"resolve_batch"},
            "model":model,"endpoint":endpoint,"image_ids":ids});
        let value = self.optional_image_resources(token, &payload).await?;
        let result: Resources = serde_json::from_value(value).ok()?;
        if result.assets.len() != if result.url_transport { ids.len() } else { 0 } {
            return None;
        }
        let mut seen = HashSet::new();
        for asset in &result.assets {
            if !ids.contains(&asset.image_id)
                || !seen.insert(&asset.image_id)
                || !valid_asset(asset)
            {
                return None;
            }
        }
        for asset in result.assets {
            self.cache.insert_reference(token, asset);
        }
        Some(result.url_transport)
    }

    async fn optional_image_resources(
        &self,
        token: &str,
        payload: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        match tokio::time::timeout(POLICY_TIMEOUT, self.image_api_once(token, payload)).await {
            Ok(Ok(value)) => Some(value),
            Ok(Err(error)) => {
                tracing::debug!(target: "model_gateway", status=%error.status, code=error.code,
                    reason=error.message, "optional image resource query failed");
                None
            }
            Err(_) => {
                tracing::debug!(target: "model_gateway", "optional image resource query timed out");
                None
            }
        }
    }
}

fn endpoint(path: &str) -> &'static str {
    if path.ends_with("/images/edits") {
        "images_edits"
    } else if path.ends_with("/responses/compact") {
        "responses_compact"
    } else if path.ends_with("/messages/count_tokens") {
        "messages_count_tokens"
    } else if path.ends_with("/messages") {
        "messages"
    } else if path.ends_with("/chat/completions") {
        "chat_completions"
    } else if path.ends_with("/responses") {
        "responses"
    } else {
        ""
    }
}

pub(super) fn valid_asset(asset: &Asset) -> bool {
    let Some(identity) = asset.image_id.strip_prefix("iyw-image-v1-") else {
        return false;
    };
    let parts: Vec<_> = identity.split('.').collect();
    if parts.len() != 3 || !digest(parts[0]) || !digest(parts[1]) {
        return false;
    }
    let extension = match asset.mime_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => return false,
    };
    let url = reqwest::Url::parse(&asset.image_url).ok();
    asset.supported
        && asset.found
        && parts[2] == extension
        && parts[1] == asset.sha256
        && asset.size_bytes > 0
        && asset.size_bytes <= super::limits::MAX_IMAGE_BYTES
        && url.is_some_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
        })
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
