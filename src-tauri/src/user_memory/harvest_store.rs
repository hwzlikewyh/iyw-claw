use chrono::{Duration, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, QueryResult, TransactionTrait};

use super::harvest::{
    MemoryHarvestRequest, UserMemoryHarvestFailureKind, UserMemoryHarvestRescanPreview,
    UserMemoryHarvestRescanResult, UserMemoryHarvestStatus, UserMemoryHarvestSubmitResult,
    USER_MEMORY_HARVEST_MAX_QUEUED, USER_MEMORY_HARVEST_MAX_RETRIES,
};
use super::harvest_store_sql::{agent_name, execute, query_all, query_one};
use super::index_checkpoint::database_error;
use crate::app_error::AppCommandError;

pub(super) enum StoreOutcome {
    Proposed {
        candidate_ids: Vec<String>,
        experience_ids: Vec<String>,
    },
    Noop(String),
    Failed {
        kind: UserMemoryHarvestFailureKind,
        detail: String,
    },
}

pub(super) async fn recover_interrupted(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    execute(
        conn,
        "UPDATE memory_harvest_outbox SET state = 'queued', updated_at = ? WHERE state = 'extracting'",
        [Utc::now().to_rfc3339().into()],
    )
    .await
    .map(|_| ())
}

pub(super) async fn submit(
    conn: &DatabaseConnection,
    request: &MemoryHarvestRequest,
) -> Result<UserMemoryHarvestSubmitResult, AppCommandError> {
    let txn = conn.begin().await.map_err(database_error)?;
    if find_state(&txn, &request.dedup_key()).await?.is_some() {
        let total = pending_count(&txn).await?;
        txn.rollback().await.map_err(database_error)?;
        return Ok(UserMemoryHarvestSubmitResult {
            enqueued: false,
            duplicate: true,
            queued_total: total,
        });
    }
    let pending = pending_count(&txn).await?;
    if pending >= USER_MEMORY_HARVEST_MAX_QUEUED as u32 {
        txn.rollback().await.map_err(database_error)?;
        return Err(AppCommandError::invalid_input(
            "User memory harvest queue is full",
        ));
    }
    insert_request(&txn, request).await?;
    txn.commit().await.map_err(database_error)?;
    Ok(UserMemoryHarvestSubmitResult {
        enqueued: true,
        duplicate: false,
        queued_total: pending.saturating_add(1),
    })
}

pub(super) async fn status(
    conn: &DatabaseConnection,
) -> Result<UserMemoryHarvestStatus, AppCommandError> {
    let rows = query_all(
        conn,
        "SELECT state, COUNT(*) AS count FROM memory_harvest_outbox GROUP BY state",
        [],
    )
    .await?;
    let mut status = UserMemoryHarvestStatus::default();
    for row in rows {
        let state: String = row.try_get("", "state").map_err(database_error)?;
        let count: i64 = row.try_get("", "count").map_err(database_error)?;
        status.set_count(&state, count.max(0) as u32);
    }
    status.backlog = status.queued + status.extracting + status.failed;
    let latest = query_one(
        conn,
        "SELECT MAX(submitted_at) AS last_harvest_at, MAX(CASE WHEN state = 'proposed' THEN processed_at END) AS last_success_write_at, MAX(CASE WHEN state IN ('failed','dead') THEN processed_at END) AS last_failure_at FROM memory_harvest_outbox",
        [],
    )
    .await?;
    if let Some(row) = latest {
        status.last_harvest_at = row.try_get("", "last_harvest_at").ok();
        status.last_success_write_at = row.try_get("", "last_success_write_at").ok();
        status.last_failure_at = row.try_get("", "last_failure_at").ok();
    }
    Ok(status)
}

