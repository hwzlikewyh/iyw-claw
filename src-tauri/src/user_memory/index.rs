use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};

use crate::app_error::AppCommandError;

use super::candidate_store;
use super::index_checkpoint::{
    database_error, mark_error, mark_stale_if_current, write_ready_checkpoint, IndexFtsStatus,
};
use super::index_fts::{rebuild_fts, FtsLane};
use super::index_integrity::{validate_current_rows, validate_snapshot_identities};
use super::index_parse::{build_index_snapshot, source_digest};
use super::index_types::{IndexItem, IndexRelation, IndexSnapshot};
use super::UserMemoryService;

impl UserMemoryService {
    pub fn schedule_index_refresh(&self) {
        self.schedule_index_refresh_inner(true);
    }

    pub(super) fn schedule_index_refresh_if_due(&self) {
        self.schedule_index_refresh_inner(false);
    }

    fn schedule_index_refresh_inner(&self, force: bool) {
        if tokio::runtime::Handle::try_current().is_err() {
            self.mark_index_unverified();
            tracing::debug!("[memory-index] no runtime; deferred source refresh");
            return;
        }
        if !self.request_index_refresh(force) {
            return;
        }
        tokio::spawn(run_index_refresh_worker(self.clone()));
    }

    pub(crate) async fn refresh_index(&self) -> Result<(), AppCommandError> {
        let _refresh_guard = self.index_refresh_lock.clone().lock_owned().await;
        self.mark_index_unverified();
        let source = self.read_index_source().await?;
        write_index(&self.db, &source)
            .await
            .map_err(database_error)?;

        let latest = self.read_index_source().await?;
        if latest.source_digest != source.source_digest {
            self.schedule_index_refresh();
            let checkpoint = super::recall_status::load_index_status(&self.db).await?;
            if checkpoint.source_digest.as_deref() == Some(source.source_digest.as_str()) {
                if let Some(generation) = checkpoint.index_generation {
                    mark_stale_if_current(
                        &self.db,
                        &source.source_key,
                        &source.source_digest,
                        generation,
                        "source_changed_during_rebuild",
                    )
                    .await
                    .map_err(database_error)?;
                }
            }
        } else {
            self.mark_index_verified_if_idle();
        }
        Ok(())
    }

    pub(super) async fn read_index_source(&self) -> Result<IndexSnapshot, AppCommandError> {
        self.read_index_source_with(|settings, candidates| {
            build_index_snapshot(&settings, candidates.as_ref())
        })
        .await
    }

    pub(crate) async fn read_index_source_digest(&self) -> Result<String, AppCommandError> {
        self.read_index_source_with(|settings, candidates| {
            source_digest(&settings, candidates.as_ref())
        })
        .await
    }

    pub(super) async fn read_index_source_with<T, F>(
        &self,
        project: F,
    ) -> Result<T, AppCommandError>
    where
        T: Send + 'static,
        F: FnOnce(super::UserMemorySettingsSnapshot, Option<super::UserMemoryLearningState>) -> T
            + Send
            + 'static,
    {
        let (io_guard, file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        let policy = self.load_policy().await?;
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            // A timed-out caller drops only the JoinHandle. The blocking job
            // retains both locks until the bounded source projection finishes, then
            // its unobserved result is discarded without racing a writer.
            let _io_guard = io_guard;
            let _file_guard = file_guard;
            let settings = service.snapshot_locked(&policy)?;
            let candidates = candidate_store::read_optional(service.resolved_root()?)?;
            Ok(project(settings, candidates))
        })
        .await
        .map_err(|error| {
            AppCommandError::task_execution_failed("User memory source read task failed")
                .with_detail(error.to_string())
        })?
    }
}

pub(super) async fn write_index(
    conn: &DatabaseConnection,
    snapshot: &IndexSnapshot,
) -> Result<(), sea_orm::DbErr> {
    validate_snapshot_identities(snapshot)?;
    let txn = conn.begin().await?;
    let result = write_index_transaction(&txn, snapshot).await;
    match result {
        Ok(()) => txn.commit().await,
        Err(error) => {
            let _ = txn.rollback().await;
            Err(error)
        }
    }
}

