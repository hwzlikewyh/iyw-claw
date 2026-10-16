use std::{sync::Arc, time::Duration};

use sea_orm::DatabaseConnection;
use tokio::sync::Mutex;

use crate::db::{
    error::DbError,
    service::{app_metadata_service, automation_run_service, automation_service},
};

const QUEUE_KEY: &str = "automation_retry_queue";
const MAX_BATCH_SIZE: usize = 50;
const POLL_SECONDS: u64 = 2;
static QUEUE_LOCK: Mutex<()> = Mutex::const_new(());

pub async fn enqueue(db: &DatabaseConnection, ids: Vec<i32>) -> Result<usize, DbError> {
    if ids.is_empty() || ids.len() > MAX_BATCH_SIZE {
        return Err(DbError::Validation(
            "select between 1 and 50 failed runs".into(),
        ));
    }
    let _guard = QUEUE_LOCK.lock().await;
    let mut queue = load(db).await?;
    let initial = queue.len();
    for id in ids {
        let original = automation_run_service::validate_retry(db, id).await?;
        automation_service::get(db, original.automation_id).await?;
        if !queue.contains(&id) {
            queue.push(id);
        }
    }
    if queue.len() > MAX_BATCH_SIZE {
        return Err(DbError::Validation("retry queue is full".into()));
    }
    save(db, &queue).await?;
    let count = queue.len() - initial;
    tracing::info!(count, "[automation] retries queued");
    Ok(count)
}

pub fn spawn(engine: Arc<super::AutomationEngine>, db: DatabaseConnection) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(POLL_SECONDS));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut failed = false;
        loop {
            tick.tick().await;
            match dispatch(&engine, &db).await {
                Err(error) => {
                    if !failed {
                        tracing::warn!(error = %error, "[automation] retry queue processing failed");
                    }
                    failed = true;
                }
                Ok(()) if failed => {
                    tracing::info!("[automation] retry queue processing recovered");
                    failed = false;
                }
                Ok(()) => {}
            }
        }
    });
}

async fn dispatch(
    engine: &super::AutomationEngine,
    db: &DatabaseConnection,
) -> Result<(), DbError> {
    let first = {
        let _guard = QUEUE_LOCK.lock().await;
        load(db).await?.first().copied()
    };
    let Some(id) = first else {
        return Ok(());
    };
    if let Ok(original) = automation_run_service::get(db, id).await {
        if automation_service::has_active_run(db, original.automation_id).await? {
            return Ok(());
        }
    }
    if let Err(error) = engine.retry_run(id).await {
        tracing::warn!(run_id = id, error = %error, "[automation] queued retry did not launch");
    }
    let _guard = QUEUE_LOCK.lock().await;
    let mut queue = load(db).await?;
    queue.retain(|queued| *queued != id);
    save(db, &queue).await
}

async fn load(db: &DatabaseConnection) -> Result<Vec<i32>, DbError> {
    match app_metadata_service::get_value(db, QUEUE_KEY).await? {
        Some(value) => serde_json::from_str(&value)
            .map_err(|error| DbError::Validation(format!("invalid retry queue: {error}"))),
        None => Ok(Vec::new()),
    }
}

async fn save(db: &DatabaseConnection, ids: &[i32]) -> Result<(), DbError> {
    let value =
        serde_json::to_string(ids).map_err(|error| DbError::Validation(error.to_string()))?;
    app_metadata_service::upsert_value(db, QUEUE_KEY, &value).await
}
