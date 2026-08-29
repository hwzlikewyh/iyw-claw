use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, DbBackend, Statement, Value};

use crate::app_error::AppCommandError;

use super::index_types::normalize_alias;
use super::recall_rank::{add_rows, Candidate, LaneScore};
use super::recall_scope::UserMemoryRecallScope;
use super::recall_status::database_error;
use super::recall_validity::{push_query_at, valid_at_sql};

pub(super) const MAX_LANE_CANDIDATES: usize = 24;
const MAX_RELATION_CANDIDATES: usize = 4;

#[derive(Clone, Copy)]
pub(super) struct RecallQuery<'a> {
    query: &'a str,
    query_at: &'a str,
    scope: &'a UserMemoryRecallScope,
}

impl<'a> RecallQuery<'a> {
    pub(super) fn new(query: &'a str, query_at: &'a str, scope: &'a UserMemoryRecallScope) -> Self {
        Self {
            query,
            query_at,
            scope,
        }
    }

    pub(super) fn query(self) -> &'a str {
        self.query
    }

    pub(super) fn query_at(self) -> &'a str {
        self.query_at
    }

    pub(super) fn scope(self) -> &'a UserMemoryRecallScope {
        self.scope
    }
}

pub(super) struct RelationQuery<'a> {
    seeds: &'a [String],
    query_at: &'a str,
    scope: &'a UserMemoryRecallScope,
}

pub(super) struct LaneCollection {
    pub candidate_count: usize,
    pub empty_reason: Option<&'static str>,
}

impl LaneCollection {
    pub(super) fn collected(candidate_count: usize) -> Self {
        Self {
            candidate_count,
            empty_reason: (candidate_count == 0).then_some("no_candidates"),
        }
    }

    pub(super) fn skipped(reason: &'static str) -> Self {
        Self {
            candidate_count: 0,
            empty_reason: Some(reason),
        }
    }
}

impl<'a> RelationQuery<'a> {
    pub(super) fn new(
        seeds: &'a [String],
        query_at: &'a str,
        scope: &'a UserMemoryRecallScope,
    ) -> Self {
        Self {
            seeds,
            query_at,
            scope,
        }
    }
}

pub(super) async fn collect_exact<C: ConnectionTrait>(
    db: &C,
    query: RecallQuery<'_>,
    out: &mut BTreeMap<String, Candidate>,
) -> Result<LaneCollection, AppCommandError> {
    let validity = valid_at_sql("memory_item_current");
    let scope = query.scope.predicate("memory_item_current");
    let sql = format!(
        "SELECT id FROM memory_item_current WHERE id = ? AND {scope} AND trust_class IN ('host_confirmed', 'agent_experience') AND sensitive = 0 AND superseded_by IS NULL{validity} LIMIT ?"
    );
    let mut values = vec![query.query.to_string().into()];
    query.scope.push_bind(&mut values);
    push_query_at(&mut values, query.query_at);
    values.push((MAX_LANE_CANDIDATES as i64).into());
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await
        .map_err(database_error)?;
    let candidate_count = rows.len();
    add_rows(
        rows,
        out,
        LaneScore {
            name: "exact",
            weight: 1.0,
        },
    );
    Ok(LaneCollection::collected(candidate_count))
}

pub(super) async fn collect_alias<C: ConnectionTrait>(
    db: &C,
    query: RecallQuery<'_>,
    out: &mut BTreeMap<String, Candidate>,
) -> Result<LaneCollection, AppCommandError> {
    let validity = valid_at_sql("i");
    let alias_scope = query.scope.predicate("a");
    let item_scope = query.scope.predicate("i");
    let sql = format!(
        "SELECT a.memory_id FROM memory_alias_current AS a JOIN memory_item_current AS i ON i.id = a.memory_id WHERE a.normalized_alias = ? AND {alias_scope} AND {item_scope} AND i.trust_class IN ('host_confirmed', 'agent_experience') AND i.sensitive = 0 AND i.superseded_by IS NULL{validity} GROUP BY a.memory_id ORDER BY a.memory_id LIMIT ?"
    );
    let mut values = vec![normalize_alias(query.query).into()];
    query.scope.push_bind(&mut values);
    query.scope.push_bind(&mut values);
    push_query_at(&mut values, query.query_at);
    values.push((MAX_LANE_CANDIDATES as i64).into());
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await
        .map_err(database_error)?;
    let candidate_count = rows.len();
    add_rows(
        rows,
        out,
        LaneScore {
            name: "alias",
            weight: 0.9,
        },
    );
    Ok(LaneCollection::collected(candidate_count))
}

pub(super) async fn collect_relations<C: ConnectionTrait>(
    db: &C,
    query: RelationQuery<'_>,
    out: &mut BTreeMap<String, Candidate>,
) -> Result<LaneCollection, AppCommandError> {
    if query.seeds.is_empty() {
        return Ok(LaneCollection::skipped("no_relation_seeds"));
    }
    let placeholders = std::iter::repeat("?")
        .take(query.seeds.len())
        .collect::<Vec<_>>()
        .join(",");
    let validity = valid_at_sql("i");
    let scope = query.scope.predicate("i");
    let sql = format!(
        "SELECT r.target_id FROM memory_relation_current AS r JOIN memory_item_current AS i ON i.id = r.target_id WHERE r.source_id IN ({placeholders}) AND r.relation IN ('supports', 'relates_to', 'related') AND r.confidence >= 50 AND {scope} AND i.trust_class IN ('host_confirmed', 'agent_experience') AND i.sensitive = 0 AND i.superseded_by IS NULL{validity} GROUP BY r.target_id ORDER BY MAX(r.confidence) DESC, r.target_id LIMIT ?"
    );
    let mut values = query
        .seeds
        .iter()
        .cloned()
        .map(Value::from)
        .collect::<Vec<_>>();
    query.scope.push_bind(&mut values);
    push_query_at(&mut values, query.query_at);
    values.push((MAX_RELATION_CANDIDATES as i64).into());
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await
        .map_err(database_error)?;
    let candidate_count = rows.len();
    add_rows(
        rows,
        out,
        LaneScore {
            name: "relation",
            weight: 0.35,
        },
    );
    Ok(LaneCollection::collected(candidate_count))
}
