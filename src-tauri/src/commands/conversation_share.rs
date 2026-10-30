use reqwest::Method;
use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::app_error::AppCommandError;
use crate::commands::skill_market::client;

pub async fn action_core(conn: &DatabaseConnection, action: String, data: Value) -> Result<Value, AppCommandError> {
    if action == "configuration" {
        let endpoint = client::endpoint("/")?;
        let api_base = endpoint.to_string().trim_end_matches('/').to_string();
        let web_base = std::env::var("IYW_CLAW_SHARE_WEB_BASE_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{api_base}/conversation-shares/view"));
        return Ok(serde_json::json!({"apiBase": api_base, "webBase": web_base}));
    }
    let path = match action.as_str() {
        "create" => "/conversation-shares/create",
        "publish" => "/conversation-shares/publish",
        "list" => "/conversation-shares/list",
        "revoke" => "/conversation-shares/revoke",
        _ => return Err(AppCommandError::invalid_input("分享操作无效")),
    };
    tracing::info!(action = %action, "[conversation-share] request started");
    let result = client::send(client::request(conn, Method::POST, path).await?.json(&data)).await;
    if let Err(error) = &result {
        tracing::warn!(action = %action, code = ?error.code, "[conversation-share] request failed");
    }
    result
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn conversation_share_action(
    db: tauri::State<'_, crate::db::AppDatabase>, action: String, data: Value,
) -> Result<Value, AppCommandError> {
    action_core(&db.conn, action, data).await
}
