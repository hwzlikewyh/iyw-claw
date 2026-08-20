use serde_json::{json, Value};

use super::authorization::AuthorizationEntry;
use super::service::{ChannelToolService, MutationStart};
use super::types::{ChannelCaller, CredentialInput};
use crate::commands::chat_channel;

impl ChannelToolService {
    pub(super) async fn start_authorization(
        &self,
        caller: &ChannelCaller,
        input: CredentialInput,
    ) -> Result<Value, String> {
        let request_id = input.request_id.as_deref().ok_or("INVALID_REQUEST_ID")?;
        let channel = self.channel(input.channel_id).await?;
        let digest = json!({ "channel_id": input.channel_id, "operation": "start_authorization" });
        let start = self
            .begin_mutation(caller, "manage_channel_credential", request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return self.authorization_mutation_return(start).await;
        };
        let authorization = request_authorization(self, &channel.channel_type).await;
        let (provider_ref, qr_content) = match authorization {
            Ok(value) => value,
            Err(code) => {
                return self
                    .finish_mutation(
                        caller,
                        "manage_channel_credential",
                        request_id,
                        model,
                        super::service::error_value(code),
                        Some(input.channel_id),
                    )
                    .await;
            }
        };
        let (authorization_id, expires_at) = self
            .authorizations
            .insert(
                input.channel_id,
                &channel.channel_type,
                provider_ref,
                qr_content.clone(),
            )
            .await;
        let cached = authorization_result(&authorization_id, input.channel_id, expires_at);
        self.finish_mutation(
            caller,
            "manage_channel_credential",
            request_id,
            model,
            cached,
            Some(input.channel_id),
        )
        .await?;
        Ok(authorization_response(
            authorization_id,
            input.channel_id,
            expires_at,
            qr_content,
        ))
    }

    pub(super) async fn check_authorization(
        &self,
        caller: &ChannelCaller,
        input: CredentialInput,
    ) -> Result<Value, String> {
        let request_id = input.request_id.as_deref().ok_or("INVALID_REQUEST_ID")?;
        let authorization_id = input
            .authorization_id
            .as_deref()
            .ok_or("AUTHORIZATION_REQUIRED")?;
        let entry = self
            .authorizations
            .get(authorization_id)
            .await
            .ok_or_else(|| "AUTHORIZATION_EXPIRED".to_string())?;
        if entry.channel_id != input.channel_id {
            return Err("AUTHORIZATION_CHANNEL_MISMATCH".to_string());
        }
        let digest = json!({
            "channel_id": input.channel_id,
            "operation": "check_authorization",
            "authorization_id": authorization_id,
        });
        let start = self
            .begin_mutation(caller, "manage_channel_credential", request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return mutation_return(start);
        };
        let result = self
            .check_authorization_inner(authorization_id, &entry)
            .await
            .map(|status| json!({ "authorization_id": authorization_id, "status": status }))
            .unwrap_or_else(super::service::error_value);
        self.finish_mutation(
            caller,
            "manage_channel_credential",
            request_id,
            model,
            result,
            Some(input.channel_id),
        )
        .await
    }

    async fn check_authorization_inner(
        &self,
        authorization_id: &str,
        entry: &AuthorizationEntry,
    ) -> Result<String, String> {
        let status = authorization_status(self, entry).await?;
        if status == "confirmed" {
            self.authorizations.remove(authorization_id).await;
        }
        Ok(status)
    }

    async fn authorization_mutation_return(&self, start: MutationStart) -> Result<Value, String> {
        let MutationStart::Return(mut value) = start else {
            unreachable!()
        };
        if value.get("error").is_some()
            || value.get("status").and_then(Value::as_str) == Some("processing")
        {
            return Ok(value);
        }
        let authorization_id = value
            .get("authorization_id")
            .and_then(Value::as_str)
            .ok_or("AUTHORIZATION_EXPIRED")?;
        let entry = self
            .authorizations
            .get(authorization_id)
            .await
            .ok_or("AUTHORIZATION_EXPIRED")?;
        if let Some(object) = value.as_object_mut() {
            object.insert("qr_content".to_string(), json!(entry.qr_content));
        }
        Ok(value)
    }
}

async fn request_authorization(
    service: &ChannelToolService,
    channel_type: &str,
) -> Result<(String, String), String> {
    match channel_type {
        "weixin" => chat_channel::weixin_get_qrcode_core(&service.db)
            .await
            .map(|value| (value.qrcode_id, value.qrcode_img_content))
            .map_err(|_| "AUTHORIZATION_START_FAILED".to_string()),
        "wecom" => chat_channel::wecom_start_auth_core(&service.db, &service.manager)
            .await
            .map(|value| (value.auth_url.clone(), value.auth_url))
            .map_err(|_| "AUTHORIZATION_START_FAILED".to_string()),
        _ => Err("AUTHORIZATION_UNSUPPORTED".to_string()),
    }
}

async fn authorization_status(
    service: &ChannelToolService,
    entry: &AuthorizationEntry,
) -> Result<String, String> {
    if entry.channel_type == "weixin" {
        return chat_channel::weixin_check_qrcode_core(
            &service.db,
            &service.manager,
            entry.channel_id,
            &entry.provider_ref,
            None,
        )
        .await
        .map(|value| value.status)
        .map_err(|_| "AUTHORIZATION_CHECK_FAILED".to_string());
    }
    if entry.channel_type == "wecom" {
        return chat_channel::wecom_get_auth_status_core(&service.db, &service.manager)
            .await
            .map(|value| {
                if value.authorized {
                    "confirmed"
                } else {
                    "waiting"
                }
                .to_string()
            })
            .map_err(|_| "AUTHORIZATION_CHECK_FAILED".to_string());
    }
    Err("AUTHORIZATION_UNSUPPORTED".to_string())
}

fn authorization_result(
    authorization_id: &str,
    channel_id: i32,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Value {
    json!({
        "status": "authorization_started",
        "authorization_id": authorization_id,
        "channel_id": channel_id,
        "expires_at": expires_at.to_rfc3339(),
        "qr_available": true,
    })
}

fn authorization_response(
    authorization_id: String,
    channel_id: i32,
    expires_at: chrono::DateTime<chrono::Utc>,
    qr_content: String,
) -> Value {
    json!({
        "status": "authorization_started",
        "authorization_id": authorization_id,
        "channel_id": channel_id,
        "expires_at": expires_at.to_rfc3339(),
        "qr_content": qr_content,
    })
}

fn mutation_return(start: MutationStart) -> Result<Value, String> {
    match start {
        MutationStart::Return(value) => Ok(value),
        MutationStart::Started(_) => unreachable!(),
    }
}
