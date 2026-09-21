use chrono::Utc;
use sea_orm::{DatabaseConnection, QueryResult, TryGetable};

use super::harvest::{
    UserMemoryHarvestRescanPreview, UserMemoryHarvestRescanResult, USER_MEMORY_HARVEST_MAX_RETRIES,
};
use super::harvest_store::available_slots;
use super::harvest_store_sql::{execute, query_one};
use crate::app_error::AppCommandError;

pub(super) async fn rescan(
    conn: &DatabaseConnection,
    execute_update: bool,
) -> Result<UserMemoryHarvestRescanResult, AppCommandError> {
    let mut preview = preview(conn).await?;
    if execute_update {
        requeue_recoverable(conn).await?;
        preview.recovered_dead = requeue_retryable_dead(conn).await?;
    }
    Ok(UserMemoryHarvestRescanResult {
        preview,
        executed: execute_update,
    })
}

async fn preview(
    conn: &DatabaseConnection,
) -> Result<UserMemoryHarvestRescanPreview, AppCommandError> {
    let row = query_one(
        conn,
        RESCAN_COUNTS_SQL,
        [i64::from(USER_MEMORY_HARVEST_MAX_RETRIES).into()],
    )
    .await?;
    Ok(UserMemoryHarvestRescanPreview {
        re_queued: count(&row, "recoverable"),
        retryable_dead: count(&row, "retryable_dead"),
        retained_terminal: count(&row, "terminal"),
        recovered_dead: 0,
        discovered_unqueued: 0,
        recovered_unqueued: 0,
        skipped_sensitive: 0,
        skipped_context_poor: 0,
    })
}

async fn requeue_recoverable(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    execute(
        conn,
        "UPDATE memory_harvest_outbox SET state='queued',next_attempt_at=NULL,updated_at=? WHERE state IN ('queued','extracting') OR (state='failed' AND attempts<?)",
        [
            Utc::now().to_rfc3339().into(),
            i64::from(USER_MEMORY_HARVEST_MAX_RETRIES).into(),
        ],
    )
    .await
    .map(|_| ())
}

async fn requeue_retryable_dead(conn: &DatabaseConnection) -> Result<u32, AppCommandError> {
    let available = available_slots(conn).await? as i64;
    if available == 0 {
        return Ok(0);
    }
    let result = execute(
        conn,
        REQUEUE_RETRYABLE_DEAD_SQL,
        [Utc::now().to_rfc3339().into(), available.into()],
    )
    .await?;
    Ok(result.rows_affected().min(u64::from(u32::MAX)) as u32)
}

fn count(row: &Option<QueryResult>, field: &str) -> u32 {
    row.as_ref()
        .and_then(|value| value.try_get::<i64>("", field).ok())
        .unwrap_or(0)
        .max(0) as u32
}

const RESCAN_COUNTS_SQL: &str = "SELECT SUM(CASE WHEN state IN ('queued','extracting') OR (state='failed' AND attempts<?) THEN 1 ELSE 0 END) AS recoverable,SUM(CASE WHEN state='dead' AND (COALESCE(failure_detail,'') LIKE 'provider_error_code=ConfigurationInvalid%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=ConfigurationMissing%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=AuthenticationFailed%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=NetworkError%') THEN 1 ELSE 0 END) AS retryable_dead,SUM(CASE WHEN state IN ('proposed','noop') OR (state='dead' AND NOT (COALESCE(failure_detail,'') LIKE 'provider_error_code=ConfigurationInvalid%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=ConfigurationMissing%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=AuthenticationFailed%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=NetworkError%')) THEN 1 ELSE 0 END) AS terminal FROM memory_harvest_outbox";
const REQUEUE_RETRYABLE_DEAD_SQL: &str = "UPDATE memory_harvest_outbox SET state='queued',attempts=0,failure_kind=NULL,failure_detail=NULL,noop_reason=NULL,candidate_ids=NULL,experience_ids=NULL,processed_at=NULL,processing_ms=NULL,next_attempt_at=NULL,updated_at=? WHERE id IN (SELECT id FROM memory_harvest_outbox WHERE state='dead' AND (COALESCE(failure_detail,'') LIKE 'provider_error_code=ConfigurationInvalid%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=ConfigurationMissing%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=AuthenticationFailed%' OR COALESCE(failure_detail,'') LIKE 'provider_error_code=NetworkError%') ORDER BY id LIMIT ?)";
