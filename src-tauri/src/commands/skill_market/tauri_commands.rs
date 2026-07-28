use tauri::State;

use crate::db::AppDatabase;
use crate::models::AgentType;

use super::*;

#[tauri::command]
pub async fn skill_market_list(
    params: SkillMarketListParams,
    db: State<'_, AppDatabase>,
) -> Result<SkillMarketListResult, AppCommandError> {
    list_core(&db.conn, params).await
}

#[tauri::command]
pub async fn skill_market_categories(
    db: State<'_, AppDatabase>,
) -> Result<Vec<SkillMarketCategory>, AppCommandError> {
    categories_core(&db.conn).await
}

#[tauri::command]
pub async fn skill_market_detail(
    id: String,
    version: Option<String>,
    db: State<'_, AppDatabase>,
) -> Result<SkillMarketDetail, AppCommandError> {
    detail_core(&db.conn, id, version).await
}

#[tauri::command]
pub async fn skill_market_list_versions(
    id: String,
    db: State<'_, AppDatabase>,
) -> Result<Vec<SkillMarketVersion>, AppCommandError> {
    versions_core(&db.conn, id).await
}

#[tauri::command]
pub async fn skill_market_publish(
    request: SkillMarketPublishRequest,
    db: State<'_, AppDatabase>,
) -> Result<SkillMarketDetail, AppCommandError> {
    publish_core(&db.conn, request).await
}

#[tauri::command]
pub async fn skill_market_add_version(
    request: SkillMarketAddVersionRequest,
    db: State<'_, AppDatabase>,
) -> Result<SkillMarketDetail, AppCommandError> {
    add_version_core(&db.conn, request).await
}

#[tauri::command]
pub async fn skill_market_update_metadata(
    request: SkillMarketMetadataRequest,
    db: State<'_, AppDatabase>,
) -> Result<SkillMarketDetail, AppCommandError> {
    update_metadata_core(&db.conn, request).await
}

#[tauri::command]
pub async fn skill_market_delete(
    id: String,
    db: State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    delete_core(&db.conn, id).await
}

#[tauri::command]
pub async fn skill_market_install(
    id: String,
    version: String,
    agent_type: AgentType,
    db: State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    install::install_core(&db.conn, id, version, agent_type).await
}
