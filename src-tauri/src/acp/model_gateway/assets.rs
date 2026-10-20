use std::{collections::HashMap, sync::Mutex, time::Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::io::ReaderStream;

use super::{
    error::GatewayError,
    limits::{MAX_CACHE_ENTRIES, METADATA_BYTES, PREPARE_TIMEOUT, URL_REUSE},
    media::Prepared,
    target, Runtime,
};

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Asset {
    pub image_id: String,
    pub image_url: String,
    pub sha256: String,
    pub mime_type: String,
    pub size_bytes: u64,
    #[serde(default = "yes")]
    pub supported: bool,
    #[serde(default = "yes")]
    pub found: bool,
}

fn yes() -> bool {
    true
}

#[derive(Default)]
pub(super) struct AssetCache(Mutex<HashMap<[u8; 32], (Instant, Asset)>>);

impl AssetCache {
    pub fn key(token: &str, encoded: &[u8; 32]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(token.as_bytes());
        hash.update(encoded);
        hash.finalize().into()
    }

    pub fn get(&self, key: &[u8; 32]) -> Option<Asset> {
        self.0
            .lock()
            .ok()?
            .get(key)
            .filter(|(created, _)| created.elapsed() < URL_REUSE)
            .map(|(_, asset)| asset.clone())
    }

    fn insert(&self, key: [u8; 32], asset: Asset) {
        let Ok(mut cache) = self.0.lock() else {
            return;
        };
        if cache.len() >= MAX_CACHE_ENTRIES {
            if let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, (created, _))| *created)
                .map(|(key, _)| *key)
            {
                cache.remove(&oldest);
            }
        }
        cache.insert(key, (Instant::now(), asset));
    }
}

#[derive(Deserialize)]
struct Ticket {
    found: bool,
    upload: Option<Upload>,
}

#[derive(Deserialize)]
struct Upload {
    url: String,
    headers: HashMap<String, String>,
}

impl Runtime {
    pub(super) async fn upload(
        &self,
        (token, model): (&str, &str),
        prepared: Prepared,
    ) -> Result<Asset, GatewayError> {
        let started = Instant::now();
        let key = AssetCache::key(token, &prepared.encoded_digest);
        if let Some(asset) = self.cache.get(&key) {
            return Ok(asset);
        }
        let payload = serde_json::json!({"operation":"init", "model":model,
            "sha256":prepared.digest, "mime_type":prepared.mime, "size_bytes":prepared.size});
        let value = self.image_api(token, &payload).await?;
        let ticket: Ticket = serde_json::from_value(value.clone())
            .map_err(|_| GatewayError::unavailable("Invalid image upload ticket"))?;
        let value = if ticket.found {
            value
        } else {
            let upload = ticket
                .upload
                .ok_or_else(|| GatewayError::unavailable("Image upload ticket is missing"))?;
            self.put_image(upload, prepared.file, prepared.size).await?;
            let mut payload = payload;
            payload["operation"] = "complete".into();
            self.image_api(token, &payload).await?
        };
        let asset: Asset = serde_json::from_value(value)
            .map_err(|_| GatewayError::unavailable("Image resource response is invalid"))?;
        if !asset.supported
            || !asset.found
            || asset.sha256 != prepared.digest
            || asset.mime_type != prepared.mime
            || asset.size_bytes != prepared.size
            || !asset.image_id.starts_with("iyw-image-v1-")
            || !asset.image_url.starts_with("https://")
        {
            return Err(GatewayError::unavailable(
                "Image resource identity does not match",
            ));
        }
        self.cache.insert(key, asset.clone());
        tracing::info!(target: "model_gateway", image_bytes = prepared.size,
            storage_upload = !ticket.found, elapsed_ms = started.elapsed().as_millis(),
            "local image resource prepared");
        Ok(asset)
    }

    async fn put_image(
        &self,
        upload: Upload,
        file: std::fs::File,
        size: u64,
    ) -> Result<(), GatewayError> {
        let url = reqwest::Url::parse(&upload.url)
            .map_err(|_| GatewayError::unavailable("Invalid storage upload URL"))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(GatewayError::unavailable(
                "Storage upload requires credential-free HTTPS",
            ));
        }
        let mut request = self
            .client
            .put(url)
            .timeout(PREPARE_TIMEOUT)
            .header("content-length", size)
            .body(reqwest::Body::wrap_stream(ReaderStream::new(
                tokio::fs::File::from_std(file),
            )));
        for (name, value) in upload.headers {
            let lower = name.to_ascii_lowercase();
            if lower == "content-type" || lower.starts_with("x-tos-") {
                request = request.header(name, value);
            }
        }
        let response = request
            .send()
            .await
            .map_err(|_| GatewayError::unavailable("Image direct upload failed"))?;
        if !response.status().is_success() && response.status().as_u16() != 409 {
            return Err(GatewayError::unavailable(
                "Image storage rejected the upload",
            ));
        }
        Ok(())
    }

    async fn image_api(
        &self,
        token: &str,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, GatewayError> {
        super::auth::check(token)?;
        let url = target::resolve(&self.upstream, "/v1/images/input/prepare")?;
        let response = self
            .client
            .post(url)
            .header("token", token)
            .timeout(PREPARE_TIMEOUT)
            .json(payload)
            .send()
            .await
            .map_err(|_| GatewayError::unavailable("Image resource service is unavailable"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(GatewayError {
                status,
                code: "image_resource_preparation_failed",
                message: "Cloud image preparation failed before model execution",
            });
        }
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| {
                GatewayError::unavailable("Image resource response was interrupted")
            })?;
            if body.len() + chunk.len() > METADATA_BYTES {
                return Err(GatewayError::unavailable("Image metadata exceeds limit"));
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body)
            .map_err(|_| GatewayError::unavailable("Image resource response is not JSON"))
    }
}