pub(super) async fn rescan(
    conn: &DatabaseConnection,
    execute_update: bool,
) -> Result<UserMemoryHarvestRescanResult, AppCommandError> {
    let row = query_one(
        conn,
        "SELECT SUM(CASE WHEN state IN ('queued','extracting') OR (state = 'failed' AND attempts < ?) THEN 1 ELSE 0 END) AS recoverable, SUM(CASE WHEN state IN ('proposed','noop','dead') THEN 1 ELSE 0 END) AS terminal FROM memory_harvest_outbox",
        [i64::from(USER_MEMORY_HARVEST_MAX_RETRIES).into()],
    )
    .await?;
    let preview = UserMemoryHarvestRescanPreview {
        re_queued: row
            .as_ref()
            .and_then(|value| value.try_get::<i64>("", "recoverable").ok())
            .unwrap_or(0)
            .max(0) as u32,
        retained_terminal: row
            .as_ref()
            .and_then(|value| value.try_get::<i64>("", "terminal").ok())
            .unwrap_or(0)
            .max(0) as u32,
    };
    if execute_update {
        execute(
            conn,
            "UPDATE memory_harvest_outbox SET state = 'queued', next_attempt_at = NULL, updated_at = ? WHERE state IN ('queued','extracting') OR (state = 'failed' AND attempts < ?)",
            [Utc::now().to_rfc3339().into(), i64::from(USER_MEMORY_HARVEST_MAX_RETRIES).into()],
        )
        .await?;
    }
    Ok(UserMemoryHarvestRescanResult {
        preview,
        executed: execute_update,
    })
}

pub(super) async fn recoverable(
    conn: &DatabaseConnection,
) -> Result<Vec<MemoryHarvestRequest>, AppCommandError> {
    query_all(
        conn,
        "SELECT * FROM memory_harvest_outbox WHERE state IN ('queued','extracting') OR (state = 'failed' AND attempts < ? AND (next_attempt_at IS NULL OR next_attempt_at <= ?)) ORDER BY id LIMIT 32",
        [i64::from(USER_MEMORY_HARVEST_MAX_RETRIES).into(), Utc::now().to_rfc3339().into()],
    )
    .await?
    .into_iter()
    .map(decode_request)
        .collect()
}

pub(super) async fn next_retry_delay(
    conn: &DatabaseConnection,
) -> Result<Option<std::time::Duration>, AppCommandError> {
    let row = query_one(
        conn,
        "SELECT MIN(next_attempt_at) AS next_attempt_at FROM memory_harvest_outbox WHERE state = 'failed' AND attempts < ? AND next_attempt_at IS NOT NULL",
        [i64::from(USER_MEMORY_HARVEST_MAX_RETRIES).into()],
    )
    .await?;
    let Some(value) = row.and_then(|row| row.try_get::<String>("", "next_attempt_at").ok()) else {
        return Ok(None);
    };
    let Ok(next) = chrono::DateTime::parse_from_rfc3339(&value) else {
        return Ok(Some(std::time::Duration::ZERO));
    };
    Ok((next.with_timezone(&Utc) - Utc::now())
        .to_std()
        .ok()
        .or(Some(std::time::Duration::ZERO)))
}

pub(super) async fn claim(
    conn: &DatabaseConnection,
    dedup_key: &str,
) -> Result<bool, AppCommandError> {
    execute(
        conn,
        "UPDATE memory_harvest_outbox SET state = 'extracting', attempts = attempts + 1, updated_at = ? WHERE dedup_key = ? AND state NOT IN ('proposed','noop','dead')",
        [Utc::now().to_rfc3339().into(), dedup_key.to_string().into()],
    )
    .await
    .map(|result| result.rows_affected() == 1)
}

pub(super) async fn finish(
    conn: &DatabaseConnection,
    dedup_key: &str,
    elapsed_ms: u64,
    outcome: StoreOutcome,
) -> Result<(), AppCommandError> {
    let now = Utc::now().to_rfc3339();
    let (state, failure_kind, failure_detail, noop_reason, candidate_ids, experience_ids) =
        outcome_fields(conn, dedup_key, outcome).await?;
    execute(
        conn,
        "UPDATE memory_harvest_outbox SET state = ?, failure_kind = ?, failure_detail = ?, noop_reason = ?, candidate_ids = ?, experience_ids = ?, processed_at = ?, processing_ms = ?, next_attempt_at = CASE WHEN ? = 'failed' THEN ? ELSE NULL END, updated_at = ? WHERE dedup_key = ?",
        [
            state.clone().into(), failure_kind.into(), failure_detail.into(), noop_reason.into(),
            candidate_ids.into(), experience_ids.into(), now.clone().into(), (elapsed_ms as i64).into(),
            state.into(), (Utc::now() + Duration::seconds(2)).to_rfc3339().into(), now.into(), dedup_key.to_string().into(),
        ],
    )
    .await
    .map(|_| ())
}

