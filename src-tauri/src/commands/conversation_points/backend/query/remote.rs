use super::{Query, Record};
use serde::Deserialize;
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Deserialize)]
pub(super) struct QueryResult {
    pub(super) items: Vec<Record>,
    pub(super) pending_request_ids: Vec<String>,
}

pub(super) async fn fetch(query: Query<'_>, timeout: Duration) -> Result<QueryResult, String> {
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
