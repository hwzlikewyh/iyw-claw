use std::collections::BTreeMap;
use std::time::Instant;

use sea_orm::ConnectionTrait;

use super::recall::ReadyRecall;
use super::recall_conflict::{filter_conflict_seeds, filter_ranked_conflicts};
use super::recall_execute_record::{
    hydrate_measurement, record_optional_result, record_query_result, QueryLaneResult,
};
use super::recall_fts::{collect_fts_lanes, record_fts_skipped};
use super::recall_hydrate::{hydrate, HydrateRequest};
use super::recall_query::{
    collect_alias, collect_exact, collect_relations, RecallQuery, RelationQuery,
};
use super::recall_rank::{has_lane_candidate, ranked_candidates, relation_seed_ids, Candidate};
use super::recall_shadow::RecallShadow;
use super::recall_temporal::{collect_temporal, is_pure_temporal_query};
use super::recall_types::UserMemoryRecallItem;

pub(super) struct RecallAccumulator {
    pub(super) candidates: BTreeMap<String, Candidate>,
    pub(super) reason_codes: Vec<String>,
    pub(super) shadow: RecallShadow,
}

impl RecallAccumulator {
    fn new(started_at: Instant) -> Self {
        Self {
            candidates: BTreeMap::new(),
            reason_codes: Vec::new(),
            shadow: RecallShadow::new(started_at),
        }
    }

    fn failure(self, reason: &'static str) -> IndexRecallFailure {
        IndexRecallFailure {
            reason,
            shadow: self.shadow,
        }
    }
}

pub(super) struct IndexRecallOutcome {
    pub items: Vec<UserMemoryRecallItem>,
    pub reason_codes: Vec<String>,
    pub shadow: RecallShadow,
}

pub(super) struct IndexRecallFailure {
    pub reason: &'static str,
    pub shadow: RecallShadow,
}

struct FinishRecall<'a> {
    context: &'a ReadyRecall,
    recall: RecallAccumulator,
    relation_added: bool,
}

struct HydrateOutcome {
    started_at: Instant,
    candidate_count: usize,
    result: Result<Vec<UserMemoryRecallItem>, crate::app_error::AppCommandError>,
}

pub(super) async fn execute_index_recall<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
) -> Result<IndexRecallOutcome, IndexRecallFailure> {
    let mut recall = RecallAccumulator::new(context.attempt.started_at);
    if let Err(reason) = collect_required_lanes(conn, context, &mut recall).await {
        return Err(recall.failure(reason));
    }
    collect_temporal_lane(conn, context, &mut recall).await;
    if fts_is_redundant(context, &recall) {
        record_fts_skipped(&mut recall, "strong_lane_satisfied");
    } else if let Err(reason) = collect_fts_lanes(conn, context, &mut recall).await {
        return Err(recall.failure(reason));
    }
    if let Err(reason) = filter_conflict_seeds(conn, context, &mut recall).await {
        return Err(recall.failure(reason));
    }
    let relation_added = collect_relation_lane(conn, context, &mut recall).await;
    finish_index_recall(
        conn,
        FinishRecall {
            context,
            recall,
            relation_added,
        },
    )
    .await
}

async fn collect_required_lanes<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
    recall: &mut RecallAccumulator,
) -> Result<(), &'static str> {
    let query = RecallQuery::new(
        &context.attempt.query,
        &context.attempt.query_at,
        &context.attempt.scope,
    );
    let started_at = Instant::now();
    let exact = collect_exact(conn, query, &mut recall.candidates).await;
    record_query_result(
        recall,
        QueryLaneResult {
            lane: "exact",
            started_at,
            result: exact,
            error_reason: "exact_error",
        },
    )?;
    let started_at = Instant::now();
    let alias = collect_alias(conn, query, &mut recall.candidates).await;
    record_query_result(
        recall,
        QueryLaneResult {
            lane: "alias",
            started_at,
            result: alias,
            error_reason: "alias_error",
        },
    )
}

