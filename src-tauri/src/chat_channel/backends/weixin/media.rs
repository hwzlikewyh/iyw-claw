use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use md5::{Digest, Md5};
use rand::RngCore;
use serde_json::{json, Value};

use super::{inbound::CDN_BASE, media_crypto, WeixinBackend, ILINK_CHANNEL_VERSION};
use crate::chat_channel::attachments::{ChannelAttachment, MIB};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http::{self, failure, transport};
use crate::chat_channel::types::{ChannelMessageTarget, SentMessageId};

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(90);

struct Upload<'a> {
    to: &'a str,
    file: &'a ChannelAttachment,
    file_key: String,
    raw_file_md5: String,
    key: [u8; 16],
    ciphertext: Vec<u8>,
    image: bool,
}

impl<'a> Upload<'a> {
    fn new(to: &'a str, file: &'a ChannelAttachment) -> Result<Self, ChatChannelError> {
        let mut key = [0_u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut key);
        Ok(Self {
            to,
            file,
            key,
            file_key: uuid::Uuid::new_v4().simple().to_string(),
            raw_file_md5: format!("{:x}", Md5::digest(&file.bytes)),
            ciphertext: media_crypto::encrypt(&file.bytes, &key)?,
            image: crate::chat_channel::media_capabilities::native_image(
                "weixin",
                &file.mime_type,
                file.byte_len(),
            ),
        })
    }

    fn message(&self, download_parameter: String) -> Value {
        let media = json!({ "encrypt_query_param": download_parameter,
            "aes_key": STANDARD.encode(media_crypto::hex(&self.key)), "encrypt_type": 1 });
        if self.image {
            json!({ "type": 2, "image_item": { "media": media, "mid_size": self.ciphertext.len() } })
        } else {
            json!({ "type": 4, "file_item": { "media": media, "file_name": self.file.name, "len": self.file.byte_len().to_string() } })
        }
    }
}

impl WeixinBackend {
    pub(super) async fn send_media(
        &self,
        file: &ChannelAttachment,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        file.validate(100 * MIB)?;
        let to = target
            .chat_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| failure("Weixin media target is missing"))?;
        let context = self
            .context_token_for(target, to)
            .await
            .ok_or_else(|| failure("TARGET_CONTEXT_EXPIRED: Weixin target has no reply context"))?;
        let upload = Upload::new(to, file)?;
        let url = self.upload_url(&upload).await?;
        let parameter = upload_ciphertext(&url, &upload.ciphertext).await?;
        self.send_media_item(to, &context, upload.message(parameter))
            .await
    }

    async fn upload_url(&self, upload: &Upload<'_>) -> Result<reqwest::Url, ChatChannelError> {
        let body = json!({
            "filekey": upload.file_key, "media_type": if upload.image { 1 } else { 3 },
            "to_user_id": upload.to, "rawsize": upload.file.byte_len(),
            "rawfilemd5": upload.raw_file_md5,
            "filesize": upload.ciphertext.len(), "no_need_thumb": true,
            "aeskey": media_crypto::hex(&upload.key),
            "base_info": { "channel_version": ILINK_CHANNEL_VERSION },
        });
        let response = self
            .client
            .post(format!("{}/ilink/bot/getuploadurl", self.base_url))
            .headers(Self::build_headers(&self.bot_token, &self.wechat_uin))
            .json(&body)
            .send()
            .await
            .map_err(transport)?;
        let result = media_http::json(response)
            .await
            .map_err(|error| failure(format!("Weixin getuploadurl failed: {error}")))?;
        let source = result["upload_full_url"]
            .as_str()
            .filter(|url| !url.is_empty())
            .map(str::to_string)
            .or_else(|| {
                result["upload_param"]
                    .as_str()
                    .filter(|p| !p.is_empty())
                    .map(|parameter| {
                        format!(
                            "{CDN_BASE}/upload?encrypted_query_param={}&filekey={}",
                            urlencoding::encode(parameter),
                            upload.file_key
                        )
                    })
            })
            .ok_or_else(|| failure("Weixin media upload URL missing"))?;
        let url =
            reqwest::Url::parse(&source).map_err(|_| failure("Invalid Weixin media upload URL"))?;
        if url.scheme() != "https" {
            return Err(failure("Weixin media upload requires HTTPS"));
        }
        Ok(url)
    }

    async fn send_media_item(
        &self,
        to: &str,
        context: &str,
        item: Value,
    ) -> Result<SentMessageId, ChatChannelError> {
        let body = json!({ "msg": { "from_user_id": "", "to_user_id": to,
            "client_id": format!("iyw-claw-{}", uuid::Uuid::new_v4()),
            "message_type": 2, "message_state": 2, "context_token": context, "item_list": [item] },
            "base_info": { "channel_version": ILINK_CHANNEL_VERSION } });
        let response = self
            .client
            .post(format!("{}/ilink/bot/sendmessage", self.base_url))
            .headers(Self::build_headers(&self.bot_token, &self.wechat_uin))
            .json(&body)
            .send()
            .await
            .map_err(transport)?;
        let bytes = media_http::read_limited(response, 1024 * 1024).await?;
        let result: Value = serde_json::from_slice(&bytes)
            .map_err(|_| failure("Weixin media reply is not JSON"))?;
        for field in ["ret", "errcode"] {
            let code = result[field].as_i64().unwrap_or(0);
            if matches!(code, -2 | -14) {
                let _ = crate::db::service::sender_context_service::clear_weixin_context_token_if_matches(
                    &self.database, self.channel_id, to, context).await;
                return Err(failure("TARGET_CONTEXT_EXPIRED"));
            }
            if code != 0 {
                return Err(failure(format!(
                    "Weixin media send rejected: {field}={code}"
                )));
            }
        }
        Ok(SentMessageId(String::new()))
    }
}

async fn upload_ciphertext(
    url: &reqwest::Url,
    ciphertext: &[u8],
) -> Result<String, ChatChannelError> {
    let client = crate::remote_image::network::validated_client(url)
        .await
        .map_err(|error| failure(format!("Weixin upload destination rejected: {error}")))?;
    let response = client
        .post(url.clone())
        .header("Content-Type", "application/octet-stream")
        .timeout(UPLOAD_TIMEOUT)
        .body(ciphertext.to_vec())
        .send()
        .await
        .map_err(transport)?;
    if !response.status().is_success() {
        return Err(failure(format!(
            "Weixin CDN upload HTTP {}",
            response.status()
        )));
    }
    response
        .headers()
        .get("x-encrypted-param")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| failure("Weixin CDN response omitted encrypted parameter"))
}
