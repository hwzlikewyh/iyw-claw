use serde_json::{json, Value};

use super::service::{ChannelToolService, MutationStart};
use super::types::{ChannelCaller, SettingsInput, SettingsOperation, WebhookInput};
use crate::chat_channel::natural_router_config::ChatNaturalRouterConfigInput;
use crate::chat_channel::webhook::WebhookConfig;
use crate::commands::chat_channel;
use crate::db::service::chat_channel_service;

impl ChannelToolService {
    pub(super) async fn manage_settings(
        &self,
        caller: &ChannelCaller,
        input: Result<SettingsInput, String>,
    ) -> Result<Value, String> {
        let input = input?;
        match input.operation {
            SettingsOperation::Get => self.get_settings().await,
            SettingsOperation::Patch => self.patch_settings(caller, input).await,
        }
    }

    async fn get_settings(&self) -> Result<Value, String> {
        let command_prefix = chat_channel::get_chat_command_prefix_core(&self.db)
            .await
            .map_err(|_| "SETTINGS_QUERY_FAILED".to_string())?;
        let message_language = chat_channel::get_chat_message_language_core(&self.db)
            .await
            .map_err(|_| "SETTINGS_QUERY_FAILED".to_string())?;
        let event_filter = chat_channel::get_chat_event_filter_core(&self.db)
            .await
            .map_err(|_| "SETTINGS_QUERY_FAILED".to_string())?;
        let webhooks = chat_channel::get_chat_event_webhooks_core(&self.db)
            .await
            .map_err(|_| "SETTINGS_QUERY_FAILED".to_string())?
            .into_iter()
            .map(safe_webhook)
            .collect::<Vec<_>>();
        let router = chat_channel::get_chat_natural_router_config_core(&self.db)
            .await
            .map_err(|_| "SETTINGS_QUERY_FAILED".to_string())?;
        Ok(json!({
            "command_prefix": command_prefix,
            "message_language": message_language,
            "event_filter": event_filter,
            "webhooks": webhooks,
            "natural_router": {
                "enabled": router.enabled,
                "model": router.model,
                "credential_configured": router.has_api_key,
            },
        }))
    }

    async fn patch_settings(
        &self,
        caller: &ChannelCaller,
        input: SettingsInput,
    ) -> Result<Value, String> {
        let request_id = input.request_id.as_deref().ok_or("INVALID_REQUEST_ID")?;
        let patch = input.patch.as_ref().ok_or("SETTINGS_PATCH_REQUIRED")?;
        let digest = serde_json::to_value(patch).map_err(|_| "INVALID_INPUT".to_string())?;
        let start = self
            .begin_mutation(caller, "manage_channel_settings", request_id, digest)
            .await?;
        let MutationStart::Started(model) = start else {
            return match start {
                MutationStart::Return(value) => Ok(value),
                _ => unreachable!(),
            };
        };
        let result = self
            .patch_settings_inner(patch)
            .await
            .map(|_| json!({ "status": "updated" }))
            .unwrap_or_else(super::service::error_value);
        self.finish_mutation(
            caller,
            "manage_channel_settings",
            request_id,
            model,
            result,
            None,
        )
        .await
    }

    async fn patch_settings_inner(
        &self,
        patch: &super::types::SettingsPatchInput,
    ) -> Result<(), String> {
        self.patch_basic_settings(patch).await?;
        self.patch_event_settings(patch).await?;
        self.patch_router_settings(patch).await?;
        self.patch_daily_reports(patch).await
    }

    async fn patch_basic_settings(
        &self,
        patch: &super::types::SettingsPatchInput,
    ) -> Result<(), String> {
        if let Some(prefix) = &patch.command_prefix {
            chat_channel::set_chat_command_prefix_core(&self.db, prefix.clone())
                .await
                .map_err(|_| "INVALID_COMMAND_PREFIX".to_string())?;
        }
        if let Some(language) = &patch.message_language {
            chat_channel::set_chat_message_language_core(&self.db, language.clone())
                .await
                .map_err(|_| "INVALID_MESSAGE_LANGUAGE".to_string())?;
        }
        Ok(())
    }

    async fn patch_event_settings(
        &self,
        patch: &super::types::SettingsPatchInput,
    ) -> Result<(), String> {
        if patch.reset_event_filter == Some(true) {
            chat_channel::set_chat_event_filter_core(&self.db, None)
                .await
                .map_err(|_| "SETTINGS_UPDATE_FAILED".to_string())?;
        } else if let Some(filter) = &patch.event_filter {
            chat_channel::set_chat_event_filter_core(&self.db, Some(filter.clone()))
                .await
                .map_err(|_| "SETTINGS_UPDATE_FAILED".to_string())?;
        }
        if let Some(webhooks) = &patch.webhooks {
            chat_channel::set_chat_event_webhooks_core(&self.db, webhook_configs(webhooks))
                .await
                .map_err(|_| "INVALID_WEBHOOK".to_string())?;
        }
        Ok(())
    }

    async fn patch_router_settings(
        &self,
        patch: &super::types::SettingsPatchInput,
    ) -> Result<(), String> {
        if let Some(router) = &patch.natural_router {
            let current = chat_channel::get_chat_natural_router_config_core(&self.db)
                .await
                .map_err(|_| "SETTINGS_QUERY_FAILED".to_string())?;
            chat_channel::set_chat_natural_router_config_core(
                &self.db,
                ChatNaturalRouterConfigInput {
                    enabled: router.enabled,
                    api_url: current.api_url,
                    model: router.model.clone(),
                    timeout_ms: current.timeout_ms,
                    min_confidence: current.min_confidence,
                },
            )
            .await
            .map_err(|_| "INVALID_ROUTER_SETTINGS".to_string())?;
            if router.delete_api_key == Some(true) {
                chat_channel::delete_chat_natural_router_api_key_core()
                    .map_err(|_| "ROUTER_CREDENTIAL_DELETE_FAILED".to_string())?;
            } else if let Some(api_key) = router.api_key.as_deref() {
                chat_channel::save_chat_natural_router_api_key_core(api_key)
                    .map_err(|_| "ROUTER_CREDENTIAL_SAVE_FAILED".to_string())?;
            }
        }
        Ok(())
    }

    async fn patch_daily_reports(
        &self,
        patch: &super::types::SettingsPatchInput,
    ) -> Result<(), String> {
        if patch.daily_report_enabled.is_none() && patch.daily_report_time.is_none() {
            return Ok(());
        }
        let channels = chat_channel_service::list_all(&self.db.conn)
            .await
            .map_err(|_| "CHANNEL_QUERY_FAILED".to_string())?;
        for channel in channels {
            chat_channel::update_chat_channel_core(
                &self.db,
                &self.manager,
                channel.id,
                None,
                None,
                None,
                None,
                patch.daily_report_enabled,
                patch.daily_report_time.clone().map(Some),
            )
            .await
            .map_err(|_| "SETTINGS_UPDATE_FAILED".to_string())?;
        }
        Ok(())
    }
}

fn webhook_configs(values: &[WebhookInput]) -> Vec<WebhookConfig> {
    values
        .iter()
        .map(|value| WebhookConfig {
            url: value.url.clone(),
            enabled: value.enabled,
        })
        .collect()
}

fn safe_webhook(value: WebhookConfig) -> Value {
    let host = reqwest::Url::parse(&value.url).ok().and_then(|url| {
        let host = url.host_str()?.to_string();
        Some(match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host,
        })
    });
    json!({ "host": host.unwrap_or_else(|| "invalid".to_string()), "enabled": value.enabled })
}
