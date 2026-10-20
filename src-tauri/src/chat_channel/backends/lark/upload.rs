use serde_json::Value;

use super::{sending, ChannelAttachment, ChatChannelError, LarkBackend};

impl LarkBackend {
    pub(super) async fn upload_attachment(
        &self,
        file: &ChannelAttachment,
        image: bool,
    ) -> Result<String, ChatChannelError> {
        for attempt in 0..2 {
            let token = self.get_tenant_access_token().await?;
            let response = self.post_upload((file, image), &token).await?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.invalidate_token(&token).await;
                continue;
            }
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(ChatChannelError::DeliveryDeferred(
                    "PROVIDER_RATE_LIMIT".into(),
                ));
            }
            let body: Value = response.json().await.map_err(|_| {
                ChatChannelError::DeliveryDeferred("MEDIA_UPLOAD_RESPONSE_RETRY".into())
            })?;
            let code = body["code"].as_i64().ok_or_else(|| {
                ChatChannelError::SendFailed("PROVIDER_REJECTED:LARK_UPLOAD_RESPONSE".into())
            })? as i32;
            if sending::token_error(code) && attempt == 0 {
                self.invalidate_token(&token).await;
                continue;
            }
            sending::check_code(code)?;
            let key = if image { "image_key" } else { "file_key" };
            return body["data"][key]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    ChatChannelError::DeliveryDeferred("MEDIA_UPLOAD_KEY_UNAVAILABLE".into())
                });
        }
        Err(ChatChannelError::AuthenticationFailed(
            "LARK_UPLOAD_TOKEN_REJECTED".into(),
        ))
    }

    async fn post_upload(
        &self,
        file: (&ChannelAttachment, bool),
        token: &str,
    ) -> Result<reqwest::Response, ChatChannelError> {
        self.client
            .post(format!(
                "{}/open-apis/im/v1/{}",
                self.api_base_url,
                if file.1 { "images" } else { "files" }
            ))
            .bearer_auth(token)
            .multipart(form(file.0, file.1)?)
            .send()
            .await
            .map_err(|_| ChatChannelError::DeliveryDeferred("MEDIA_UPLOAD_RETRY".into()))
    }
}

fn form(
    file: &ChannelAttachment,
    image: bool,
) -> Result<reqwest::multipart::Form, ChatChannelError> {
    let part = reqwest::multipart::Part::bytes(file.bytes.to_vec())
        .file_name(file.name.clone())
        .mime_str(&file.mime_type)
        .map_err(|_| {
            ChatChannelError::ConfigurationInvalid("Invalid attachment MIME type".into())
        })?;
    let form = reqwest::multipart::Form::new();
    Ok(if image {
        form.text("image_type", "message").part("image", part)
    } else {
        form.text("file_type", "stream")
            .text("file_name", file.name.clone())
            .part("file", part)
    })
}
