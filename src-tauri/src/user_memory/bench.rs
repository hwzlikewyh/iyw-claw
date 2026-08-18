mod connection;
pub mod public_recall;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use sea_orm::{DatabaseConnection, Statement};
use serde::{Deserialize, Serialize};

use self::connection::{capture_plan_samples, explain_plans, CountingConnection};
use super::index_types::{IndexAlias, IndexEvidence, IndexItem, IndexRelation, IndexSnapshot};
use super::recall::{ReadyRecall, RecallAttempt};
use super::recall_execute::execute_index_recall;
use super::recall_status::load_index_status;

pub const BENCH_DENSE_TEMPORAL_MONTH: &str = "2025-07";
const BENCH_DENSE_TEMPORAL_OBSERVED_AT: &str = "2025-07-15T00:00:00Z";

#[derive(Clone)]
pub struct BenchMemoryInput {
    pub id: String,
    pub kind: String,
    pub content: String,
    pub content_digest: String,
    pub aliases: Vec<String>,
    pub scope_type: String,
    pub scope_key: String,
    pub sensitive: bool,
    pub superseded_by: Option<String>,
    pub source_revision: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub relation_ids: Vec<String>,
    pub contradicts_ids: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BenchQuery {
    pub name: String,
    pub text: String,
    pub query_at: String,
    pub limit: usize,
    pub scope_type: String,
    pub scope_key: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BenchRecallItem {
    pub id: String,
    pub lanes: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BenchRecallMeasurement {
    pub name: String,
    pub status: String,
    pub latency_us: u64,
    pub items: Vec<BenchRecallItem>,
    pub abstained: bool,
    pub reason_codes: Vec<String>,
    pub candidate_sql_count: usize,
    pub hydrate_sql_count: usize,
    pub total_sql_count: usize,
}

#[derive(Serialize)]
pub struct BenchBuildMetrics {
    pub item_count: usize,
    pub elapsed_ms: u64,
    pub items_per_second: f64,
}

#[derive(Serialize)]
pub struct BenchStorageMetrics {
    pub db_bytes: u64,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
    pub fts_bytes: Option<u64>,
    pub fts_size_reason: Option<String>,
}

#[derive(Serialize)]
pub struct BenchQueryPlan {
    pub lane: String,
    pub sql: String,
    pub details: Vec<String>,
    pub error: Option<String>,
    pub required_index: Option<String>,
    pub required_index_hit: Option<bool>,
}

pub struct ProductionBench {
    conn: DatabaseConnection,
    db_path: PathBuf,
    plan_samples: Mutex<BTreeMap<String, Statement>>,
}

impl ProductionBench {
    pub async fn create(
        root: &Path,
        source_digest: String,
        items: Vec<BenchMemoryInput>,
    ) -> Result<(Self, BenchBuildMetrics), String> {
        let bench = Self::open(root).await?;
        let metrics = bench.replace(source_digest, items).await?;
        Ok((bench, metrics))
    }

    pub async fn replace(
        &self,
        source_digest: String,
        items: Vec<BenchMemoryInput>,
    ) -> Result<BenchBuildMetrics, String> {
        let item_count = items.len();
        let started = Instant::now();
        let snapshot = build_snapshot(source_digest, &items);
        super::index::write_index(&self.conn, &snapshot)
            .await
            .map_err(|error| error.to_string())?;
        connection::apply_superseded(&self.conn, &items).await?;
        let elapsed = started.elapsed();
        let rate = item_count as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
        Ok(BenchBuildMetrics {
            item_count,
            elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
            items_per_second: rate,
        })
    }

    pub async fn open(root: &Path) -> Result<Self, String> {
        let database =
            crate::db::init_database_with_user_memory_root(root, env!("CARGO_PKG_VERSION"), None)
                .await
                .map_err(|error| error.to_string())?;
        Ok(Self {
            conn: database.conn,
            db_path: root.join(crate::db::database_file_name()),
            plan_samples: Mutex::new(BTreeMap::new()),
        })
    }

    pub async fn recall(&self, query: BenchQuery) -> Result<BenchRecallMeasurement, String> {
        let checkpoint = load_index_status(&self.conn)
            .await
            .map_err(|error| error.to_string())?;
        let started = Instant::now();
        let context = recall_context(&query, checkpoint, started);
        let counted = CountingConnection::new(&self.conn);
        let outcome = execute_index_recall(&counted, &context).await;
        let elapsed = started.elapsed();
        let statements = counted.take_statements();
        capture_plan_samples(&self.plan_samples, &statements);
        let counts = connection::sql_counts(&statements);
        match outcome {
            Ok(outcome) => {
                let super::recall_execute::IndexRecallOutcome {
                    items,
                    reason_codes: mut reason_codes,
                    shadow: _,
                } = outcome;
                let abstained = items.is_empty();
                if abstained {
                    reason_codes.push("recall_abstained".to_string());
                }
                Ok(BenchRecallMeasurement {
                    name: query.name,
                    status: "measured".to_string(),
                    latency_us: elapsed.as_micros().min(u64::MAX as u128) as u64,
                    items: items
                        .into_iter()
                        .map(|item| BenchRecallItem {
                            id: item.id,
                            lanes: item.lanes,
                        })
                        .collect(),
                    abstained,
                    reason_codes,
                    candidate_sql_count: counts.candidate,
                    hydrate_sql_count: counts.hydrate,
                    total_sql_count: counts.total,
                })
            }
            Err(failure) => Err(format!("production recall failed: {}", failure.reason)),
        }
    }

    pub async fn query_plans(&self) -> Vec<BenchQueryPlan> {
        explain_plans(&self.conn, &self.plan_samples).await
    }

    pub async fn index_status(&self) -> Result<super::UserMemoryIndexStatus, String> {
        load_index_status(&self.conn)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn storage_metrics(&self) -> BenchStorageMetrics {
        connection::storage_metrics(&self.conn, &self.db_path).await
    }

    pub async fn close(self) -> Result<(), String> {
        self.conn.close().await.map_err(|error| error.to_string())
    }
}

fn build_snapshot(source_digest: String, items: &[BenchMemoryInput]) -> IndexSnapshot {
    IndexSnapshot {
        source_key: "user_memory".to_string(),
        source_digest,
        items: items.iter().map(index_item).collect(),
        relations: index_relations(items),
    }
}

fn index_item(input: &BenchMemoryInput) -> IndexItem {
    IndexItem {
        id: input.id.clone(),
        kind: input.kind.clone(),
        trust_class: "host_confirmed".to_string(),
        scope_type: input.scope_type.clone(),
        scope_key: input.scope_key.clone(),
        content: input.content.clone(),
        content_digest: input.content_digest.clone(),
        confidence: 100,
        importance: 0.5,
        sensitive: input.sensitive,
        valid_from: input.valid_from.clone(),
        valid_to: input.valid_to.clone(),
        source_revision: input.source_revision.clone(),
        aliases: input.aliases.iter().map(index_alias).collect(),
        evidence: vec![index_evidence(input)],
    }
}

fn index_alias(value: &String) -> IndexAlias {
    IndexAlias {
        kind: "synthetic".to_string(),
        value: value.clone(),
    }
}

fn index_evidence(input: &BenchMemoryInput) -> IndexEvidence {
    IndexEvidence {
        source_kind: "memory_benchmark".to_string(),
        source_id: input.id.clone(),
        conversation_id: None,
        turn_nonce: 0,
        excerpt_digest: input.content_digest.clone(),
        observed_at: BENCH_DENSE_TEMPORAL_OBSERVED_AT.to_string(),
    }
}

fn index_relations(items: &[BenchMemoryInput]) -> Vec<IndexRelation> {
    let mut relations = Vec::new();
    for item in items {
        for target in &item.relation_ids {
            relations.push(index_relation(&item.id, "related", target));
        }
        for target in &item.contradicts_ids {
            relations.push(index_relation(&item.id, "contradicts", target));
        }
    }
    relations
}

fn index_relation(source: &str, relation: &str, target: &str) -> IndexRelation {
    IndexRelation {
        source_id: source.to_string(),
        relation: relation.to_string(),
        target_id: target.to_string(),
        confidence: 100,
        created_at: "2026-08-17T00:00:00Z".to_string(),
    }
}

fn recall_context(
    query: &BenchQuery,
    checkpoint: super::UserMemoryIndexStatus,
    started_at: Instant,
) -> ReadyRecall {
    let scope = if query.scope_type == "workspace" {
        super::UserMemoryRecallScope::from_workspace_key(query.scope_key.clone())
    } else {
        super::UserMemoryRecallScope::global()
    };
    ReadyRecall {
        attempt: RecallAttempt {
            query: query.text.clone(),
            limit: query.limit,
            query_at: query.query_at.clone(),
            started_at,
            scope,
        },
        source_digest: checkpoint.source_digest.clone().unwrap_or_default(),
        checkpoint,
    }
}
