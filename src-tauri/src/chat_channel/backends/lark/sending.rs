use super::{
    ChatChannelError, LarkBackend, SendMessageRequest, SendMessageResponse, SentMessageId,
};

pub(super) fn token_error(code: i32) -> bool {
    matches!(code, 99991663 | 99991665)
}

impl LarkBackend {
    pub(super) async fn send_message_request(
        &self,
        request: SendMessageRequest,
    ) -> Result<SentMessageId, ChatChannelError> {
        for attempt in 0..2 {
            let token = self.get_tenant_access_token().await?;
            let response = self.post_message(&request, &token).await?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.invalidate_token(&token).await;
                continue;
            }
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(ChatChannelError::DeliveryDeferred(
                    "PROVIDER_RATE_LIMIT".into(),
                ));
            }
            let result: SendMessageResponse = response.json().await.map_err(|_| {
                ChatChannelError::SendFailed("DELIVERY_UNKNOWN:LARK_RESPONSE".into())
            })?;
            if token_error(result.code) && attempt == 0 {
                self.invalidate_token(&token).await;
                continue;
            }
            check_code(result.code)?;
            return result
                .data
                .and_then(|data| data.message_id)
                .filter(|id| !id.is_empty())
                .map(SentMessageId)
                .ok_or_else(|| {
                    ChatChannelError::SendFailed("DELIVERY_UNKNOWN:LARK_RECEIPT".into())
                });
        }
        Err(ChatChannelError::AuthenticationFailed(
            "LARK_TOKEN_REJECTED".into(),
        ))
    }

    async fn post_message(
        &self,
        request: &SendMessageRequest,
        token: &str,
    ) -> Result<reqwest::Response, ChatChannelError> {
        self.client
            .post(format!(
                "{}/open-apis/im/v1/messages?receive_id_type=chat_id",
                self.api_base_url
            ))
            .bearer_auth(token)
            .json(request)
            .send()
            .await
            .map_err(|error| {
                if error.is_connect() {
                    ChatChannelError::NotConnected
                } else {
                    ChatChannelError::SendFailed("DELIVERY_UNKNOWN:LARK_TRANSPORT".into())
                }
            })
    }

    pub(super) async fn invalidate_token(&self, rejected: &str) {
        let mut cache = self.token_cache.write().await;
        if cache.as_ref().is_some_and(|entry| entry.token == rejected) {
            *cache = None;
        }
    }
}

pub(super) fn check_code(code: i32) -> Result<(), ChatChannelError> {
    if token_error(code) {
        return Err(ChatChannelError::AuthenticationFailed(
            "LARK_TOKEN_REJECTED".into(),
        ));
    }
    if code == 99991400 {
        return Err(ChatChannelError::DeliveryDeferred(
            "PROVIDER_RATE_LIMIT".into(),
        ));
    }
    if code != 0 {
        return Err(ChatChannelError::SendFailed(format!(
            "PROVIDER_REJECTED:LARK:{code}"
        )));
    }
    Ok(())
}
