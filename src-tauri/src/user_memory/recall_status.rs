use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement};

use crate::app_error::AppCommandError;

use super::recall_types::{UserMemoryIndexStatus, UserMemoryRecallResult, UserMemoryRecallState};

const SOURCE_KEY: &str = "user_memory";

pub(super) fn index_status_row(row: QueryResult) -> UserMemoryIndexStatus {
    UserMemoryIndexStatus {
        source_key: row
            .try_get("", "source_key")
            .unwrap_or_else(|_| SOURCE_KEY.to_string()),
        source_digest: row.try_get("", "source_digest").ok(),
        index_generation: row.try_get("", "index_generation").ok(),
        indexed_at: row.try_get("", "indexed_at").ok(),
        status: row
            .try_get("", "status")
            .unwrap_or_else(|_| "unknown".to_string()),
        fts_unicode_status: row
            .try_get("", "fts_unicode_status")
            .unwrap_or_else(|_| "unknown".to_string()),
        fts_trigram_status: row
            .try_get("", "fts_trigram_status")
            .unwrap_or_else(|_| "unknown".to_string()),
        last_error: row.try_get("", "last_error").ok(),
    }
}

pub(super) async fn load_index_status<C: ConnectionTrait>(
    conn: &C,
) -> Result<UserMemoryIndexStatus, AppCommandError> {
    let row = conn
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT source_key, source_digest, index_generation, indexed_at, status, fts_unicode_status, fts_trigram_status, last_error FROM memory_source_checkpoint WHERE source_key = ?",
            [SOURCE_KEY.to_string().into()],
        ))
        .await
        .map_err(database_error)?;
    Ok(row.map(index_status_row).unwrap_or_else(not_ready_status))
}

pub(super) fn same_checkpoint_generation(
    expected: &UserMemoryIndexStatus,
    current: &UserMemoryIndexStatus,
) -> bool {
    matches!(current.status.as_str(), "ready" | "ready_fallback")
        && current.source_digest == expected.source_digest
        && current.index_generation == expected.index_generation
}

fn not_ready_status() -> UserMemoryIndexStatus {
    UserMemoryIndexStatus {
        source_key: SOURCE_KEY.to_string(),
        source_digest: None,
        index_generation: None,
        indexed_at: None,
        status: "not_ready".to_string(),
        fts_unicode_status: "unknown".to_string(),
        fts_trigram_status: "unknown".to_string(),
        last_error: None,
    }
}

pub(super) fn empty_result(query: String, status: &str, reason: &str) -> UserMemoryRecallResult {
    UserMemoryRecallResult {
        query,
        items: Vec::new(),
        index_generation: None,
        source_digest: None,
        status: status.to_string(),
        result_state: if unavailable_status(status, reason) {
            UserMemoryRecallState::Unavailable
        } else {
            UserMemoryRecallState::NoEvidence
        },
        abstained: true,
        reason_codes: vec![reason.to_string()],
    }
}

fn unavailable_status(status: &str, reason: &str) -> bool {
    matches!(status, "disabled" | "unavailable" | "timeout" | "stale")
        || reason.contains("unavailable")
        || reason.contains("failed")
        || reason.contains("timeout")
}

pub(super) fn database_error(error: sea_orm::DbErr) -> AppCommandError {
    AppCommandError::database_error("User memory recall failed").with_detail(error.to_string())
}
