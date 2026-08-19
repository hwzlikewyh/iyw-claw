use std::collections::BTreeSet;

use sea_orm::{ConnectionTrait, DbBackend, Statement, Value};

use crate::app_error::AppCommandError;

use super::recall::ReadyRecall;
use super::recall_execute::RecallAccumulator;
use super::recall_scope::UserMemoryRecallScope;
use super::recall_shadow::LaneMeasurement;
use super::recall_status::database_error;
use super::recall_validity::{push_query_at, valid_at_sql};

struct ConflictQuery<'a> {
    context: &'a ReadyRecall,
    ids: &'a [String],
    lane: &'static str,
    started_at: std::time::Instant,
}

struct ConflictMeasurement {
    lane: &'static str,
    started_at: std::time::Instant,
    removed: usize,
}

struct UnresolvedConflictQuery<'a> {
    candidate_ids: &'a [String],
    query_at: &'a str,
    scope: &'a UserMemoryRecallScope,
}

pub(super) async fn filter_conflict_seeds<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
    recall: &mut RecallAccumulator,
) -> Result<(), &'static str> {
    let started_at = std::time::Instant::now();
    let initial_ids =
        super::recall_rank::relation_seed_ids(&recall.candidates, context.attempt.limit);
    let initial_conflicts = query_conflicts(
        conn,
        recall,
        ConflictQuery {
            context,
            ids: &initial_ids,
            lane: "conflict_seed",
            started_at,
        },
    )
    .await?;
    let conflicts = if initial_conflicts.is_empty() {
        initial_conflicts
    } else {
        let all_ids = recall.candidates.keys().cloned().collect::<Vec<_>>();
        query_conflicts(
            conn,
            recall,
            ConflictQuery {
                context,
                ids: &all_ids,
                lane: "conflict_seed",
                started_at,
            },
        )
        .await?
    };
    let before = recall.candidates.len();
    recall.candidates.retain(|id, _| !conflicts.contains(id));
    let removed = before.saturating_sub(recall.candidates.len());
    record_filter(
        recall,
        ConflictMeasurement {
            lane: "conflict_seed",
            started_at,
            removed,
        },
    );
    Ok(())
}

pub(super) async fn filter_ranked_conflicts<C: ConnectionTrait>(
    conn: &C,
    context: &ReadyRecall,
    recall: &mut RecallAccumulator,
    mut ranked: Vec<(String, f64, Vec<String>)>,
) -> Result<Vec<(String, f64, Vec<String>)>, &'static str> {
    let started_at = std::time::Instant::now();
    let ids = ranked
        .iter()
        .map(|(id, _, _)| id.clone())
        .collect::<Vec<_>>();
    let conflicts = query_conflicts(
        conn,
        recall,
        ConflictQuery {
            context,
            ids: &ids,
            lane: "conflict",
            started_at,
        },
    )
    .await?;
    let before = ranked.len();
    ranked.retain(|(id, _, _)| !conflicts.contains(id));
    record_filter(
        recall,
        ConflictMeasurement {
            lane: "conflict",
            started_at,
            removed: before.saturating_sub(ranked.len()),
        },
    );
    Ok(ranked)
}

async fn query_conflicts<C: ConnectionTrait>(
    conn: &C,
    recall: &mut RecallAccumulator,
    query: ConflictQuery<'_>,
) -> Result<BTreeSet<String>, &'static str> {
    unresolved_conflict_ids(
        conn,
        UnresolvedConflictQuery {
            candidate_ids: query.ids,
            query_at: &query.context.attempt.query_at,
            scope: &query.context.attempt.scope,
        },
    )
    .await
    .map_err(|error| {
        tracing::warn!(error = %error, lane = query.lane, "[memory-recall] conflict filter failed");
        recall.shadow.record_lane(LaneMeasurement::empty(
            query.lane,
            query.started_at,
            "query_error",
        ));
        "conflict_query_error"
    })
}

fn record_filter(recall: &mut RecallAccumulator, measurement: ConflictMeasurement) {
    recall.shadow.record_lane(
        LaneMeasurement::collected(
            measurement.lane,
            measurement.started_at,
            measurement.removed,
        )
        .with_reason((measurement.removed > 0).then_some("unresolved_conflict")),
    );
    if measurement.removed > 0
        && !recall
            .reason_codes
            .iter()
            .any(|reason| reason == "unresolved_conflict")
    {
        recall.reason_codes.push("unresolved_conflict".to_string());
    }
}

async fn unresolved_conflict_ids<C: ConnectionTrait>(
    db: &C,
    query: UnresolvedConflictQuery<'_>,
) -> Result<BTreeSet<String>, AppCommandError> {
    if query.candidate_ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(query.candidate_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let source_scope = query.scope.predicate("source");
    let target_scope = query.scope.predicate("target");
    let source_validity = valid_at_sql("source");
    let target_validity = valid_at_sql("target");
    let sql = format!(
        "SELECT DISTINCT r.source_id AS id FROM memory_relation_current AS r \
         JOIN memory_item_current AS source ON source.id = r.source_id \
         JOIN memory_item_current AS target ON target.id = r.target_id \
         WHERE r.source_id IN ({placeholders}) AND r.relation = 'contradicts' \
         AND r.confidence >= 50 AND {source_scope} AND {target_scope} \
         AND source.trust_class = 'host_confirmed' AND source.sensitive = 0 \
         AND source.superseded_by IS NULL AND target.trust_class = 'host_confirmed' \
         AND target.sensitive = 0 AND target.superseded_by IS NULL \
         {source_validity}{target_validity} ORDER BY r.source_id"
    );
    let mut values = query
        .candidate_ids
        .iter()
        .cloned()
        .map(Value::from)
        .collect::<Vec<_>>();
    query.scope.push_bind(&mut values);
    query.scope.push_bind(&mut values);
    push_query_at(&mut values, query.query_at);
    push_query_at(&mut values, query.query_at);
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await
        .map_err(database_error)?;
    rows.into_iter()
        .map(|row| row.try_get("", "id").map_err(database_error))
        .collect()
}
