use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::DingtalkBackend;
use crate::chat_channel::attachments::{
    AttachmentSource, ChannelAttachment, IncomingAttachment, MIB,
};
use crate::chat_channel::error::ChatChannelError;
use crate::chat_channel::media_http::{self, failure, required, transport};
use crate::chat_channel::types::{ChannelMessageTarget, SentMessageId};

pub(super) struct Token {
    value: String,
    expires: Instant,
}

impl DingtalkBackend {
    async fn access_token(&self) -> Result<String, ChatChannelError> {
        let mut cache = self.media_token.lock().await;
        if let Some(token) = cache
            .as_ref()
            .filter(|token| token.expires > Instant::now())
        {
            return Ok(token.value.clone());
        }
        let response = self
            .client
            .post("https://api.dingtalk.com/v1.0/oauth2/accessToken")
            .json(&json!({ "appKey": self.config.client_id, "appSecret": self.client_secret }))
            .send()
            .await
            .map_err(transport)?;
        let value = media_http::json(response).await?;
        let token = required(&value, "accessToken")?.to_string();
        let ttl = value["expireIn"]
            .as_u64()
            .unwrap_or(0)
            .min(7200)
            .saturating_sub(60);
        *cache = Some(Token {
            value: token.clone(),
            expires: Instant::now() + Duration::from_secs(ttl),
        });
        Ok(token)
    }

    pub(super) async fn send_openapi_text(
        &self,
        text: &str,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        let (endpoint, mut body) = target_body(target)?;
        body["robotCode"] = json!(self.config.client_id);
        body["msgKey"] = json!("sampleMarkdown");
        body["msgParam"] = json!(json!({ "title": "iyw-claw", "text": text }).to_string());
        let token = self.access_token().await?;
        let response = self
            .client
            .post(format!("https://api.dingtalk.com/v1.0/robot/{endpoint}"))
            .header("x-acs-dingtalk-access-token", token)
            .json(&body)
            .send()
            .await
            .map_err(transport)?;
        receipt(&media_http::json(response).await?)
    }

    pub(super) async fn send_media(
        &self,
        file: &ChannelAttachment,
        target: &ChannelMessageTarget,
    ) -> Result<SentMessageId, ChatChannelError> {
        file.validate(20 * MIB)?;
        let (endpoint, mut body) = target_body(target)?;
        let image = crate::chat_channel::media_capabilities::native_image(
            "dingtalk",
            &file.mime_type,
            file.byte_len(),
        );
        let token = self.access_token().await?;
        let media_id = self.upload(file, &token, image).await?;
        let (key, parameter) = message_parameter(file, &media_id, image);
        body["robotCode"] = json!(self.config.client_id);
        body["msgKey"] = json!(key);
        body["msgParam"] = json!(parameter.to_string());
        let response = self
            .client
            .post(format!("https://api.dingtalk.com/v1.0/robot/{endpoint}"))
            .header("x-acs-dingtalk-access-token", &token)
            .json(&body)
            .send()
            .await
            .map_err(transport)?;
        let result = media_http::json(response).await?;
        receipt(&result)
    }

    async fn upload(
        &self,
        file: &ChannelAttachment,
        token: &str,
        image: bool,
    ) -> Result<String, ChatChannelError> {
        let part = reqwest::multipart::Part::bytes(file.bytes.to_vec())
            .file_name(file.name.clone())
            .mime_str(&file.mime_type)
            .map_err(|_| failure("Invalid media MIME type"))?;
        let response = self
            .client
            .post("https://oapi.dingtalk.com/media/upload")
            .query(&[
                ("access_token", token),
                ("type", if image { "image" } else { "file" }),
            ])
            .multipart(reqwest::multipart::Form::new().part("media", part))
            .send()
            .await
            .map_err(transport)?;
        let body = media_http::json(response).await?;
        Ok(required(&body, "media_id")?.to_string())
    }

    pub(super) async fn download_media(
        &self,
        source: &IncomingAttachment,
    ) -> Result<ChannelAttachment, ChatChannelError> {
        let AttachmentSource::Dingtalk { download_code } = &source.source else {
            return Err(failure("Invalid DingTalk media source"));
        };
        let token = self.access_token().await?;
        let response = self
            .client
            .post("https://api.dingtalk.com/v1.0/robot/messageFiles/download")
            .header("x-acs-dingtalk-access-token", token)
            .json(&json!({ "robotCode": self.config.client_id, "downloadCode": download_code }))
            .send()
            .await
            .map_err(transport)?;
        let result = media_http::json(response).await?;
        let bytes = media_http::download(required(&result, "downloadUrl")?).await?;
        Ok(media_http::attachment(source, bytes))
    }
}

fn receipt(result: &Value) -> Result<SentMessageId, ChatChannelError> {
    for field in [
        "invalidStaffIdList",
        "flowControlledStaffIdList",
        "filteredStaffIdList",
    ] {
        if result[field]
            .as_array()
            .is_some_and(|list| !list.is_empty())
        {
            return Err(failure(format!("DingTalk media target rejected: {field}")));
        }
    }
    Ok(SentMessageId(
        required(result, "processQueryKey")?.to_string(),
    ))
}

fn target_body(target: &ChannelMessageTarget) -> Result<(&'static str, Value), ChatChannelError> {
    let payload = target
        .provider_payload
        .as_ref()
        .ok_or_else(|| failure("DingTalk media target is missing"))?;
    if payload["chat_type"] == "2" {
        let chat_id = target
            .chat_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| failure("DingTalk group target is missing"))?;
        return Ok((
            "groupMessages/send",
            json!({ "openConversationId": chat_id }),
        ));
    }
    let user_id = payload["sender_staff_id"].as_str().filter(|id| !id.is_empty())
        .ok_or_else(|| failure("TARGET_NOT_SENDABLE: DingTalk target has no staff ID; refresh it with a new inbound message"))?;
    Ok(("oToMessages/batchSend", json!({ "userIds": [user_id] })))
}

fn message_parameter(
    file: &ChannelAttachment,
    media_id: &str,
    image: bool,
) -> (&'static str, Value) {
    if image {
        let clean_id = media_id.trim_start_matches('@');
        return (
            "sampleImageMsg",
            json!({ "photoURL": format!("https://down.dingtalk.com/media/{clean_id}") }),
        );
    }
    let kind = Path::new(&file.name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("file");
    (
        "sampleFile",
        json!({ "mediaId": media_id, "fileName": file.name, "fileType": kind }),
    )
}
