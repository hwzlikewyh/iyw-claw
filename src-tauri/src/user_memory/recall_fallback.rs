use std::time::Instant;

use chrono::{DateTime, Utc};

use super::index_parse::build_index_snapshot;
use super::index_types::IndexSnapshot;
use super::recall::RecallAttempt;
use super::recall_fallback_scan::{scan_snapshot, FallbackScan, FallbackScanContext};
use super::recall_shadow::{LaneMeasurement, RecallShadow};
use super::recall_types::{UserMemoryRecallResult, UserMemoryRecallState};
use super::UserMemoryService;

#[derive(Clone)]
pub(super) struct SourceFallbackRequest {
    attempt: RecallAttempt,
    reason: &'static str,
    shadow: RecallShadow,
}

struct FallbackResultContext<'a> {
    reason: &'static str,
    attempt: RecallAttempt,
    snapshot: &'a IndexSnapshot,
}

impl SourceFallbackRequest {
    pub(super) fn new(attempt: RecallAttempt, reason: &'static str) -> Self {
        let shadow = RecallShadow::new(attempt.started_at);
        Self {
            attempt,
            reason,
            shadow,
        }
    }

    pub(super) fn with_shadow(
        attempt: RecallAttempt,
        reason: &'static str,
        shadow: RecallShadow,
    ) -> Self {
        Self {
            attempt,
            reason,
            shadow,
        }
    }
}

impl UserMemoryService {
    pub(super) async fn recall_source_fallback(
        &self,
        request: SourceFallbackRequest,
    ) -> UserMemoryRecallResult {
        let query_chars = request.attempt.query.chars().count();
        let failed_request = request.clone();
        match self
            .read_index_source_with(move |settings, candidates| {
                let snapshot = build_index_snapshot(&settings, candidates.as_ref());
                fallback_result(request, &snapshot)
            })
            .await
        {
            Ok(result) => {
                tracing::debug!(
                    reason = ?result.reason_codes.first(),
                    query_chars,
                    item_count = result.items.len(),
                    abstained = result.abstained,
                    invariant_failed = result.reason_codes.iter().any(|reason| reason == "index_invariant_failed"),
                    "[memory-recall] source fallback completed"
                );
                result
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    reason = failed_request.reason,
                    query_chars,
                    "[memory-recall] source fallback unavailable"
                );
                fallback_failed_result(failed_request)
            }
        }
    }
}

fn fallback_result(
    mut request: SourceFallbackRequest,
    snapshot: &IndexSnapshot,
) -> UserMemoryRecallResult {
    let query_at = parse_query_at(&request.attempt.query_at);
    let started_at = Instant::now();
    let scan = scan_snapshot(FallbackScanContext {
        snapshot,
        query: &request.attempt.query,
        limit: request.attempt.limit,
        query_at: query_at.as_ref(),
        scope: &request.attempt.scope,
    });
    record_fallback_scan(&mut request.shadow, &scan, started_at);
    let result = build_fallback_result(
        FallbackResultContext {
            reason: request.reason,
            attempt: request.attempt,
            snapshot,
        },
        scan,
    );
    let reason = if result.abstained {
        "recall_abstained"
    } else {
        "none"
    };
    request.shadow.log("source_fallback", &result.items, reason);
    result
}

fn build_fallback_result(
    context: FallbackResultContext<'_>,
    scan: FallbackScan,
) -> UserMemoryRecallResult {
    let abstained = scan.items.is_empty();
    let mut reason_codes = vec![
        context.reason.to_string(),
        "source_scan_fallback".to_string(),
    ];
    if scan.conflicting_item_count > 0 {
        reason_codes.push("index_invariant_failed".to_string());
    }
    if scan.unresolved_conflict_count > 0 {
        reason_codes.push("unresolved_conflict".to_string());
    }
    if context.attempt.query.chars().count() < 3 {
        reason_codes.push("fallback_short_query_exact_only".to_string());
    }
    if abstained {
        reason_codes.push("recall_abstained".to_string());
    }
    UserMemoryRecallResult {
        query: context.attempt.query,
        items: scan.items,
        index_generation: None,
        source_digest: Some(context.snapshot.source_digest.clone()),
        status: "fallback".to_string(),
        result_state: if abstained {
            UserMemoryRecallState::NoEvidence
        } else {
            UserMemoryRecallState::Matched
        },
        abstained,
        reason_codes,
    }
}

fn fallback_failed_result(mut request: SourceFallbackRequest) -> UserMemoryRecallResult {
    for lane in [
        "source_exact",
        "source_alias",
        "source_lexical",
        "source_temporal",
    ] {
        request.shadow.record_lane(LaneMeasurement::empty(
            lane,
            Instant::now(),
            "source_unavailable",
        ));
    }
    request
        .shadow
        .log("source_fallback", &[], "source_scan_failed");
    UserMemoryRecallResult {
        query: request.attempt.query,
        items: Vec::new(),
        index_generation: None,
        source_digest: None,
        status: "stale".to_string(),
        result_state: UserMemoryRecallState::Unavailable,
        abstained: true,
        reason_codes: vec![
            request.reason.to_string(),
            "source_scan_failed".to_string(),
            "index_rebuild_queued".to_string(),
            "recall_abstained".to_string(),
        ],
    }
}

fn record_fallback_scan(shadow: &mut RecallShadow, scan: &FallbackScan, started_at: Instant) {
    for (lane, count) in [
        ("source_exact", scan.exact_count),
        ("source_alias", scan.alias_count),
        ("source_lexical", scan.lexical_count),
        ("source_temporal", scan.temporal_count),
    ] {
        shadow.record_lane(LaneMeasurement::collected(lane, started_at, count));
    }
    shadow.set_ranking_counts(scan.union_count, scan.items.len());
}

fn parse_query_at(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .ok()
}
