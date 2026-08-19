use std::path::Path;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

const SOURCE_KEY: &str = "user_memory";
const RESTORE_REASON: &str = "restore_source_changed";

#[derive(Debug, Default)]
pub struct RestoreMemoryStartup {
    source_changed: bool,
    checkpoint_marked_stale: bool,
}

impl RestoreMemoryStartup {
    pub fn requires_index_refresh(&self) -> bool {
        self.source_changed
    }

    pub fn schedule_index_refresh(&self, schedule: impl FnOnce()) {
        if self.requires_index_refresh() {
            schedule();
        }
    }

    fn log_refresh_required(self) -> Self {
        tracing::info!(
            checkpoint_marked_stale = self.checkpoint_marked_stale,
            "[RESTORE] user-memory index rebuild required"
        );
        self
    }
}

pub(super) async fn record_restore_source_changed(
    conn: &DatabaseConnection,
    data_dir: &Path,
    source_changed: bool,
) -> RestoreMemoryStartup {
    let marker = data_dir.join(crate::commands::backup::restore::RESTORE_SOURCE_CHANGED_MARKER);
    if !source_changed && !restore_marker_pending(&marker) {
        return RestoreMemoryStartup::default();
    }
    let checkpoint_marked_stale = match mark_checkpoint_stale(conn).await {
        Ok(()) => {
            consume_restore_marker(&marker);
            true
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                "[RESTORE] failed to mark user-memory checkpoint stale; rebuild still required"
            );
            false
        }
    };
    RestoreMemoryStartup {
        source_changed: true,
        checkpoint_marked_stale,
    }
    .log_refresh_required()
}

fn restore_marker_pending(marker: &Path) -> bool {
    match marker.try_exists() {
        Ok(pending) => pending,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "[RESTORE] failed to inspect user-memory rebuild marker; rebuild still required"
            );
            true
        }
    }
}

fn consume_restore_marker(marker: &Path) {
    if let Err(error) = std::fs::remove_file(marker) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                error = %error,
                "[RESTORE] failed to consume user-memory rebuild marker"
            );
        }
    }
}

async fn mark_checkpoint_stale(conn: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO memory_source_checkpoint (source_key, source_digest, index_generation, indexed_at, status, fts_unicode_status, fts_trigram_status, last_error) \
         VALUES (?, '', 0, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'stale', 'unverified', 'unverified', ?) \
         ON CONFLICT(source_key) DO UPDATE SET status = 'stale', last_error = excluded.last_error",
        [SOURCE_KEY.into(), RESTORE_REASON.into()],
    ))
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement, TryGetable};
    use tempfile::tempdir;

    use super::{record_restore_source_changed, RESTORE_REASON, SOURCE_KEY};
    use crate::commands::backup::restore::RESTORE_SOURCE_CHANGED_MARKER;

    async fn open_checkpoint_database() -> sea_orm::DatabaseConnection {
        let conn = Database::connect("sqlite::memory:").await.unwrap();
        conn.execute_unprepared(
            "CREATE TABLE memory_source_checkpoint (source_key TEXT PRIMARY KEY, source_digest TEXT NOT NULL, index_generation INTEGER NOT NULL, indexed_at TEXT NOT NULL, status TEXT NOT NULL, fts_unicode_status TEXT NOT NULL, fts_trigram_status TEXT NOT NULL, last_error TEXT)",
        )
        .await
        .unwrap();
        conn
    }

    async fn checkpoint_status(conn: &sea_orm::DatabaseConnection) -> Option<(String, String)> {
        conn.query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT status, last_error FROM memory_source_checkpoint WHERE source_key = ?",
            [SOURCE_KEY.into()],
        ))
        .await
        .unwrap()
        .map(|row| {
            (
                String::try_get(&row, "", "status").unwrap(),
                String::try_get(&row, "", "last_error").unwrap(),
            )
        })
    }

    #[tokio::test]
    async fn unchanged_restore_does_not_create_checkpoint() {
        let conn = open_checkpoint_database().await;
        let data_dir = tempdir().unwrap();

        let state = record_restore_source_changed(&conn, data_dir.path(), false).await;

        assert!(!state.source_changed);
        assert!(!state.requires_index_refresh());
        let mut scheduled = false;
        state.schedule_index_refresh(|| scheduled = true);
        assert!(!scheduled);
        assert_eq!(checkpoint_status(&conn).await, None);
    }

    #[tokio::test]
    async fn changed_restore_marks_checkpoint_stale() {
        let conn = open_checkpoint_database().await;
        let data_dir = tempdir().unwrap();

        let state = record_restore_source_changed(&conn, data_dir.path(), true).await;

        assert!(state.source_changed);
        assert!(state.checkpoint_marked_stale);
        assert!(state.requires_index_refresh());
        let mut scheduled = false;
        state.schedule_index_refresh(|| scheduled = true);
        assert!(scheduled);
        assert_eq!(
            checkpoint_status(&conn).await,
            Some(("stale".to_string(), RESTORE_REASON.to_string()))
        );
    }

    #[tokio::test]
    async fn checkpoint_failure_keeps_refresh_required() {
        let conn = Database::connect("sqlite::memory:").await.unwrap();
        let data_dir = tempdir().unwrap();
        let marker = data_dir.path().join(RESTORE_SOURCE_CHANGED_MARKER);
        std::fs::write(&marker, RESTORE_REASON).unwrap();

        let state = record_restore_source_changed(&conn, data_dir.path(), false).await;

        assert!(state.source_changed);
        assert!(!state.checkpoint_marked_stale);
        assert!(state.requires_index_refresh());
        let mut scheduled = false;
        state.schedule_index_refresh(|| scheduled = true);
        assert!(scheduled);
        assert!(marker.is_file());
    }

    #[tokio::test]
    async fn successful_checkpoint_consumes_crash_handoff() {
        let conn = open_checkpoint_database().await;
        let data_dir = tempdir().unwrap();
        let marker = data_dir.path().join(RESTORE_SOURCE_CHANGED_MARKER);
        std::fs::write(&marker, RESTORE_REASON).unwrap();

        let state = record_restore_source_changed(&conn, data_dir.path(), false).await;

        assert!(state.source_changed);
        assert!(state.checkpoint_marked_stale);
        assert!(state.requires_index_refresh());
        assert!(!marker.exists());
    }
}