async fn collect_temporal_lane<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
    recall: &mut RecallAccumulator,
) {
    let query = RecallQuery::new(
        &context.attempt.query,
        &context.attempt.query_at,
        &context.attempt.scope,
    );
    let started_at = Instant::now();
    let temporal = collect_temporal(conn, query, &mut recall.candidates).await;
    record_optional_result(
        recall,
        QueryLaneResult {
            lane: "temporal",
            started_at,
            result: temporal,
            error_reason: "temporal_error",
        },
    );
}

fn fts_is_redundant(context: &ReadyRecall, recall: &RecallAccumulator) -> bool {
    has_lane_candidate(&recall.candidates, &["exact", "alias"])
        || is_pure_temporal_query(&context.attempt.query)
            && has_lane_candidate(&recall.candidates, &["temporal"])
}

async fn collect_relation_lane<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
    recall: &mut RecallAccumulator,
) -> bool {
    if recall.candidates.len() > context.attempt.limit {
        super::recall_execute_record::record_skipped(
            &mut recall.shadow,
            "relation",
            "ambiguous_relation_seeds",
        );
        return false;
    }
    let before = recall.candidates.len();
    let seeds = relation_seed_ids(&recall.candidates, context.attempt.limit);
    let started_at = Instant::now();
    let relation = collect_relations(
        conn,
        RelationQuery::new(&seeds, &context.attempt.query_at, &context.attempt.scope),
        &mut recall.candidates,
    )
    .await;
    record_optional_result(
        recall,
        QueryLaneResult {
            lane: "relation",
            started_at,
            result: relation,
            error_reason: "relation_error",
        },
    );
    recall.candidates.len() > before
}

async fn finish_index_recall<C: ConnectionTrait>(
    conn: &C,
    finish: FinishRecall<'_>,
) -> Result<IndexRecallOutcome, IndexRecallFailure> {
    let FinishRecall {
        context,
        mut recall,
        relation_added,
    } = finish;
    let union_count = recall.candidates.len();
    let candidates = std::mem::take(&mut recall.candidates);
    let ranked = ranked_candidates(candidates);
    let ranked_count = ranked.len();
    recall.shadow.set_ranking_counts(union_count, ranked_count);
    let ranked = if relation_added {
        match filter_ranked_conflicts(conn, context, &mut recall, ranked).await {
            Ok(ranked) => ranked,
            Err(reason) => return Err(recall.failure(reason)),
        }
    } else {
        ranked
    };
    let ranked = ranked
        .into_iter()
        .take(context.attempt.limit)
        .collect::<Vec<_>>();
    let hydrate_count = ranked.len();
    let started_at = Instant::now();
    let result = hydrate(
        conn,
        HydrateRequest {
            ranked,
            query_at: &context.attempt.query_at,
            limit: context.attempt.limit,
            scope: &context.attempt.scope,
        },
    )
    .await;
    complete_hydrate(
        recall,
        HydrateOutcome {
            started_at,
            candidate_count: hydrate_count,
            result,
        },
    )
}

fn complete_hydrate(
    mut recall: RecallAccumulator,
    outcome: HydrateOutcome,
) -> Result<IndexRecallOutcome, IndexRecallFailure> {
    match outcome.result {
        Ok(items) => {
            recall.shadow.record_lane(hydrate_measurement(
                outcome.started_at,
                outcome.candidate_count,
                None,
            ));
            Ok(IndexRecallOutcome {
                items,
                reason_codes: recall.reason_codes,
                shadow: recall.shadow,
            })
        }
        Err(error) => {
            recall.shadow.record_lane(hydrate_measurement(
                outcome.started_at,
                outcome.candidate_count,
                Some("hydrate_error"),
            ));
            tracing::warn!(error = %error, "[memory-recall] hydrate failed; using source fallback");
            Err(recall.failure("hydrate_error"))
        }
    }
}
