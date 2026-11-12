use crate::app_error::AppCommandError;
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMarketRequest {
    pub action: String,
    pub body: Value,
}

pub async fn request_core(
    conn: &DatabaseConnection,
    input: UserMarketRequest,
) -> Result<Value, AppCommandError> {
    if !matches!(
        input.action.as_str(),
        "mine"
            | "detail-owned"
            | "save"
            | "publish"
            | "unpublish"
            | "uploads/init"
            | "uploads/complete"
            | "uploads/abort"
    ) || !input.body.is_object()
    {
        return Err(AppCommandError::invalid_input("连接器用户操作无效"));
    }
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    let path = format!("/connector-market/{}", input.action);
    let request =
        crate::commands::skill_market::client::request(conn, reqwest::Method::POST, &path)
            .await?
            .json(&input.body)
            .timeout(std::time::Duration::from_secs(600));
    let result = crate::commands::skill_market::client::send(request).await?;
    if crate::commands::mcp_cli_config::account_scope(conn).await? != scope {
        return Err(AppCommandError::permission_denied(
            "操作期间账号或企业空间已变化，请重新加载",
        ));
    }
    Ok(result)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn connector_market_user_request(
    db: tauri::State<'_, crate::db::AppDatabase>,
    request: UserMarketRequest,
) -> Result<Value, AppCommandError> {
    request_core(&db.conn, request).await
}
