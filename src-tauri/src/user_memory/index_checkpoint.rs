use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

use crate::app_error::AppCommandError;

use super::index_types::IndexSnapshot;

pub(super) struct IndexFtsStatus {
    pub unicode: String,
    pub trigram: String,
}

struct CheckpointState<'a> {
    status: &'a str,
    reason: &'a str,
}

pub(super) async fn mark_stale(
    conn: &DatabaseConnection,
    source_key: &str,
    reason: &str,
) -> Result<(), sea_orm::DbErr> {
    update_status(
        conn,
        source_key,
        CheckpointState {
            status: "stale",
            reason,
        },
    )
    .await
}

pub(super) async fn mark_error(
    conn: &DatabaseConnection,
    source_key: &str,
    reason: &str,
) -> Result<(), sea_orm::DbErr> {
    update_status(
        conn,
        source_key,
        CheckpointState {
            status: "error",
            reason,
        },
    )
    .await
}

pub(super) async fn mark_stale_if_current(
    conn: &DatabaseConnection,
    source_key: &str,
    expected_digest: &str,
    expected_generation: i64,
    reason: &str,
) -> Result<bool, sea_orm::DbErr> {
    let result = conn
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE memory_source_checkpoint SET status = 'stale', last_error = ? WHERE source_key = ? AND source_digest = ? AND index_generation = ? AND status IN ('ready', 'ready_fallback')",
            [
                reason.to_string().into(),
                source_key.to_string().into(),
                expected_digest.to_string().into(),
                expected_generation.into(),
            ],
        ))
        .await?;
    Ok(result.rows_affected() == 1)
}

async fn update_status(
    conn: &DatabaseConnection,
    source_key: &str,
    state: CheckpointState<'_>,
) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE memory_source_checkpoint SET status = ?, last_error = ? WHERE source_key = ?",
        [
            state.status.to_string().into(),
            state.reason.to_string().into(),
            source_key.to_string().into(),
        ],
    ))
    .await
    .map(|_| ())
}

pub(super) async fn write_ready_checkpoint<C: ConnectionTrait>(
    conn: &C,
    snapshot: &IndexSnapshot,
    fts: IndexFtsStatus,
) -> Result<(), sea_orm::DbErr> {
    let status = if fts.unicode == "ready" || fts.trigram == "ready" {
        "ready"
    } else {
        "ready_fallback"
    };
    let last_error = [
        (fts.unicode != "ready").then_some(fts.unicode.as_str()),
        (fts.trigram != "ready").then_some(fts.trigram.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(",");
    conn.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO memory_source_checkpoint (source_key, source_digest, index_generation, indexed_at, status, fts_unicode_status, fts_trigram_status, last_error) \
         VALUES (?, ?, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?, ?, ?, NULLIF(?, '')) \
         ON CONFLICT(source_key) DO UPDATE SET source_digest = excluded.source_digest, \
         index_generation = memory_source_checkpoint.index_generation + 1, indexed_at = excluded.indexed_at, \
         status = excluded.status, fts_unicode_status = excluded.fts_unicode_status, \
         fts_trigram_status = excluded.fts_trigram_status, last_error = excluded.last_error",
        [
            snapshot.source_key.clone().into(),
            snapshot.source_digest.clone().into(),
            status.into(),
            fts.unicode.into(),
            fts.trigram.into(),
            last_error.into(),
        ],
    ))
    .await
    .map(|_| ())
}

pub(super) fn database_error(error: sea_orm::DbErr) -> AppCommandError {
    AppCommandError::database_error("User memory index update failed")
        .with_detail(error.to_string())
}
