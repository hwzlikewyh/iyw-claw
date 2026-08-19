use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use sea_orm::{DatabaseTransaction, TransactionTrait};

use crate::app_error::AppCommandError;

use super::index_checkpoint::mark_stale_if_current;
use super::recall_execute::execute_index_recall;
use super::recall_fallback::SourceFallbackRequest;
use super::recall_result::{complete_index_result, empty_attempt_result};
use super::recall_scope::UserMemoryRecallScope;
use super::recall_shadow::RecallShadow;
use super::recall_status::{empty_result, load_index_status, same_checkpoint_generation};
use super::recall_types::{UserMemoryIndexStatus, UserMemoryRecallRequest, UserMemoryRecallResult};
use super::UserMemoryService;

const SOURCE_KEY: &str = "user_memory";
const LOCAL_RECALL_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Clone)]
pub(super) struct RecallAttempt {
    pub(super) query: String,
    pub(super) limit: usize,
    pub(super) query_at: String,
    pub(super) started_at: Instant,
    pub(super) scope: UserMemoryRecallScope,
}

pub(super) struct ReadyRecall {
    pub(super) attempt: RecallAttempt,
    pub(super) checkpoint: UserMemoryIndexStatus,
    pub(super) source_digest: String,
}

impl UserMemoryService {
    pub async fn recall(
        &self,
        request: UserMemoryRecallRequest,
        scope: UserMemoryRecallScope,
    ) -> Result<UserMemoryRecallResult, AppCommandError> {
        let (query, limit) = request.normalized()?;
        let query_chars = query.chars().count();
        let timeout_query = query.clone();
        let started_at = Instant::now();
        let attempt = RecallAttempt {
            query,
            limit,
            query_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            started_at,
            scope,
        };
        match tokio::time::timeout(LOCAL_RECALL_TIMEOUT, self.recall_normalized(attempt)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::info!(
                    query_chars,
                    timeout_ms = LOCAL_RECALL_TIMEOUT.as_millis(),
                    "[memory-recall] local recall timed out; abstaining"
                );
                RecallShadow::new(started_at).log("timeout", &[], "recall_timeout");
                Ok(empty_result(timeout_query, "timeout", "recall_timeout"))
            }
        }
    }

    async fn recall_normalized(
        &self,
        attempt: RecallAttempt,
    ) -> Result<UserMemoryRecallResult, AppCommandError> {
        match self.memory_read_enabled().await {
            Ok(true) => {}
            Ok(false) => {
                return Ok(empty_attempt_result(
                    attempt,
                    "disabled",
                    "memory_read_disabled",
                ));
            }
            Err(()) => {
                return Ok(empty_attempt_result(
                    attempt,
                    "unavailable",
                    "memory_source_unavailable",
                ));
            }
        }
        match self.ready_recall_context(attempt).await {
            Ok(context) => self.recall_ready(context).await,
            Err(result) => Ok(result),
        }
    }

    async fn memory_read_enabled(&self) -> Result<bool, ()> {
        let (_io_guard, _file_guard) = self.acquire_locks().await.map_err(|_| ())?;
        let policy = self.load_policy().await.map_err(|_| ())?;
        Ok(policy.enabled && policy.documents.values().any(|enabled| *enabled))
    }

    async fn ready_recall_context(
        &self,
        attempt: RecallAttempt,
    ) -> Result<ReadyRecall, UserMemoryRecallResult> {
        let (attempt, checkpoint) = self.ready_recall_checkpoint(attempt).await?;
        let source_digest = match self.read_index_source_digest().await {
            Ok(digest) => digest,
            Err(_) => {
                self.mark_recall_checkpoint_stale(&checkpoint, "memory_source_unavailable")
                    .await;
                self.schedule_index_refresh_if_due();
                let fallback = SourceFallbackRequest::new(attempt, "memory_source_unavailable");
                return Err(self.recall_source_fallback(fallback).await);
            }
        };
        if checkpoint.source_digest.as_deref() != Some(source_digest.as_str()) {
            self.mark_recall_checkpoint_stale(&checkpoint, "index_stale_source")
                .await;
            self.schedule_index_refresh_if_due();
            let fallback = SourceFallbackRequest::new(attempt, "index_stale_source");
            return Err(self.recall_source_fallback(fallback).await);
        }
        Ok(ReadyRecall {
            attempt,
            checkpoint,
            source_digest,
        })
    }

    async fn ready_recall_checkpoint(
        &self,
        attempt: RecallAttempt,
    ) -> Result<(RecallAttempt, UserMemoryIndexStatus), UserMemoryRecallResult> {
        if !self.recall_index_enabled {
            tracing::debug!("[memory-recall] indexed recall disabled; using source fallback");
            let fallback = SourceFallbackRequest::new(attempt, "index_recall_disabled");
            return Err(self.recall_source_fallback(fallback).await);
        }
        if !self.index_verified_for_process() {
            self.ensure_index_refresh();
            let fallback = SourceFallbackRequest::new(attempt, "index_unverified");
            return Err(self.recall_source_fallback(fallback).await);
        }
        let checkpoint = match self.index_status().await {
            Ok(checkpoint) => checkpoint,
            Err(_) => {
                self.schedule_index_refresh_if_due();
                let fallback = SourceFallbackRequest::new(attempt, "index_unavailable");
                return Err(self.recall_source_fallback(fallback).await);
            }
        };
        if checkpoint.status == "ready_fallback" {
            tracing::debug!("[memory-recall] FTS lanes unavailable; using source fallback");
            self.schedule_degraded_index_refresh_if_due();
            let fallback = SourceFallbackRequest::new(attempt, "index_fts_unavailable");
            return Err(self.recall_source_fallback(fallback).await);
        }
        if checkpoint.status != "ready" {
            self.schedule_index_refresh_if_due();
            let fallback = SourceFallbackRequest::new(attempt, "index_stale");
            return Err(self.recall_source_fallback(fallback).await);
        }
        Ok((attempt, checkpoint))
    }

    async fn recall_ready(
        &self,
        context: ReadyRecall,
    ) -> Result<UserMemoryRecallResult, AppCommandError> {
        let txn = match self.db.begin().await {
            Ok(txn) => txn,
            Err(error) => {
                tracing::warn!(error = %error, "[memory-recall] read transaction unavailable; using source fallback");
                return Ok(self
                    .fallback_ready_context(context, "index_transaction_error", None)
                    .await);
            }
        };
        let current = match load_index_status(&txn).await {
            Ok(current) => current,
            Err(error) => {
                tracing::warn!(error = %error, "[memory-recall] checkpoint read failed; using source fallback");
                let _ = txn.rollback().await;
                return Ok(self
                    .fallback_ready_context(context, "checkpoint_read_error", None)
                    .await);
            }
        };
        if !same_checkpoint_generation(&context.checkpoint, &current) {
            let _ = txn.rollback().await;
            return Ok(self
                .fallback_ready_context(context, "index_changed_before_recall", None)
                .await);
        }
        self.execute_recall_transaction(context, txn).await
    }

    async fn execute_recall_transaction(
        &self,
        context: ReadyRecall,
        txn: DatabaseTransaction,
    ) -> Result<UserMemoryRecallResult, AppCommandError> {
        let indexed = match execute_index_recall(&txn, &context).await {
            Ok(outcome) => outcome,
            Err(failure) => {
                let _ = txn.rollback().await;
                return Ok(self
                    .fallback_ready_context(context, failure.reason, Some(failure.shadow))
                    .await);
            }
        };
        if let Err(error) = txn.commit().await {
            tracing::warn!(error = %error, "[memory-recall] read transaction commit failed; using source fallback");
            return Ok(self
                .fallback_ready_context(context, "index_transaction_error", Some(indexed.shadow))
                .await);
        }
        if self.read_index_source_digest().await.ok().as_deref()
            != Some(context.source_digest.as_str())
        {
            return Ok(self
                .fallback_ready_context(
                    context,
                    "index_changed_during_recall",
                    Some(indexed.shadow),
                )
                .await);
        }
        Ok(complete_index_result(context, indexed))
    }

    async fn fallback_ready_context(
        &self,
        context: ReadyRecall,
        reason: &'static str,
        shadow: Option<RecallShadow>,
    ) -> UserMemoryRecallResult {
        self.mark_recall_checkpoint_stale(&context.checkpoint, reason)
            .await;
        self.schedule_index_refresh_if_due();
        let fallback = match shadow {
            Some(shadow) => SourceFallbackRequest::with_shadow(context.attempt, reason, shadow),
            None => SourceFallbackRequest::new(context.attempt, reason),
        };
        self.recall_source_fallback(fallback).await
    }

    async fn mark_recall_checkpoint_stale(
        &self,
        checkpoint: &UserMemoryIndexStatus,
        reason: &'static str,
    ) {
        let (Some(digest), Some(generation)) = (
            checkpoint.source_digest.as_deref(),
            checkpoint.index_generation,
        ) else {
            return;
        };
        if let Err(error) =
            mark_stale_if_current(&self.db, SOURCE_KEY, digest, generation, reason).await
        {
            tracing::debug!(error = %error, reason, "[memory-recall] failed to mark checkpoint stale");
        }
    }

    pub async fn index_status(&self) -> Result<UserMemoryIndexStatus, AppCommandError> {
        load_index_status(&self.db).await
    }
}
