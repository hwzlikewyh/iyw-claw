use std::collections::BTreeMap;

use super::error::AcpError;
use crate::models::AgentType;

// 冷启动时先解析会话模型，保证第一条排队消息就使用该模型保存的窗口。
pub(crate) async fn resolve(
    preferences: &mut BTreeMap<String, String>,
    context: (AgentType, Option<&sea_orm::DatabaseConnection>, Option<i32>),
) -> Result<(), AcpError> {
    let (agent, database, conversation_id) = context;
    if !preferences.contains_key("model") {
        if let Some(model) = conversation_model((agent, database, conversation_id)).await? {
            preferences.insert("model".into(), model);
        }
    }
    let saved = preferences.remove("__iyw_context_windows");
    if preferences.contains_key(super::context_window_config::CONFIG_ID) {
        return Ok(());
    }
    let windows = saved
        .as_deref()
        .map(serde_json::from_str::<BTreeMap<String, String>>)
        .transpose()
        .map_err(|error| {
            AcpError::protocol(format!("Invalid saved context window preferences: {error}"))
        })?
        .unwrap_or_default();
    let model = preferences
        .get("model")
        .map(String::as_str)
        .unwrap_or_else(|| super::model_catalog::default_model_for(agent));
    if let Some(value) = windows.get(model) {
        let tiers = super::model_catalog::context_window_tiers_for(agent, model);
        let valid = value == "auto"
            || value
                .parse::<u64>()
                .is_ok_and(|tokens| tiers.contains(&tokens));
        preferences.insert(
            super::context_window_config::CONFIG_ID.into(),
            if valid { value.clone() } else { "auto".into() },
        );
    }
    Ok(())
}

async fn conversation_model(
    context: (AgentType, Option<&sea_orm::DatabaseConnection>, Option<i32>),
) -> Result<Option<String>, AcpError> {
    let (agent, Some(database), Some(id)) = context else {
        return Ok(None);
    };
    let row = crate::db::service::conversation_service::get_by_id(database, id)
        .await
        .map_err(|error| {
            AcpError::protocol(format!(
                "Cannot load conversation context configuration: {error}"
            ))
        })?;
    if row.agent_type != agent {
        return Ok(None);
    }
    Ok(row
        .model
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty()))
}