async fn write_index_transaction<C: ConnectionTrait>(
    conn: &C,
    snapshot: &IndexSnapshot,
) -> Result<(), sea_orm::DbErr> {
    for table in [
        "memory_relation_current",
        "memory_evidence",
        "memory_alias_current",
        "memory_item_current",
    ] {
        conn.execute(Statement::from_string(
            DbBackend::Sqlite,
            format!("DELETE FROM {table}"),
        ))
        .await?;
    }

    for item in &snapshot.items {
        insert_item(conn, item).await?;
        insert_aliases(conn, item).await?;
        insert_evidence(conn, item).await?;
    }
    for relation in &snapshot.relations {
        insert_relation(conn, relation).await?;
    }
    validate_current_rows(conn, snapshot).await?;

    let unicode_status = rebuild_fts(conn, FtsLane::Unicode).await;
    let trigram_status = rebuild_fts(conn, FtsLane::Trigram).await;
    write_ready_checkpoint(
        conn,
        snapshot,
        IndexFtsStatus {
            unicode: unicode_status,
            trigram: trigram_status,
        },
    )
    .await
}

async fn run_index_refresh_worker(service: UserMemoryService) {
    loop {
        let requested = {
            let mut state = service
                .index_refresh_state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !state.requested {
                state.running = false;
                false
            } else {
                state.requested = false;
                true
            }
        };
        if !requested {
            break;
        }
        if let Err(error) = service.refresh_index().await {
            record_refresh_error(&service, &error).await;
        }
    }
}

async fn record_refresh_error(service: &UserMemoryService, error: &AppCommandError) {
    service.mark_index_refresh_failed();
    tracing::warn!(error = %error, "[memory-index] source refresh failed");
    if let Err(checkpoint_error) =
        mark_error(&service.db, "user_memory", "index_rebuild_failed").await
    {
        tracing::debug!(
            error = %checkpoint_error,
            "[memory-index] failed to mark checkpoint error"
        );
    }
}

async fn insert_item<C: ConnectionTrait>(conn: &C, item: &IndexItem) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO memory_item_current (id, kind, trust_class, scope_type, scope_key, content, content_digest, confidence, importance, valid_from, valid_to, source_revision, sensitive, superseded_by) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        [
            item.id.clone().into(),
            item.kind.clone().into(),
            item.trust_class.clone().into(),
            item.scope_type.clone().into(),
            item.scope_key.clone().into(),
            item.content.clone().into(),
            item.content_digest.clone().into(),
            item.confidence.into(),
            item.importance.into(),
            item.valid_from.clone().into(),
            item.valid_to.clone().into(),
            item.source_revision.clone().into(),
            (item.sensitive as i64).into(),
        ],
    ))
    .await
    .map(|_| ())
}

async fn insert_aliases<C: ConnectionTrait>(
    conn: &C,
    item: &IndexItem,
) -> Result<(), sea_orm::DbErr> {
    for alias in &item.aliases {
        let (kind, normalized) = super::index_parse::alias_row(alias);
        if normalized.is_empty() {
            continue;
        }
        conn.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO memory_alias_current (memory_id, alias_kind, alias, normalized_alias, scope_type, scope_key) VALUES (?, ?, ?, ?, ?, ?)",
            [
                item.id.clone().into(),
                kind.into(),
                alias.value.clone().into(),
                normalized.into(),
                item.scope_type.clone().into(),
                item.scope_key.clone().into(),
            ],
        ))
        .await?;
    }
    Ok(())
}

async fn insert_evidence<C: ConnectionTrait>(
    conn: &C,
    item: &IndexItem,
) -> Result<(), sea_orm::DbErr> {
    for evidence in &item.evidence {
        conn.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO memory_evidence (memory_id, source_kind, source_id, conversation_id, turn_nonce, excerpt_digest, observed_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                item.id.clone().into(),
                evidence.source_kind.clone().into(),
                evidence.source_id.clone().into(),
                evidence.conversation_id.clone().into(),
                evidence.turn_nonce.into(),
                evidence.excerpt_digest.clone().into(),
                evidence.observed_at.clone().into(),
            ],
        ))
        .await?;
    }
    Ok(())
}

async fn insert_relation<C: ConnectionTrait>(
    conn: &C,
    relation: &IndexRelation,
) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO memory_relation_current (source_id, relation, target_id, confidence, created_at) VALUES (?, ?, ?, ?, ?)",
        [
            relation.source_id.clone().into(),
            relation.relation.clone().into(),
            relation.target_id.clone().into(),
            relation.confidence.into(),
            relation.created_at.clone().into(),
        ],
    ))
    .await
    .map(|_| ())
}
