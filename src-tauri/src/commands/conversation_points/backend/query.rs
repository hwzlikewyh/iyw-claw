use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

mod eviction;
mod remote;
use remote::{fetch, QueryResult};

const QUERY_BATCH: usize = 100;
const QUERY_BUDGET: Duration = Duration::from_secs(5);
const CACHE_LIMIT: usize = 10_000;
const PENDING_QUERY_INTERVAL: Duration = Duration::from_secs(1);
const FAILED_QUERY_INTERVAL: Duration = Duration::from_secs(3);
const SETTLEMENT_QUERY_INTERVAL: Duration = Duration::from_secs(5);
/// 结算已落定的请求只做低频复核，避免每次刷新都重发整个会话的 request_id。
const SETTLED_QUERY_INTERVAL: Duration = Duration::from_secs(60);
/// 与前端 needsSettlement 的终态集合保持一致。
const SETTLED_STATUSES: [&str; 7] = [
    "charged",
    "settled_without_debit",
    "revoked",
    "disabled",
    "identity_missing",
    "unsupported_currency",
    "not_billable",
];

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Record {
    #[serde(default)]
    pub(super) model_display_name: String,
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

    fn is_settled(&self) -> bool {
        self.has_final_cost_rows()
            && self.items.iter().all(|row| {
                row.charged_points.is_some()
                    && SETTLED_STATUSES.contains(&row.settlement_status.as_str())
            })
    }
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
    // 消费金额可持久复用；扣点及撤销状态按 retry_after 有界刷新，结算落定后降为低频复核。
    if query.ids.is_empty() {
        return records;
    }
    records.extend(query_missing(conn, query, scope).await);
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
    let now = Instant::now();
    let mut missing: Vec<String> = query
        .ids
        .iter()
        .filter(|id| {
            cached
                .retry_after
                .get(*id)
                .is_none_or(|after| *after <= now)
        })
        .cloned()
        .collect();
    missing.sort_by_key(|id| cached.records.get(id).is_some_and(RequestRows::is_settled));
    if missing.is_empty() {
        return;
    }
    tracing::debug!(
        requested = query.ids.len(),
        deferred = query.ids.len() - missing.len(),
        queried = missing.len(),
        "[conversation-points] backend query plan"
    );
    let deadline = Instant::now() + QUERY_BUDGET;
    for batch in missing.chunks(QUERY_BATCH) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let fetched = fetch(
            Query {
                url: query.url,
                scope: query.scope,
                token: query.token,
                ids: batch,
            },
            remaining,
        )
        .await;
        if !apply(cached, batch, fetched) {
            break;
        }
    }
}

/// 写入一批查询结果并安排下次查询时间；返回 false 表示本轮应停止后续批次。
fn apply(cached: &mut Cache, batch: &[String], fetched: Result<QueryResult, String>) -> bool {
    match fetched {
        Ok(result) => {
            let delay = if result.pending_request_ids.is_empty() {
                SETTLEMENT_QUERY_INTERVAL
            } else {
                PENDING_QUERY_INTERVAL
            };
            store(cached, batch, result);
            defer_queries(cached, batch, delay);
            true
        }
        Err(error) => {
            for id in batch {
                let Some(rows) = cached.records.get_mut(id) else {
                    continue;
                };
                for row in &mut rows.items {
                    row.charged_points = None;
                    row.settlement_status.clear();
                }
            }
            defer_queries(cached, batch, FAILED_QUERY_INTERVAL);
            tracing::warn!(error = %error, request_count = batch.len(), "[conversation-points] backend query failed");
            false
        }
    }
}

fn defer_queries(cached: &mut Cache, ids: &[String], delay: Duration) {
    let now = Instant::now();
    cached.retry_after.retain(|_, after| *after > now);
    for id in ids {
        let settled = cached.records.get(id).is_some_and(RequestRows::is_settled);
        let delay = if settled {
            SETTLED_QUERY_INTERVAL
        } else {
            delay
        };
        cached.retry_after.insert(id.clone(), now + delay);
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

fn store(cached: &mut Cache, ids: &[String], result: QueryResult) {
    eviction::reserve(cached, ids);
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
