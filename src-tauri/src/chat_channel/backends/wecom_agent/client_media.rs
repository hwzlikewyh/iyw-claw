use serde::Deserialize;

use super::client::{provider_result, send_failed, transport_error, SendError, SendReceipt};
use super::WecomAgentClient;
use crate::chat_channel::attachments::ChannelAttachment;

const UPLOAD_MEDIA_URL: &str = "https://qyapi.weixin.qq.com/cgi-bin/media/upload";

#[derive(Debug, Clone, Copy)]
pub(super) enum MediaKind {
    Image,
    File,
}

impl MediaKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::File => "file",
        }
    }
}

impl WecomAgentClient {
    pub(super) async fn upload_media(
        &self,
        access_token: &str,
        kind: MediaKind,
        attachment: &ChannelAttachment,
    ) -> Result<String, SendError> {
        let part = reqwest::multipart::Part::bytes(attachment.bytes.to_vec())
            .file_name(attachment.name.clone())
            .mime_str(&attachment.mime_type)
            .map_err(|_| send_failed("invalid attachment MIME type"))?;
        let response = self
            .client
            .post(UPLOAD_MEDIA_URL)
            .query(&[("access_token", access_token), ("type", kind.as_str())])
            .multipart(reqwest::multipart::Form::new().part("media", part))
            .send()
            .await
            .map_err(|error| SendError::Failed(transport_error("media upload failed", error)))?;
        if !response.status().is_success() {
            return Err(send_failed(&format!(
                "WeCom media upload failed (HTTP {})",
                response.status()
            )));
        }
        let body: UploadResponse = response.json().await.map_err(|error| {
            SendError::Failed(transport_error("media upload response was invalid", error))
        })?;
        provider_result(body.errcode, &body.errmsg, "media upload")?;
        body.media_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| send_failed("WeCom media upload response omitted media_id"))
    }

    pub(super) async fn send_media(
        &self,
        access_token: &str,
        user_id: &str,
        agent_id: i64,
        kind: MediaKind,
        media_id: &str,
    ) -> Result<SendReceipt, SendError> {
        let mut payload = serde_json::json!({
            "touser": user_id,
            "msgtype": kind.as_str(),
            "agentid": agent_id,
            "enable_duplicate_check": 1,
            "duplicate_check_interval": 1800,
        });
        payload[kind.as_str()] = serde_json::json!({ "media_id": media_id });
        self.send_payload(access_token, payload).await
    }
}

#[derive(Debug, Deserialize)]
struct UploadResponse {
    #[serde(default)]
    errcode: i64,
    #[serde(default)]
    errmsg: String,
    media_id: Option<String>,
}
