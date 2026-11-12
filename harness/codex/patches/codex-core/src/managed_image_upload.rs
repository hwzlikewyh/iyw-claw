//! 进程网关已在upload内执行init查重，避免在前面重复访问Fusion。
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use codex_attachment_store::{AttachmentStoreError, UploadRequest};
use serde_json::Value;
use url::Url;

use super::{ImageResponse, ManagedImageStore};

const LOCAL_GATEWAY_PREFIX_BYTES: usize = 64;

pub(super) fn is_local_image_gateway(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    let Some(path) = url.path().strip_prefix('/') else {
        return false;
    };
    let Some((prefix, suffix)) = path.split_once('/') else {
        return false;
    };
    url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port().is_some_and(|port| port != 0)
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && prefix.len() == LOCAL_GATEWAY_PREFIX_BYTES
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && suffix == "v1/images/input/prepare"
}

impl ManagedImageStore {
    pub(super) async fn prepare_upload(
        &self,
        request: &UploadRequest,
        (digest, mime_type): (&str, &str),
    ) -> Result<Option<ImageResponse>, AttachmentStoreError> {
        let mut payload = serde_json::json!({"operation":"lookup", "model":self.model,
            "sha256":digest, "mime_type":mime_type});
        // 只在严格匹配的本地网关上发送字节；直连云端仍先查重以节省带宽。
        if is_local_image_gateway(&self.url) {
            set_upload(&mut payload, &request.data);
            return self.request(&payload).await;
        }
        let Some(response) = self.request(&payload).await? else {
            return Ok(None);
        };
        if response.found {
            return Ok(Some(response));
        }
        set_upload(&mut payload, &request.data);
        self.request(&payload).await
    }
}

fn set_upload(payload: &mut Value, data: &[u8]) {
    payload["operation"] = "upload".into();
    payload["data"] = STANDARD.encode(data).into();
}
