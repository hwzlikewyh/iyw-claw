use super::recall::{ReadyRecall, RecallAttempt};
use super::recall_execute::IndexRecallOutcome;
use super::recall_shadow::RecallShadow;
use super::recall_status::empty_result;
use super::recall_types::UserMemoryRecallResult;

pub(super) fn complete_index_result(
    context: ReadyRecall,
    outcome: IndexRecallOutcome,
) -> UserMemoryRecallResult {
    let IndexRecallOutcome {
        items,
        mut reason_codes,
        shadow,
    } = outcome;
    let abstained = items.is_empty();
    if abstained {
        reason_codes.push("recall_abstained".to_string());
    }
    let reason = if abstained {
        "recall_abstained"
    } else {
        "none"
    };
    shadow.log("index", &items, reason);
    UserMemoryRecallResult {
        query: context.attempt.query,
        items,
        index_generation: context.checkpoint.index_generation,
        source_digest: context.checkpoint.source_digest,
        status: context.checkpoint.status,
        abstained,
        reason_codes,
    }
}

pub(super) fn empty_attempt_result(
    attempt: RecallAttempt,
    status: &'static str,
    reason: &'static str,
) -> UserMemoryRecallResult {
    RecallShadow::new(attempt.started_at).log(status, &[], reason);
    empty_result(attempt.query, status, reason)
}
