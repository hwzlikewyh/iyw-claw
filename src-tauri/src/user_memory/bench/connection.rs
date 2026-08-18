use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, ExecResult, QueryResult, Statement,
};

use super::{BenchMemoryInput, BenchQueryPlan, BenchStorageMetrics};

pub(super) struct CountingConnection<'a> {
    inner: &'a DatabaseConnection,
    statements: Mutex<Vec<Statement>>,
}

pub(super) struct SqlCounts {
    pub total: usize,
    pub candidate: usize,
    pub hydrate: usize,
}

impl<'a> CountingConnection<'a> {
    pub fn new(inner: &'a DatabaseConnection) -> Self {
        Self {
            inner,
            statements: Mutex::new(Vec::new()),
        }
    }

    pub fn take_statements(&self) -> Vec<Statement> {
        std::mem::take(
            &mut *self
                .statements
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }

    fn record(&self, statement: &Statement) {
        self.statements
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(statement.clone());
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for CountingConnection<'_> {
    fn get_database_backend(&self) -> DbBackend {
        self.inner.get_database_backend()
    }

    async fn execute(&self, statement: Statement) -> Result<ExecResult, DbErr> {
        self.record(&statement);
        self.inner.execute(statement).await
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        self.record(&Statement::from_string(DbBackend::Sqlite, sql));
        self.inner.execute_unprepared(sql).await
    }

    async fn query_one(&self, statement: Statement) -> Result<Option<QueryResult>, DbErr> {
        self.record(&statement);
        self.inner.query_one(statement).await
    }

    async fn query_all(&self, statement: Statement) -> Result<Vec<QueryResult>, DbErr> {
        self.record(&statement);
        self.inner.query_all(statement).await
    }
}

pub(super) fn sql_counts(statements: &[Statement]) -> SqlCounts {
    let hydrate = statements
        .iter()
        .filter(|statement| sql_lane(&statement.sql) == Some("hydrate"))
        .count();
    SqlCounts {
        total: statements.len(),
        candidate: statements.len().saturating_sub(hydrate),
        hydrate,
    }
}

pub(super) fn capture_plan_samples(
    samples: &Mutex<BTreeMap<String, Statement>>,
    statements: &[Statement],
) {
    let mut samples = samples
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for statement in statements {
        if let Some(lane) = sql_lane(&statement.sql) {
            samples
                .entry(lane.to_string())
                .or_insert_with(|| statement.clone());
        }
    }
}

pub(super) async fn explain_plans(
    conn: &DatabaseConnection,
    samples: &Mutex<BTreeMap<String, Statement>>,
) -> Vec<BenchQueryPlan> {
    let samples = samples
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let mut plans = Vec::with_capacity(samples.len());
    for (lane, statement) in samples {
        plans.push(explain_statement(conn, lane, statement).await);
    }
    plans
}

async fn explain_statement(
    conn: &DatabaseConnection,
    lane: String,
    statement: Statement,
) -> BenchQueryPlan {
    let explain = Statement {
        sql: format!("EXPLAIN QUERY PLAN {}", statement.sql),
        values: statement.values.clone(),
        db_backend: statement.db_backend,
    };
    match conn.query_all(explain).await {
        Ok(rows) => plan_from_rows(lane, statement.sql, rows),
        Err(error) => BenchQueryPlan {
            required_index: required_index(&lane).map(str::to_string),
            required_index_hit: None,
            lane,
            sql: statement.sql,
            details: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

fn plan_from_rows(lane: String, sql: String, rows: Vec<QueryResult>) -> BenchQueryPlan {
    let details = rows
        .into_iter()
        .filter_map(|row| row.try_get("", "detail").ok())
        .collect::<Vec<String>>();
    let required = required_index(&lane);
    let hit = required.map(|index| details.iter().any(|detail| detail.contains(index)));
    BenchQueryPlan {
        lane,
        sql,
        details,
        error: None,
        required_index: required.map(str::to_string),
        required_index_hit: hit,
    }
}

pub(super) async fn fts_storage_bytes(conn: &DatabaseConnection) -> Result<u64, String> {
    let row = conn
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COALESCE(SUM(pgsize), 0) AS bytes FROM dbstat WHERE name LIKE 'memory_item_fts_%'",
        ))
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "dbstat returned no row".to_string())?;
    row.try_get::<i64>("", "bytes")
        .map(|bytes| bytes.max(0) as u64)
        .map_err(|error| error.to_string())
}

pub(super) async fn storage_metrics(
    conn: &DatabaseConnection,
    db_path: &Path,
) -> BenchStorageMetrics {
    let (fts_bytes, fts_size_reason) = match fts_storage_bytes(conn).await {
        Ok(bytes) => (Some(bytes), None),
        Err(reason) => (None, Some(reason)),
    };
    BenchStorageMetrics {
        db_bytes: file_size(db_path),
        wal_bytes: file_size(&sidecar_path(db_path, "-wal")),
        shm_bytes: file_size(&sidecar_path(db_path, "-shm")),
        fts_bytes,
        fts_size_reason,
    }
}

pub(super) async fn apply_superseded(
    conn: &DatabaseConnection,
    items: &[BenchMemoryInput],
) -> Result<(), String> {
    for item in items {
        let Some(target) = &item.superseded_by else {
            continue;
        };
        conn.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE memory_item_current SET superseded_by = ? WHERE id = ?",
            [target.clone().into(), item.id.clone().into()],
        ))
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|value| value.len())
        .unwrap_or(0)
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}

fn required_index(lane: &str) -> Option<&'static str> {
    (lane == "temporal").then_some("idx_memory_evidence_time")
}

fn sql_lane(sql: &str) -> Option<&'static str> {
    if sql.contains("WHERE id IN (") {
        Some("hydrate")
    } else if sql.contains("FROM memory_alias_current") {
        Some("alias")
    } else if sql.contains("FROM memory_item_fts_unicode") {
        Some("fts_unicode")
    } else if sql.contains("FROM memory_item_fts_trigram") {
        Some("fts_trigram")
    } else if sql.contains("FROM memory_evidence") {
        Some("temporal")
    } else if sql.contains("FROM memory_relation_current") {
        Some("relation")
    } else if sql.contains("FROM memory_item_current WHERE id = ?") {
        Some("stable")
    } else {
        None
    }
}
