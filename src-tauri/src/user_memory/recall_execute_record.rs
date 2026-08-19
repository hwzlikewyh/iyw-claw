use std::time::Instant;

use crate::app_error::AppCommandError;

use super::recall_execute::RecallAccumulator;
use super::recall_query::LaneCollection;
use super::recall_shadow::{LaneMeasurement, RecallShadow};

pub(super) struct QueryLaneResult {
    pub lane: &'static str,
    pub started_at: Instant,
    pub result: Result<LaneCollection, AppCommandError>,
    pub error_reason: &'static str,
}

pub(super) fn record_query_result(
    recall: &mut RecallAccumulator,
    result: QueryLaneResult,
) -> Result<(), &'static str> {
    match result.result {
        Ok(outcome) => {
            recall
                .shadow
                .record_lane(lane_measurement(result.lane, result.started_at, outcome));
            Ok(())
        }
        Err(error) => {
            recall.shadow.record_lane(LaneMeasurement::empty(
                result.lane,
                result.started_at,
                "query_error",
            ));
            log_lane_error(result.lane, &error);
            Err(result.error_reason)
        }
    }
}

pub(super) fn record_optional_result(recall: &mut RecallAccumulator, result: QueryLaneResult) {
    if let Err(reason) = record_query_result(recall, result) {
        recall.reason_codes.push(reason.to_string());
    }
}

pub(super) fn record_skipped(shadow: &mut RecallShadow, lane: &'static str, reason: &'static str) {
    shadow.record_lane(LaneMeasurement::empty(lane, Instant::now(), reason));
}

pub(super) fn hydrate_measurement(
    started_at: Instant,
    candidate_count: usize,
    reason: Option<&'static str>,
) -> LaneMeasurement {
    let reason = reason.or_else(|| (candidate_count == 0).then_some("no_ranked_candidates"));
    LaneMeasurement::collected("hydrate", started_at, candidate_count)
        .with_reason(reason)
        .without_score()
}

fn lane_measurement(
    lane: &'static str,
    started_at: Instant,
    outcome: LaneCollection,
) -> LaneMeasurement {
    LaneMeasurement::collected(lane, started_at, outcome.candidate_count)
        .with_reason(outcome.empty_reason)
}

fn log_lane_error(lane: &str, error: &AppCommandError) {
    tracing::warn!(
        error = %error,
        lane,
        "[memory-recall] index lane failed; applying lane policy"
    );
}
