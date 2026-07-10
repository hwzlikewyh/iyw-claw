use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::conversations::get_folder_conversation_core;
use crate::db::service::usage_service;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
use crate::models::{DbConversationDetail, SessionUsageSnapshot, UsageDashboardStats};

const AUTO_MODEL: &str = "auto";

fn snapshot_model(detail: &DbConversationDetail) -> String {
    detail
        .summary
        .model
        .as_deref()
        .filter(|model| !model.trim().is_empty())
        .map(|model| model.trim().to_owned())
        .or_else(|| {
            detail
                .turns
                .iter()
                .rev()
                .filter_map(|turn| turn.model.as_deref())
                .find(|model| !model.trim().is_empty())
                .map(|model| model.trim().to_owned())
        })
        .unwrap_or_else(|| AUTO_MODEL.to_string())
}

fn usage_snapshot(
    conversation_id: i32,
    detail: &DbConversationDetail,
) -> Option<SessionUsageSnapshot> {
    let usage = detail.session_stats.as_ref()?.total_usage.clone()?;
    Some(SessionUsageSnapshot {
        conversation_id,
        date: detail.summary.created_at.format("%Y-%m-%d").to_string(),
        model: snapshot_model(detail),
        usage,
    })
}

pub async fn record_conversation_usage_core(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<bool, AppCommandError> {
    let (detail, _) = get_folder_conversation_core(conn, conversation_id).await?;
    let Some(snapshot) = usage_snapshot(conversation_id, &detail) else {
        return Ok(false);
    };
    usage_service::upsert_session_snapshot(conn, snapshot)
        .await
        .map_err(AppCommandError::from)?;
    Ok(true)
}

pub async fn get_usage_dashboard_core(
    conn: &DatabaseConnection,
) -> Result<UsageDashboardStats, AppCommandError> {
    usage_service::get_dashboard(conn)
        .await
        .map_err(AppCommandError::from)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_usage_dashboard(
    db: tauri::State<'_, AppDatabase>,
) -> Result<UsageDashboardStats, AppCommandError> {
    get_usage_dashboard_core(&db.conn).await
}
