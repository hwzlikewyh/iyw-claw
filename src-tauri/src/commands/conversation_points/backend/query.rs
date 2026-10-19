use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const QUERY_BATCH: usize = 100;
const QUERY_BUDGET: Duration = Duration::from_secs(5);
const CACHE_LIMIT: usize = 10_000;
const PENDING_QUERY_INTERVAL: Duration = Duration::from_secs(1);
const FAILED_QUERY_INTERVAL: Duration = Duration::from_secs(3);
const SETTLEMENT_QUERY_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Record {
    pub(super) request_id: String,
    pub(super) stage_key: String,
    pub(super) actual_points: Option<String>,
    #[serde(default)]
    pub(super) charged_points: Option<String>,
    #[serde(default)]
    pub(super) settlement_status: String,
    pub(super) cost_status: String,
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) cache_read_tokens: u64,
    pub(super) cache_write_tokens: u64,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub(super) struct RequestRows {
    pub(super) items: Vec<Record>,
    pub(super) pending: bool,
}

impl RequestRows {
    pub(super) fn has_final_cost_rows(&self) -> bool {
        !self.pending && !self.items.is_empty()
    }
}

#[derive(Deserialize)]
struct QueryResult {
    items: Vec<Record>,
    pending_request_ids: Vec<String>,
}

#[derive(Default)]
struct Cache {
    scope: String,
    records: HashMap<String, RequestRows>,
    retry_after: HashMap<String, Instant>,
    persisted: HashSet<String>,
}

struct Query<'a> {
    scope: &'a str,
    url: &'a str,
    token: &'a str,
    ids: &'a [String],
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

pub(super) async fn query(
    conn: &sea_orm::DatabaseConnection,
    ids: &[String],
) -> (HashMap<String, RequestRows>, Option<String>) {
    let Some((token, identity_scope)) =
        super::super::super::iyw_account::iyw_account_usage_credentials_core(conn)
            .await
            .ok()
            .flatten()
    else {
        return (HashMap::new(), None);
    };
    let base = crate::acp::provider_overlay::cloud_model_gateway_base_url();
    let base = base.trim_end_matches('/');
    let base = base.strip_suffix("/v1").unwrap_or(base);
    let base = base.strip_suffix("/anthropic").unwrap_or(base);
    let url = format!("{}/v1/usage/requests/query", base);
    let scope = super::confirmed_cache::scope(&url, token.expose(), &identity_scope);
    let account_scope = super::confirmed_cache::scope(&url, "", &identity_scope);
    let records = query_with_credentials(
        conn,
        Query {
            scope: &scope,
            url: &url,
            token: token.expose(),
            ids,
        },
    )
    .await;
    (records, Some(account_scope))
}

async fn query_with_credentials(
    conn: &sea_orm::DatabaseConnection,
    query: Query<'_>,
) -> HashMap<String, RequestRows> {
    let scope = query.scope.to_string();
    // 先恢复不可变消费字段；可变结算字段由有界远端读取补齐。
    let mut records = match super::confirmed_cache::load(conn, &scope, query.ids).await {
        Ok(records) => records,
        Err(error) => {
            tracing::warn!(%error, "[conversation-points] local consumption cache read failed");
            HashMap::new()
        }
    };
    // 消费金额可持久复用；扣点及撤销状态仍需有界刷新，不能永久冻结。
    let missing = query.ids.to_vec();
    if missing.is_empty() {
        return records;
    }
    records.extend(
        query_missing(
            conn,
            Query {
                ids: &missing,
                ..query
            },
            scope,
        )
        .await,
    );
    records
}

async fn query_missing(
    conn: &sea_orm::DatabaseConnection,
    query: Query<'_>,
    scope: String,
) -> HashMap<String, RequestRows> {
    // 并发读取共享一次远端查询；后到的读取复用结果，不因锁忙误报待确认。
    let mut cached = cache().lock().await;
    if cached.scope != scope {
        *cached = Cache {
            scope,
            ..Default::default()
        };
    }
    let ids = query.ids;
    refresh(&mut cached, query).await;
    persist(conn, &mut cached, ids).await;
    ids.iter()
        .filter_map(|id| {
            cached
                .records
                .get(id)
                .map(|rows| (id.clone(), rows.clone()))
        })
        .collect()
}

