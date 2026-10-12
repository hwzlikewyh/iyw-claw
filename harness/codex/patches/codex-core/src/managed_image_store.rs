//! 通过现有模型网关保存图片；未开启资源能力时保留原附件路径。
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use codex_attachment_store::{
    AttachmentMetadata, AttachmentStore, AttachmentStoreError, AttachmentStoreErrorKind,
    InlineAttachmentStore, ResolveFuture, ResolveRequest, UploadFuture, UploadRequest,
    UploadResult,
};
use codex_http_client::{HttpClient, HttpClientFactory};
use http::HeaderMap;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{
    ClientRedirectPolicy, ClientRouteClass, ClientRouting, ModelClient, create_client_for_route,
};

const IMAGE_API_PATH: &str = "/images/input/prepare";
pub(crate) const IMAGE_ID_PREFIX: &str = "iyw-image-v1-";
const IMAGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
const IMAGE_URL_LIFETIME: Duration = Duration::from_secs(3600);

#[derive(Deserialize)]
struct ImageResponse {
    #[serde(default)]
    supported: bool,
    #[serde(default)]
    found: bool,
    image_id: Option<String>,
    sha256: Option<String>,
    mime_type: Option<String>,
    size_bytes: Option<u64>,
    image_url: Option<String>,
    data: Option<String>,
}

struct ManagedImageStore {
    client: HttpClient,
    url: String,
    headers: HeaderMap,
    model: String,
    fallback: Arc<dyn AttachmentStore>,
}

impl ModelClient {
    pub(crate) fn uses_managed_image_provider(&self) -> bool {
        self.state.provider.info().name == "iyw-claw"
    }
    pub(crate) async fn managed_image_store(
        &self,
        model: &str,
        fallback: Arc<dyn AttachmentStore>,
    ) -> Arc<dyn AttachmentStore> {
        self.managed_image_backend(model, Arc::clone(&fallback))
            .await
            .map(|store| Arc::new(store) as Arc<dyn AttachmentStore>)
            .unwrap_or(fallback)
    }

    async fn managed_image_backend(
        &self,
        model: &str,
        fallback: Arc<dyn AttachmentStore>,
    ) -> Option<ManagedImageStore> {
        if !self.uses_managed_image_provider() {
            return None;
        }
        let setup = match self.current_client_setup(ClientRouting::Workspace).await {
            Ok(setup) => setup,
            Err(_) => return None,
        };
        let url = setup.api_provider.url_for_path(IMAGE_API_PATH);
        let client = match managed_http_client(&self.http_client_factory, &url) {
            Ok(client) => client,
            Err(_) => return None,
        };
        let mut headers = setup.api_provider.headers.clone();
        setup.api_auth.add_auth_headers(&mut headers);
        Some(ManagedImageStore {
            client,
            url,
            headers,
            model: model.to_owned(),
            fallback,
        })
    }

    pub(crate) async fn read_managed_image(
        &self,
        id: &str,
    ) -> Result<String, AttachmentStoreError> {
        let store = self
            .managed_image_backend("", Arc::new(InlineAttachmentStore))
            .await
            .ok_or_else(|| backend_error("the owning image gateway is unavailable"))?;
        let response = store
            .request(&serde_json::json!({"operation":"read", "image_id":id}))
            .await?
            .filter(|response| response.found && response.image_id.as_deref() == Some(id))
            .ok_or_else(|| backend_error("managed image resource was not found"))?;
        response.into_data_url()
    }
}

impl ImageResponse {
    fn into_data_url(self) -> Result<String, AttachmentStoreError> {
        let data = STANDARD
            .decode(self.data.as_deref().unwrap_or_default())
            .map_err(backend_error)?;
        let mime =
            image_mime(&data).ok_or_else(|| backend_error("stored image bytes are invalid"))?;
        let digest = format!("{:x}", Sha256::digest(&data));
        if self.size_bytes != Some(data.len() as u64)
            || self.sha256.as_deref() != Some(&digest)
            || self.mime_type.as_deref() != Some(mime)
        {
            return Err(backend_error(
                "stored image content did not match its metadata",
            ));
        }
        Ok(codex_utils_image::data_url_from_bytes(mime, &data))
    }
}