async fn outcome_fields(
    conn: &DatabaseConnection,
    dedup_key: &str,
    outcome: StoreOutcome,
) -> Result<
    (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    AppCommandError,
> {
    Ok(match outcome {
        StoreOutcome::Proposed {
            candidate_ids,
            experience_ids,
        } => (
            "proposed".into(),
            None,
            None,
            None,
            json_ids(candidate_ids)?,
            json_ids(experience_ids)?,
        ),
        StoreOutcome::Noop(reason) => ("noop".into(), None, None, Some(reason), None, None),
        StoreOutcome::Failed { kind, detail } => {
            let attempts = query_one(
                conn,
                "SELECT attempts FROM memory_harvest_outbox WHERE dedup_key = ?",
                [dedup_key.to_string().into()],
            )
            .await?
            .and_then(|row| row.try_get::<i64>("", "attempts").ok())
            .unwrap_or(0);
            let state = if attempts >= i64::from(USER_MEMORY_HARVEST_MAX_RETRIES) {
                "dead"
            } else {
                "failed"
            };
            (
                state.into(),
                Some(failure_kind_name(kind).into()),
                Some(detail),
                None,
                None,
                None,
            )
        }
    })
}

async fn insert_request<C: ConnectionTrait>(
    conn: &C,
    request: &MemoryHarvestRequest,
) -> Result<(), AppCommandError> {
    let now = Utc::now().to_rfc3339();
    execute(conn, "INSERT INTO memory_harvest_outbox (dedup_key, conversation_id, turn_nonce, agent_type, workspace_key, stop_reason, user_input_ref, assistant_input_ref, tool_outcome_ref, submitted_at, state, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'queued', ?)", [
        request.dedup_key().into(), request.conversation.clone().into(), (request.turn_nonce as i64).into(), agent_name(request.agent_type).into(), request.workspace_key.clone().into(), request.stop_reason.clone().into(), request.user_input_ref.clone().into(), request.assistant_input_ref.clone().into(), request.tool_outcome_ref.clone().into(), request.submitted_at.clone().into(), now.into(),
    ]).await.map(|_| ())
}

fn decode_request(row: QueryResult) -> Result<MemoryHarvestRequest, AppCommandError> {
    let agent: String = row.try_get("", "agent_type").map_err(database_error)?;
    Ok(MemoryHarvestRequest {
        conversation: row.try_get("", "conversation_id").map_err(database_error)?,
        turn_nonce: row
            .try_get::<i64>("", "turn_nonce")
            .map_err(database_error)?
            .max(0) as u64,
        agent_type: serde_json::from_value(serde_json::Value::String(agent))
            .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?,
        workspace_key: row.try_get("", "workspace_key").ok(),
        stop_reason: row.try_get("", "stop_reason").ok(),
        user_input_ref: row.try_get("", "user_input_ref").ok(),
        assistant_input_ref: row.try_get("", "assistant_input_ref").ok(),
        tool_outcome_ref: row.try_get("", "tool_outcome_ref").ok(),
        submitted_at: row.try_get("", "submitted_at").map_err(database_error)?,
    })
}

async fn find_state<C: ConnectionTrait>(
    conn: &C,
    key: &str,
) -> Result<Option<String>, AppCommandError> {
    Ok(query_one(
        conn,
        "SELECT state FROM memory_harvest_outbox WHERE dedup_key = ?",
        [key.to_string().into()],
    )
    .await?
    .and_then(|row| row.try_get("", "state").ok()))
}
async fn pending_count<C: ConnectionTrait>(conn: &C) -> Result<u32, AppCommandError> {
    Ok(query_one(conn, "SELECT COUNT(*) AS count FROM memory_harvest_outbox WHERE state NOT IN ('proposed','noop','dead')", []).await?.and_then(|row| row.try_get::<i64>("", "count").ok()).unwrap_or(0).max(0) as u32)
}
fn failure_kind_name(kind: UserMemoryHarvestFailureKind) -> &'static str {
    match kind {
        UserMemoryHarvestFailureKind::Io => "io",
        UserMemoryHarvestFailureKind::InvalidInput => "invalid_input",
        UserMemoryHarvestFailureKind::SensitiveContent => "sensitive_content",
        UserMemoryHarvestFailureKind::Internal => "internal",
    }
}
fn json_ids(ids: Vec<String>) -> Result<Option<String>, AppCommandError> {
    if ids.is_empty() {
        Ok(None)
    } else {
        serde_json::to_string(&ids)
            .map(Some)
            .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))
    }
}