async fn refresh(cached: &mut Cache, query: Query<'_>) {
    let missing: Vec<String> = query
        .ids
        .iter()
        .filter(|id| {
            cached
                .retry_after
                .get(*id)
                .is_none_or(|after| *after <= Instant::now())
        })
        .cloned()
        .collect();
    let deadline = Instant::now() + QUERY_BUDGET;
    for batch in missing.chunks(QUERY_BATCH) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match fetch(
            Query {
                url: query.url,
                scope: query.scope,
                token: query.token,
                ids: batch,
            },
            remaining,
        )
        .await
        {
            Ok(result) => {
                let delay = if result.pending_request_ids.is_empty() {
                    SETTLEMENT_QUERY_INTERVAL
                } else {
                    PENDING_QUERY_INTERVAL
                };
                store(cached, batch, result);
                defer_queries(cached, batch, delay);
            }
            Err(error) => {
                for id in batch {
                    if let Some(rows) = cached.records.get_mut(id) {
                        for row in &mut rows.items {
                            row.charged_points = None;
                            row.settlement_status.clear();
                        }
                    }
                }
                defer_queries(cached, batch, FAILED_QUERY_INTERVAL);
                tracing::warn!(error = %error, request_count = batch.len(), "[conversation-points] backend query failed");
                break;
            }
        }
    }
}

fn defer_queries(cached: &mut Cache, ids: &[String], delay: Duration) {
    cached
        .retry_after
        .retain(|_, after| *after > Instant::now());
    let next = Instant::now() + delay;
    for id in ids {
        cached.retry_after.insert(id.clone(), next);
    }
}

async fn persist(conn: &sea_orm::DatabaseConnection, cached: &mut Cache, ids: &[String]) {
    let records: HashMap<_, _> = ids
        .iter()
        .filter(|id| !cached.persisted.contains(*id))
        .filter_map(|id| {
            cached
                .records
                .get(id)
                .filter(|rows| rows.has_final_cost_rows())
                .map(|rows| (id.clone(), rows.clone()))
        })
        .collect();
    if let Err(error) = super::confirmed_cache::save(conn, &cached.scope, &records).await {
        tracing::warn!(%error, request_count = records.len(), "[conversation-points] local consumption cache write failed");
    } else {
        cached.persisted.extend(records.into_keys());
    }
}

async fn fetch(query: Query<'_>, timeout: Duration) -> Result<QueryResult, String> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    let response = CLIENT
        .get_or_init(reqwest::Client::new)
        .post(query.url)
        .header("token", query.token)
        .json(&serde_json::json!({"request_ids": query.ids}))
        .timeout(timeout)
        .send()
        .await
        .map_err(|error| error.without_url().to_string())?;
    let payload: serde_json::Value = response
        .error_for_status()
        .map_err(|error| error.without_url().to_string())?
        .json()
        .await
        .map_err(|_| "invalid backend consumption response")?;
    if payload.get("code").and_then(serde_json::Value::as_i64) != Some(1) {
        return Err("backend consumption query rejected".into());
    }
    serde_json::from_value(payload["data"].clone())
        .map_err(|_| "invalid backend consumption records".into())
}

fn store(cached: &mut Cache, ids: &[String], result: QueryResult) {
    let added = ids
        .iter()
        .filter(|id| !cached.records.contains_key(*id))
        .count();
    if cached.records.len().saturating_add(added) > CACHE_LIMIT {
        cached.records.clear();
        cached.retry_after.clear();
        cached.persisted.clear();
    }
    let pending: HashSet<_> = result.pending_request_ids.into_iter().collect();
    for id in ids {
        let records: Vec<_> = result
            .items
            .iter()
            .filter(|record| record.request_id == *id)
            .cloned()
            .collect();
        cached.records.insert(
            id.clone(),
            RequestRows {
                pending: pending.contains(id) || records.is_empty(),
                items: records,
            },
        );
    }
}
