use serde_json::{json, Value};

use super::service::{ChannelToolService, MutationStart};
use super::types::{ChannelCaller, CredentialInput, CredentialOperation};
use crate::commands::chat_channel;
use crate::db::entities::chat_channel as chat_channel_entity;
use crate::db::service::chat_channel_service;

impl ChannelToolService {
    pub(super) async fn manage_credential(
        &self,
        caller: &ChannelCaller,
        input: Result<CredentialInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        match input.operation {
            CredentialOperation::Status => self.credential_status(input.channel_id).await,
            CredentialOperation::Set | CredentialOperation::Replace => {
                self.write_credential(caller, input).await
            }
            CredentialOperation::StartAuthorization => {
                self.start_authorization(caller, input).await
            }
            CredentialOperation::CheckAuthorization => {
                self.check_authorization(caller, input).await
            }
            CredentialOperation::Delete => Err("CONFIRMATION_REQUIRED".to_string()),
        }
    }

    pub(super) async fn manage_credential_confirmed(
        &self,
        caller: &ChannelCaller,
        input: Result<CredentialInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        if !matches!(input.operation, CredentialOperation::Delete) {
            return Err("CONFIRMATION_NOT_APPLICABLE".to_string());
        }
        self.delete_credential(caller, input).await
    }

    async fn credential_status(&self, channel_id: i32) -> Result<Value, String> {
        let channel = self.channel(channel_id).await?;
        let configured = match channel.channel_type.as_str() {
            "wecom" => chat_channel::wecom_get_auth_status_core(&self.db, &self.manager)
                .await
                .map(|status| status.authorized)
                .unwrap_or(false),
            _ => crate::keyring_store::get_channel_token(channel_id).is_some(),
        };
        Ok(json!({
            "channel_id": channel_id,
            "configured": configured,
            "authorization_type": authorization_type(&channel.channel_type),
        }))
    }

    async fn write_credential(
        &self,
        caller: &ChannelCaller,
        input: CredentialInput,
    ) -> Result<Value, String> {
        let request_id = input.request_id.as_deref().ok_or("INVALID_REQUEST_ID")?;
        let channel = self.channel(input.channel_id).await?;
        if channel.channel_type == "wecom" {
            return Err("AUTHORIZATION_REQUIRED".to_string());
        }
        let credential = input
            .credential
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "CREDENTIAL_REQUIRED".to_string())?;
        if matches!(input.operation, CredentialOperation::Set)
            && crate::keyring_store::get_channel_token(input.channel_id).is_some()
        {
            return Err("CREDENTIAL_ALREADY_CONFIGURED".to_string());
        }
        let digest = json!({
            "channel_id": input.channel_id,
            "operation": input.operation,
            "credential": credential,
        });
        let start = self
            .begin_mutation(caller, "manage_channel_credential", request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return match start {
                MutationStart::Return(value) => Ok(value),
                _ => unreachable!(),
            };
        };
        let result = chat_channel::save_chat_channel_token_core(
            &self.db,
            &self.manager,
            input.channel_id,
            credential,
        )
        .await
        .map(|_| json!({ "status": "configured", "channel_id": input.channel_id }))
        .unwrap_or_else(|_| super::service::error_value("CREDENTIAL_SAVE_FAILED"));
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

    async fn delete_credential(
        &self,
        caller: &ChannelCaller,
        input: CredentialInput,
    ) -> Result<Value, String> {
        let request_id = input.request_id.as_deref().ok_or("INVALID_REQUEST_ID")?;
        let channel = self.channel(input.channel_id).await?;
        if channel.channel_type == "wecom" {
            return Err("CREDENTIAL_DELETE_UNSUPPORTED".to_string());
        }
        let digest = json!({ "channel_id": input.channel_id, "operation": "delete" });
        let start = self
            .begin_mutation(caller, "manage_channel_credential", request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return match start {
                MutationStart::Return(value) => Ok(value),
                _ => unreachable!(),
            };
        };
        let result =
            chat_channel::delete_chat_channel_token_core(&self.db, &self.manager, input.channel_id)
                .await
                .map(|_| json!({ "status": "deleted", "channel_id": input.channel_id }))
                .unwrap_or_else(|_| super::service::error_value("CREDENTIAL_DELETE_FAILED"));
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

    pub(super) async fn channel(
        &self,
        channel_id: i32,
    ) -> Result<chat_channel_entity::Model, String> {
        chat_channel_service::get_by_id(&self.db.conn, channel_id)
            .await
            .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
            .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())
    }
}

fn authorization_type(channel_type: &str) -> &'static str {
    match channel_type {
        "weixin" | "wecom" => "qr_code",
        _ => "token",
    }
}
