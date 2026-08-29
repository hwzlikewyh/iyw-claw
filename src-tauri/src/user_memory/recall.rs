use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use sea_orm::{DatabaseTransaction, TransactionTrait};

use crate::app_error::AppCommandError;

use super::index_checkpoint::mark_stale_if_current;
use super::recall_execute::execute_index_recall;
use super::recall_result::{complete_index_result, empty_attempt_result};
use super::recall_scope::UserMemoryRecallScope;
use super::recall_shadow::RecallShadow;
use super::recall_status::{empty_result, load_index_status, same_checkpoint_generation};
use super::recall_types::{
    UserMemoryIndexStatus, UserMemoryRecallRequest, UserMemoryRecallResult, UserMemoryRecallState,
};
use super::UserMemoryService;

const SOURCE_KEY: &str = "user_memory";
const LOCAL_RECALL_TIMEOUT: Duration = Duration::from_millis(100);
const COLD_RECALL_TIMEOUT: Duration = Duration::from_millis(500);

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
        let timeout = if self.index_verified_for_process() {
            LOCAL_RECALL_TIMEOUT
        } else {
            COLD_RECALL_TIMEOUT
        };
        match tokio::time::timeout(timeout, self.recall_normalized(attempt)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::info!(
                    query_chars,
                    timeout_ms = timeout.as_millis(),
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
        let policy = self.load_policy_unrecovered().await.map_err(|_| ())?;
        Ok(policy.enabled && policy.documents.values().any(|enabled| *enabled))
    }

    async fn ready_recall_context(
        &self,
        attempt: RecallAttempt,
    ) -> Result<ReadyRecall, UserMemoryRecallResult> {
        let (attempt, checkpoint) = self.ready_recall_checkpoint(attempt).await?;
        let source_digest = match self.read_index_source_digest_fast().await {
            Ok(digest) => digest,
            Err(_) => {
                self.mark_recall_checkpoint_stale(&checkpoint, "memory_source_unavailable")
                    .await;
                self.schedule_index_refresh_if_due();
                return Err(unavailable_attempt(
                    attempt,
                    Some(&checkpoint),
                    "memory_source_unavailable",
                ));
            }
        };
        if checkpoint.source_digest.as_deref() != Some(source_digest.as_str()) {
            self.mark_recall_checkpoint_stale(&checkpoint, "index_stale_source")
                .await;
            self.schedule_index_refresh_if_due();
            return Err(unavailable_attempt(
                attempt,
                Some(&checkpoint),
                "index_stale_source",
            ));
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
            return Err(unavailable_attempt(attempt, None, "index_recall_disabled"));
        }
        if !self.index_verified_for_process() {
            self.ensure_index_refresh();
            return Err(unavailable_attempt(attempt, None, "index_unverified"));
        }
        let checkpoint = match self.index_status().await {
            Ok(checkpoint) => checkpoint,
            Err(_) => {
                self.schedule_index_refresh_if_due();
                return Err(unavailable_attempt(attempt, None, "index_unavailable"));
            }
        };
        if checkpoint.status == "ready_fallback" {
            self.schedule_degraded_index_refresh_if_due();
            return Err(unavailable_attempt(
                attempt,
                Some(&checkpoint),
                "index_fts_unavailable",
            ));
        }
        if checkpoint.status != "ready" {
            self.schedule_index_refresh_if_due();
            return Err(unavailable_attempt(
                attempt,
                Some(&checkpoint),
                "index_stale",
            ));
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
                tracing::warn!(error = %error, "[memory-recall] read transaction unavailable");
                return Ok(self
                    .unavailable_ready_context(context, "index_transaction_error", None)
                    .await);
            }
        };
        let current = match load_index_status(&txn).await {
            Ok(current) => current,
            Err(error) => {
                tracing::warn!(error = %error, "[memory-recall] checkpoint read failed");
                let _ = txn.rollback().await;
                return Ok(self
                    .unavailable_ready_context(context, "checkpoint_read_error", None)
                    .await);
            }
        };
        if !same_checkpoint_generation(&context.checkpoint, &current) {
            let _ = txn.rollback().await;
            return Ok(self
                .unavailable_ready_context(context, "index_changed_before_recall", None)
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
                    .unavailable_ready_context(context, failure.reason, Some(failure.shadow))
                    .await);
            }
        };
        if let Err(error) = txn.commit().await {
            tracing::warn!(error = %error, "[memory-recall] read transaction commit failed");
            return Ok(self
                .unavailable_ready_context(context, "index_transaction_error", Some(indexed.shadow))
                .await);
        }
        if self.read_index_source_digest().await.ok().as_deref()
            != Some(context.source_digest.as_str())
        {
            return Ok(self
                .unavailable_ready_context(
                    context,
                    "index_changed_during_recall",
                    Some(indexed.shadow),
                )
                .await);
        }
        Ok(complete_index_result(context, indexed))
    }

    async fn unavailable_ready_context(
        &self,
        context: ReadyRecall,
        reason: &'static str,
        shadow: Option<RecallShadow>,
    ) -> UserMemoryRecallResult {
        self.mark_recall_checkpoint_stale(&context.checkpoint, reason)
            .await;
        self.schedule_index_refresh_if_due();
        let shadow = shadow.unwrap_or_else(|| RecallShadow::new(context.attempt.started_at));
        shadow.log("unavailable", &[], reason);
        unavailable_result(context.attempt, Some(&context.checkpoint), reason)
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

fn unavailable_attempt(
    attempt: RecallAttempt,
    checkpoint: Option<&UserMemoryIndexStatus>,
    reason: &'static str,
) -> UserMemoryRecallResult {
    RecallShadow::new(attempt.started_at).log("unavailable", &[], reason);
    unavailable_result(attempt, checkpoint, reason)
}

fn unavailable_result(
    attempt: RecallAttempt,
    checkpoint: Option<&UserMemoryIndexStatus>,
    reason: &'static str,
) -> UserMemoryRecallResult {
    UserMemoryRecallResult {
        query: attempt.query,
        items: Vec::new(),
        index_generation: checkpoint.and_then(|value| value.index_generation),
        source_digest: checkpoint.and_then(|value| value.source_digest.clone()),
        status: "unavailable".to_string(),
        result_state: UserMemoryRecallState::Unavailable,
        abstained: true,
        reason_codes: vec![
            reason.to_string(),
            "index_rebuild_queued".to_string(),
            "recall_abstained".to_string(),
        ],
    }
}
