pub mod dingtalk;
pub mod lark;
pub mod wecom;
pub mod wecom_ai_bot;
pub mod weixin;

use sea_orm::DatabaseConnection;

use super::error::ChatChannelError;
use super::traits::ChatChannelBackend;
/// Sent back to the originating chat when the dispatcher queue is full so a
/// message is never silently dropped (bounded queue contract).
pub const DISPATCHER_BUSY_TEXT: &str =
    "系统繁忙，请稍后重试（消息队列已满）\nBusy — the message queue is full, please retry in a moment.";
use super::types::*;

/// Factory function to create a backend instance from channel type, config, and token.
/// Eliminates duplicated match blocks across connect, test, and auto-connect paths.
pub fn create_backend(
    channel_id: i32,
    channel_type: ChannelType,
    config: &serde_json::Value,
    token: String,
    database: DatabaseConnection,
) -> Result<Box<dyn ChatChannelBackend>, ChatChannelError> {
    match channel_type {
        ChannelType::Wecom => {
            let cfg: WecomConfig = serde_json::from_value(config.clone()).map_err(|e| {
                ChatChannelError::ConfigurationInvalid(format!("Invalid WeCom config: {e}"))
            })?;
            // Credentials live in wecom-cli's own store (QR-scan auth), so no
            // token is required here.
            let _ = token;
            Ok(Box::new(wecom::WecomBackend::new(channel_id, cfg)))
        }
        ChannelType::Weixin => {
            let cfg: WeixinConfig = serde_json::from_value(config.clone()).map_err(|e| {
                ChatChannelError::ConfigurationInvalid(format!("Invalid Weixin config: {e}"))
            })?;
            if cfg.base_url.is_empty() {
                return Err(ChatChannelError::ConfigurationInvalid(
                    "base_url is required".into(),
                ));
            }
            Ok(Box::new(weixin::WeixinBackend::new(
                channel_id,
                token,
                cfg.base_url,
                database,
            )))
        }
        ChannelType::Lark => {
            let cfg: LarkConfig = serde_json::from_value(config.clone()).map_err(|e| {
                ChatChannelError::ConfigurationInvalid(format!("Invalid Lark config: {e}"))
            })?;
            if cfg.app_id.is_empty() || cfg.chat_id.is_empty() {
                return Err(ChatChannelError::ConfigurationInvalid(
                    "app_id and chat_id are required".into(),
                ));
            }
            Ok(Box::new(lark::LarkBackend::new(
                channel_id,
                cfg.app_id,
                token,
                cfg.chat_id,
            )))
        }
        ChannelType::WecomAiBot => {
            let cfg: WecomAiBotConfig =
                serde_json::from_value(config.clone()).map_err(|error| {
                    ChatChannelError::ConfigurationInvalid(format!(
                        "Invalid WeCom AI Bot config: {error}"
                    ))
                })?;
            Ok(Box::new(wecom_ai_bot::WecomAiBotBackend::new(
                channel_id, cfg, token,
            )))
        }
        ChannelType::Dingtalk => {
            let cfg: DingtalkConfig = serde_json::from_value(config.clone()).map_err(|error| {
                ChatChannelError::ConfigurationInvalid(format!("Invalid DingTalk config: {error}"))
            })?;
            if cfg.client_id.trim().is_empty() {
                return Err(ChatChannelError::ConfigurationInvalid(
                    "client_id is required".into(),
                ));
            }
            Ok(Box::new(dingtalk::DingtalkBackend::new(
                channel_id, cfg, token,
            )))
        }
        ChannelType::WecomAgent => Err(ChatChannelError::ConfigurationInvalid(
            "WeCom Agent callback backend is not available yet; use WeCom AI Bot".into(),
        )),
    }
}
