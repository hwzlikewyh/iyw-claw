use crate::db::{error::DbError, service::automation_run_service, AppDatabase};
use crate::models::{AutomationRunPage, AutomationRunQuery};

pub async fn history_core(
    db: &AppDatabase,
    query: AutomationRunQuery,
) -> Result<AutomationRunPage, DbError> {
    automation_run_service::history(&db.conn, query).await
}

pub async fn retry_core(run_id: i32) -> Result<i32, DbError> {
    let engine = crate::automation::engine()
        .ok_or_else(|| DbError::Validation("automation engine not running".into()))?;
    tracing::info!(run_id, "[automation] retry requested");
    engine.retry_run(run_id).await.map_err(DbError::Validation)
}

pub async fn get_core(
    db: &AppDatabase,
    run_id: i32,
) -> Result<crate::models::AutomationRunInfo, DbError> {
    let run = automation_run_service::get(&db.conn, run_id).await?;
    let mut linked = automation_run_service::with_retry_links(&db.conn, vec![run]).await?;
    Ok(linked.remove(0))
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn automation_get_run(
    db: tauri::State<'_, AppDatabase>,
    run_id: i32,
) -> Result<crate::models::AutomationRunInfo, DbError> {
    get_core(&db, run_id).await
}

pub async fn retry_batch_core(db: &AppDatabase, run_ids: Vec<i32>) -> Result<usize, DbError> {
    if crate::automation::engine().is_none() {
        return Err(DbError::Validation("automation engine not running".into()));
    }
    crate::automation::retry_queue::enqueue(&db.conn, run_ids).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn automation_retry_runs(
    db: tauri::State<'_, AppDatabase>,
    run_ids: Vec<i32>,
) -> Result<usize, DbError> {
    retry_batch_core(&db, run_ids).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn automation_run_history(
    db: tauri::State<'_, AppDatabase>,
    query: AutomationRunQuery,
) -> Result<AutomationRunPage, DbError> {
    history_core(&db, query).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn automation_retry_run(run_id: i32) -> Result<i32, DbError> {
    retry_core(run_id).await
}
