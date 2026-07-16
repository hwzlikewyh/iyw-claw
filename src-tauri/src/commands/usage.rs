use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::commands::conversations::get_folder_conversation_core;
use crate::db::service::usage_service;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
use crate::models::{DbConversationDetail, SessionUsageSnapshot, UsageDashboardStats};

const AUTO_MODEL: &str = "auto";

fn concrete_model(model: &str) -> Option<String> {
    let model = model.trim();
    (!model.is_empty() && !model.eq_ignore_ascii_case(AUTO_MODEL)).then(|| model.to_owned())
}

fn preferred_snapshot_model<'a>(
    summary_model: Option<&str>,
    turn_models: impl Iterator<Item = Option<&'a str>>,
) -> String {
    let mut latest_turn_model = None;
    for model in turn_models.flatten() {
        if let Some(model) = concrete_model(model) {
            latest_turn_model = Some(model);
        }
    }
    latest_turn_model
        .or_else(|| summary_model.and_then(concrete_model))
        .unwrap_or_else(|| AUTO_MODEL.to_string())
}

fn snapshot_model(detail: &DbConversationDetail) -> String {
    preferred_snapshot_model(
        detail.summary.model.as_deref(),
        detail.turns.iter().map(|turn| turn.model.as_deref()),
    )
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
    upgrade_legacy_usage_snapshots(conn).await?;
    usage_service::get_dashboard(conn)
        .await
        .map_err(AppCommandError::from)
}

async fn upgrade_legacy_usage_snapshots(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    let snapshots = usage_service::list_session_snapshots(conn)
        .await
        .map_err(AppCommandError::from)?;
    for snapshot in snapshots
        .into_iter()
        .filter(|snapshot| snapshot.model.eq_ignore_ascii_case(AUTO_MODEL))
    {
        if let Err(error) = record_conversation_usage_core(conn, snapshot.conversation_id).await {
            tracing::warn!(
                "failed to upgrade usage snapshot for conversation {}: {}",
                snapshot.conversation_id,
                error
            );
        }
    }
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_usage_dashboard(
    db: tauri::State<'_, AppDatabase>,
) -> Result<UsageDashboardStats, AppCommandError> {
    get_usage_dashboard_core(&db.conn).await
}

#[cfg(test)]
mod tests {
    use super::preferred_snapshot_model;

    #[test]
    fn concrete_turn_model_overrides_auto_summary() {
        let model = preferred_snapshot_model(
            Some("auto"),
            [None, Some(" gpt-5.4 "), Some("")].into_iter(),
        );

        assert_eq!(model, "gpt-5.4");
    }
}