fn managed_http_client(
    factory: &HttpClientFactory,
    url: &str,
) -> Result<HttpClient, std::io::Error> {
    create_client_for_route(
        factory,
        url,
        ClientRouteClass::Api,
        ClientRedirectPolicy::Reject,
    )
    .map_err(std::io::Error::from)
}

impl ManagedImageStore {
    async fn request(
        &self,
        payload: &serde_json::Value,
    ) -> Result<Option<ImageResponse>, AttachmentStoreError> {
        let response = self
            .client
            .post(&self.url)
            .headers(self.headers.clone())
            .timeout(IMAGE_REQUEST_TIMEOUT)
            .json(payload)
            .send()
            .await
            .map_err(backend_error)?;
        if matches!(response.status().as_u16(), 404 | 405 | 501 | 503) {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(backend_error(format!(
                "image preparation returned HTTP {}",
                response.status()
            )));
        }
        let response: ImageResponse = response.json().await.map_err(backend_error)?;
        Ok(response.supported.then_some(response))
    }

    async fn upload_image(
        &self,
        request: UploadRequest,
    ) -> Result<UploadResult, AttachmentStoreError> {
        let Some(mime_type) = image_mime(&request.data) else {
            return self.fallback.upload(request).await;
        };
        let digest = format!("{:x}", Sha256::digest(&request.data));
        let mut payload = serde_json::json!({"operation":"lookup", "model":self.model,
            "sha256":digest, "mime_type":mime_type});
        let Some(mut response) = self.request(&payload).await? else {
            return self.fallback.upload(request).await;
        };
        if !response.found {
            payload["operation"] = "upload".into();
            payload["data"] = STANDARD.encode(&request.data).into();
            let Some(uploaded) = self.request(&payload).await? else {
                return self.fallback.upload(request).await;
            };
            response = uploaded;
        }
        let id = response
            .image_id
            .filter(|id| id.starts_with(IMAGE_ID_PREFIX));
        if !response.found
            || response.sha256.as_deref() != Some(&digest)
            || response.mime_type.as_deref() != Some(mime_type)
            || response.size_bytes != Some(request.data.len() as u64)
            || id.is_none()
        {
            return Err(backend_error(
                "image resource identity did not match uploaded content",
            ));
        }
        tracing::info!(thread_id = %request.thread_id, bytes = request.data.len(),
            "managed image resource prepared for durable history");
        Ok(UploadResult::File {
            file_id: id.unwrap_or_default(),
        })
    }
}

impl AttachmentStore for ManagedImageStore {
    fn upload(&self, request: UploadRequest) -> UploadFuture<'_> {
        Box::pin(self.upload_image(request))
    }

    fn resolve<'a>(&'a self, request: ResolveRequest<'a>) -> ResolveFuture<'a> {
        Box::pin(async move {
            if !request.file_id.starts_with(IMAGE_ID_PREFIX) {
                return self.fallback.resolve(request).await;
            }
            if request
                .download_url_ttl
                .is_some_and(|ttl| ttl > IMAGE_URL_LIFETIME)
            {
                return Err(backend_error(
                    "requested image URL lifetime exceeds one hour",
                ));
            }
            let response = self
                .request(&serde_json::json!({"operation":"resolve", "image_id":request.file_id}))
                .await?
                .filter(|response| {
                    response.found && response.image_id.as_deref() == Some(request.file_id)
                })
                .ok_or_else(|| backend_error("managed image resource could not be resolved"))?;
            Ok(AttachmentMetadata {
                mime_type: response.mime_type,
                size_bytes: response.size_bytes,
                file_url: request.download_url_ttl.and(response.image_url),
                ..Default::default()
            })
        })
    }
}

fn backend_error(error: impl std::fmt::Display) -> AttachmentStoreError {
    AttachmentStoreError::new(
        AttachmentStoreErrorKind::Backend,
        format!("managed image storage: {error}"),
    )
}

fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Png => Some("image/png"),
        image::ImageFormat::Jpeg => Some("image/jpeg"),
        image::ImageFormat::WebP => Some("image/webp"),
        image::ImageFormat::Gif => Some("image/gif"),
        _ => None,
    }
}
