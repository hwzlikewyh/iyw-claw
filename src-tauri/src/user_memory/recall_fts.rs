use std::time::Instant;

use sea_orm::ConnectionTrait;

use super::recall::ReadyRecall;
use super::recall_execute::RecallAccumulator;
use super::recall_execute_record::record_skipped;
use super::recall_query::LaneCollection;
use super::recall_query_fts::{collect_fts, FtsQuery};
use super::recall_shadow::LaneMeasurement;

const MAX_TRIGRAM_QUERY_CHARS: usize = 128;

struct FtsLaneRun<'a> {
    context: &'a ReadyRecall,
    recall: &'a mut RecallAccumulator,
    lane: &'static str,
    table: &'static str,
    weight: f64,
}

pub(super) fn record_fts_skipped(recall: &mut RecallAccumulator, reason: &'static str) {
    record_skipped(&mut recall.shadow, "unicode", reason);
    record_skipped(&mut recall.shadow, "trigram", reason);
}

pub(super) async fn collect_fts_lanes<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
    recall: &mut RecallAccumulator,
) -> Result<(), &'static str> {
    collect_unicode_lane(conn, context, recall).await?;
    collect_trigram_lane(conn, context, recall)
        .await
        .map(|_| ())
}

async fn collect_unicode_lane<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
    recall: &mut RecallAccumulator,
) -> Result<usize, &'static str> {
    if context.checkpoint.fts_unicode_status != "ready" {
        recall.reason_codes.push(format!(
            "fts_unicode_{}",
            context.checkpoint.fts_unicode_status
        ));
        record_skipped(&mut recall.shadow, "unicode", "index_not_ready");
        return Ok(0);
    }
    run_fts_lane(
        conn,
        FtsLaneRun {
            context,
            recall,
            lane: "unicode",
            table: "memory_item_fts_unicode",
            weight: 0.75,
        },
    )
    .await
}

async fn collect_trigram_lane<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
    recall: &mut RecallAccumulator,
) -> Result<usize, &'static str> {
    let query_chars = context.attempt.query.chars().count();
    if !(3..=MAX_TRIGRAM_QUERY_CHARS).contains(&query_chars) {
        let reason = if query_chars < 3 {
            "fts_trigram_query_too_short"
        } else {
            "fts_trigram_query_too_long"
        };
        recall.reason_codes.push(reason.to_string());
        record_skipped(&mut recall.shadow, "trigram", reason);
        return Ok(0);
    }
    if context.checkpoint.fts_trigram_status != "ready" {
        recall.reason_codes.push(format!(
            "fts_trigram_{}",
            context.checkpoint.fts_trigram_status
        ));
        record_skipped(&mut recall.shadow, "trigram", "index_not_ready");
        return Ok(0);
    }
    run_fts_lane(
        conn,
        FtsLaneRun {
            context,
            recall,
            lane: "trigram",
            table: "memory_item_fts_trigram",
            weight: 0.6,
        },
    )
    .await
}

async fn run_fts_lane<C: ConnectionTrait>(
    conn: &C,
    run: FtsLaneRun<'_>,
) -> Result<usize, &'static str> {
    let started_at = Instant::now();
    let result = collect_fts(
        conn,
        FtsQuery {
            query: &run.context.attempt.query,
            query_at: &run.context.attempt.query_at,
            table: run.table,
            weight: run.weight,
            scope: &run.context.attempt.scope,
        },
        &mut run.recall.candidates,
    )
    .await;
    record_fts_result(run, started_at, result)
}

fn record_fts_result(
    run: FtsLaneRun<'_>,
    started_at: Instant,
    result: Result<LaneCollection, String>,
) -> Result<usize, &'static str> {
    match result {
        Ok(outcome) => {
            run.recall.shadow.record_lane(
                LaneMeasurement::collected(run.lane, started_at, outcome.candidate_count)
                    .with_reason(outcome.empty_reason),
            );
            Ok(outcome.candidate_count)
        }
        Err(reason) => {
            run.recall.reason_codes.push(reason);
            run.recall.shadow.record_lane(LaneMeasurement::empty(
                run.lane,
                started_at,
                "query_error",
            ));
            Err("fts_query_error")
        }
    }
}
