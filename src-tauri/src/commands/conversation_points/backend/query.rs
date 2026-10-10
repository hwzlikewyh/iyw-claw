use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const QUERY_BATCH: usize = 100;
const QUERY_BUDGET: Duration = Duration::from_secs(5);
const CACHE_LIMIT: usize = 10_000;
const CACHE_LOOKUP_BUDGET: Duration = Duration::from_millis(50);

#[derive(Clone, Deserialize)]
pub(super) struct Record {
    pub(super) request_id: String,
    pub(super) stage_key: String,
    pub(super) actual_points: Option<String>,
    pub(super) cost_status: String,
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) cache_read_tokens: u64,
    pub(super) cache_write_tokens: u64,
}

#[derive(Clone, Default)]
pub(super) struct RequestRows {
    pub(super) items: Vec<Record>,
    pub(super) pending: bool,
}

#[derive(Deserialize)]
struct QueryResult {
    items: Vec<Record>,
    pending_request_ids: Vec<String>,
}

#[derive(Default)]
struct Cache {
    scope: Vec<u8>,
    records: HashMap<String, RequestRows>,
}

struct Query<'a> {
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
) -> HashMap<String, RequestRows> {
    let Some(token) = super::super::super::iyw_account::iyw_account_access_token_core(conn)
        .await
        .ok()
        .flatten()
    else {
        return HashMap::new();
    };
    let base = crate::acp::provider_overlay::model_gateway_base_url_for(
        crate::models::agent::AgentType::Codex,
    );
    let base = base.trim_end_matches('/');
    let url = format!(
        "{}/v1/usage/requests/query",
        base.strip_suffix("/v1").unwrap_or(base)
    );
    query_with_credentials(Query {
        url: &url,
        token: token.expose(),
        ids,
    })
    .await
}

async fn query_with_credentials(query: Query<'_>) -> HashMap<String, RequestRows> {
    let scope = Sha256::digest(format!("{}:{}", query.url, query.token).as_bytes()).to_vec();
    let Ok(mut cached) = tokio::time::timeout(CACHE_LOOKUP_BUDGET, cache().lock()).await else {
        return HashMap::new();
    };
    if cached.scope != scope {
        *cached = Cache {
            scope,
            ..Default::default()
        };
    }
    let ids = query.ids;
    refresh(&mut cached, query).await;
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
        .filter(|id| cached.records.get(*id).is_none_or(|rows| rows.pending))
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
                token: query.token,
                ids: batch,
            },
            remaining,
        )
        .await
        {
            Ok(result) => store(cached, batch, result),
            Err(error) => {
                tracing::warn!(error = %error, request_count = batch.len(), "[conversation-points] backend query failed");
                break;
            }
        }
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
    if cached.records.len().saturating_add(ids.len()) > CACHE_LIMIT {
        cached.records.clear();
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
                items: records,
                pending: pending.contains(id),
            },
        );
    }
}
