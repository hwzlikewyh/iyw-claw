use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;
use crate::commands::mcp_cli_config::account_scope;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishCheckRequest {
    pub folder_name: String,
    pub display_name: String,
    pub package_type: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishCheckResult {
    pub slug: String,
    pub name_available: bool,
    pub slug_available: bool,
}

pub async fn check_publication_core(
    conn: &DatabaseConnection,
    request: PublishCheckRequest,
) -> Result<PublishCheckResult, AppCommandError> {
    let scope = account_scope(conn).await?;
    let builder = super::client::request(conn, reqwest::Method::POST, "/skills/publishing/check")
        .await?
        .json(&request);
    let value = super::client::send(builder).await.map_err(|error| {
        tracing::warn!(error = %error, "[skill-market] publication check failed");
        error
    })?;
    if account_scope(conn).await? != scope {
        return Err(AppCommandError::permission_denied(
            "账号或企业空间已变化，请重新打开上传窗口",
        ));
    }
    super::parse_value(value, None)
}
