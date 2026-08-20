use sea_orm::DatabaseConnection;

use super::backends;
use super::error::ChatChannelError;
use super::manager::ChatChannelManager;
use super::types::{ChannelType, WecomAgentSecrets};
use crate::db::entities::chat_channel;

pub async fn credential_ready(
    _db: &DatabaseConnection,
    manager: &ChatChannelManager,
    model: &chat_channel::Model,
) -> Result<(), String> {
    let channel_type = parse_channel_type(model).map_err(|error| error.to_string())?;
    match channel_type {
        ChannelType::Wecom => legacy_wecom_ready(manager).await,
        ChannelType::WecomAgent => {
            let raw = crate::keyring_store::get_channel_token(model.id)
                .ok_or_else(|| "缺少企业微信自建应用安全凭证".to_string())?;
            WecomAgentSecrets::parse(&raw).map(|_| ())
        }
        ChannelType::Lark
        | ChannelType::Weixin
        | ChannelType::WecomAiBot
        | ChannelType::Dingtalk => token_ready(model.id, channel_type),
    }
}

async fn legacy_wecom_ready(manager: &ChatChannelManager) -> Result<(), String> {
    let data_dir = manager
        .data_dir()
        .await
        .ok_or_else(|| "应用数据目录尚未初始化".to_string())?;
    if !crate::wecom_ai::cli_is_ready(&data_dir) {
        return Err("wecom-cli 未安装，请先点击授权安装".to_string());
    }
    match backends::wecom::auth_status(&data_dir).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("企微尚未完成扫码授权，请先在设置中完成授权".to_string()),
        Err(error) => Err(format!("企微授权状态检查失败：{error}")),
    }
}

fn token_ready(channel_id: i32, channel_type: ChannelType) -> Result<(), String> {
    if crate::keyring_store::get_channel_token(channel_id).is_some() {
        return Ok(());
    }
    let hint = match channel_type {
        ChannelType::Weixin => "请先扫码完成微信授权",
        ChannelType::Dingtalk => "请先保存 Client Secret",
        ChannelType::WecomAiBot => "请先保存 Bot Secret",
        _ => "请先保存 App Secret",
    };
    Err(format!("缺少渠道凭据（{hint}）"))
}

pub(super) fn parse_channel_type(
    model: &chat_channel::Model,
) -> Result<ChannelType, ChatChannelError> {
    serde_json::from_value(serde_json::Value::String(model.channel_type.clone())).map_err(|_| {
        ChatChannelError::ConfigurationInvalid(format!("未知渠道类型：{}", model.channel_type))
    })
}
