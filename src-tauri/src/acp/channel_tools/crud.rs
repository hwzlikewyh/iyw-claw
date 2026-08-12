use serde_json::{json, Value};

use super::channel_config::{create_config, update_config};
use super::service::{ChannelToolService, MutationStart};
use super::types::{ChannelCaller, DeleteChannelInput, ListChannelsInput, SaveChannelInput};
use super::views::ChannelView;
use crate::commands::chat_channel;
use crate::db::service::chat_channel_service;

impl ChannelToolService {
    pub(super) async fn delete_channel(
        &self,
        caller: &ChannelCaller,
        input: Result<DeleteChannelInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        let digest =
            json!({ "channel_id": input.channel_id, "reason_present": input.reason.is_some() });
        let start = self
            .begin_mutation(caller, "delete_message_channel", &input.request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return match start {
                MutationStart::Return(value) => Ok(value),
                _ => unreachable!(),
            };
        };
        let result = match ensure_channel(&self.db, input.channel_id).await {
            Ok(()) => {
                chat_channel::delete_chat_channel_core(&self.db, &self.manager, input.channel_id)
                    .await
                    .map(|_| json!({ "status": "deleted", "channel_id": input.channel_id }))
                    .unwrap_or_else(|_| super::service::error_value("CHANNEL_DELETE_FAILED"))
            }
            Err(code) => super::service::error_value(code),
        };
        self.finish_mutation(
            caller,
            "delete_message_channel",
            &input.request_id,
            model,
            result,
            Some(input.channel_id),
        )
        .await
    }

    pub(super) async fn list_channels(
        &self,
        input: Result<ListChannelsInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        let rows = chat_channel_service::list_all(&self.db.conn)
            .await
            .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?;
        let wecom_authorized = self.wecom_authorized(&rows).await;
        let filtered = rows
            .into_iter()
            .filter(|row| input.channel_id.is_none_or(|id| row.id == id))
            .filter(|row| {
                input
                    .channel_type
                    .as_deref()
                    .is_none_or(|v| row.channel_type == v)
            })
            .filter(|row| input.enabled.is_none_or(|v| row.enabled == v))
            .filter(|row| {
                input
                    .runtime_status
                    .as_deref()
                    .is_none_or(|v| row.runtime_status == v)
            });
        let mut channels = Vec::new();
        for row in filtered {
            channels.push(ChannelView::from_model(&self.db.conn, row, wecom_authorized).await?);
        }
        Ok(json!({ "channels": channels }))
    }

    pub(super) async fn save_channel(
        &self,
        caller: &ChannelCaller,
        input: Result<SaveChannelInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        let digest = safe_save_digest(&input);
        let start = self
            .begin_mutation(caller, "save_message_channel", &input.request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return match start {
                MutationStart::Return(value) => Ok(value),
                _ => unreachable!(),
            };
        };
        let result = self.save_channel_inner(&input).await;
        let result = result.unwrap_or_else(super::service::error_value);
        self.finish_mutation(
            caller,
            "save_message_channel",
            &input.request_id,
            model,
            result,
            input.channel_id,
        )
        .await
    }

    async fn save_channel_inner(&self, input: &SaveChannelInput) -> Result<Value, String> {
        let info = if let Some(channel_id) = input.channel_id {
            self.update_channel(channel_id, input).await?
        } else {
            self.create_channel(input).await?
        };
        let target_error = info.target_registration_error.clone();
        if let Some(credential) = input.credential.as_deref() {
            require_nonempty(credential, "CREDENTIAL_REQUIRED")?;
            chat_channel::save_chat_channel_token_core(
                &self.db,
                &self.manager,
                info.id,
                credential,
            )
            .await
            .map_err(|_| "CREDENTIAL_SAVE_FAILED".to_string())?;
        }
        let row = chat_channel_service::get_by_id(&self.db.conn, info.id)
            .await
            .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
            .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())?;
        Ok(json!({
            "status": save_status(&row.runtime_status, target_error.is_some()),
            "channel": ChannelView::from_model(
                &self.db.conn,
                row.clone(),
                self.wecom_authorized(std::slice::from_ref(&row)).await,
            ).await?,
            "target_error": target_error,
        }))
    }

    async fn wecom_authorized(
        &self,
        channels: &[crate::db::entities::chat_channel::Model],
    ) -> Option<bool> {
        if !channels
            .iter()
            .any(|channel| channel.channel_type == "wecom")
        {
            return None;
        }
        chat_channel::wecom_get_auth_status_core(&self.db, &self.manager)
            .await
            .ok()
            .map(|status| status.authorized)
    }

    async fn create_channel(
        &self,
        input: &SaveChannelInput,
    ) -> Result<crate::models::chat_channel::ChatChannelInfo, String> {
        let name = input
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "CHANNEL_NAME_REQUIRED".to_string())?;
        let channel_type = input
            .channel_type
            .as_deref()
            .ok_or_else(|| "CHANNEL_TYPE_REQUIRED".to_string())?;
        let config_json =
            serde_json::to_string(&create_config(channel_type, input.config.as_ref())?)
                .map_err(|_| "INVALID_INPUT".to_string())?;
        chat_channel::create_chat_channel_core(
            &self.db,
            &self.manager,
            name.to_string(),
            channel_type.to_string(),
            config_json,
            input.enabled.unwrap_or(false),
            input.daily_report_enabled.unwrap_or(false),
            input.daily_report_time.clone(),
        )
        .await
        .map_err(|_| "CHANNEL_SAVE_FAILED".to_string())
    }

    async fn update_channel(
        &self,
        channel_id: i32,
        input: &SaveChannelInput,
    ) -> Result<crate::models::chat_channel::ChatChannelInfo, String> {
        let current = chat_channel_service::get_by_id(&self.db.conn, channel_id)
            .await
            .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
            .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())?;
        if input
            .channel_type
            .as_deref()
            .is_some_and(|kind| kind != current.channel_type)
        {
            return Err("CHANNEL_TYPE_IMMUTABLE".to_string());
        }
        let patch = input
            .config
            .as_ref()
            .map(|config| update_config(&current.channel_type, config));
        let patch_json = patch.transpose()?.map(|value| value.to_string());
        chat_channel::update_chat_channel_core(
            &self.db,
            &self.manager,
            channel_id,
            input.name.clone(),
            input.enabled,
            patch_json,
            None,
            input.daily_report_enabled,
            input.daily_report_time.clone().map(Some),
        )
        .await
        .map_err(|_| "CHANNEL_SAVE_FAILED".to_string())
    }
}

fn safe_save_digest(input: &SaveChannelInput) -> Value {
    json!({
        "channel_id": input.channel_id,
        "name": input.name,
        "channel_type": input.channel_type,
        "enabled": input.enabled,
        "daily_report_enabled": input.daily_report_enabled,
        "daily_report_time": input.daily_report_time,
        "config": input.config,
        "credential": input.credential,
    })
}

fn save_status(runtime_status: &str, target_error: bool) -> &'static str {
    if target_error {
        "saved_with_target_error"
    } else if runtime_status == "error" {
        "saved_with_runtime_error"
    } else {
        "saved"
    }
}

fn require_nonempty(value: &str, code: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(code.to_string())
    } else {
        Ok(())
    }
}

async fn ensure_channel(db: &crate::db::AppDatabase, channel_id: i32) -> Result<(), String> {
    chat_channel_service::get_by_id(&db.conn, channel_id)
        .await
        .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?
        .map(|_| ())
        .ok_or_else(|| "CHANNEL_NOT_FOUND".to_string())
}
