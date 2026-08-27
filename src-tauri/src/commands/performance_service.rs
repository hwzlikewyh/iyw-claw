use crate::acp::manager::ConnectionManager;

use super::{collect_stats, AppPerformanceStats};

pub async fn get_performance_stats_core(
    manager: &ConnectionManager,
    db: &sea_orm::DatabaseConnection,
) -> AppPerformanceStats {
    let sessions = manager.runtime_session_snapshots().await;
    #[cfg(feature = "tauri-runtime")]
    let sample = move || collect_stats(sessions, None);
    #[cfg(not(feature = "tauri-runtime"))]
    let sample = move || collect_stats(sessions);
    let mut stats = tokio::task::spawn_blocking(sample)
        .await
        .unwrap_or_default();
    hydrate_conversation_titles(db, &mut stats).await;
    stats
}

#[cfg(feature = "tauri-runtime")]
pub(super) async fn get_performance_stats_with_browser_core(
    manager: &ConnectionManager,
    db: &sea_orm::DatabaseConnection,
    managed_browser: Option<crate::browser::ManagedBrowserProcessSnapshot>,
) -> AppPerformanceStats {
    let sessions = manager.runtime_session_snapshots().await;
    let sample = move || collect_stats(sessions, managed_browser);
    let mut stats = tokio::task::spawn_blocking(sample)
        .await
        .unwrap_or_default();
    hydrate_conversation_titles(db, &mut stats).await;
    stats
}

async fn hydrate_conversation_titles(
    db: &sea_orm::DatabaseConnection,
    stats: &mut AppPerformanceStats,
) {
    for session in &mut stats.agent_sessions {
        let Some(conversation_id) = session.conversation_id else {
            continue;
        };
        if let Ok(conversation) =
            crate::db::service::conversation_service::get_by_id(db, conversation_id).await
        {
            session.conversation_title = conversation.title;
        }
    }
}
