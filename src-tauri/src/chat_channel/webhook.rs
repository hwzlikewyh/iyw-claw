//! Outbound webhook delivery for the global chat-channel event feed.
//!
//! Webhooks are a channel-agnostic event sink: when an ACP event passes the
//! global event filter (and the bridged-permission suppression), the event
//! subscriber POSTs a structured JSON payload to every configured URL — in
//! addition to the IM channel fan-out. Unlike IM channels, webhooks are NOT
//! debounced and do NOT participate in the per-channel filter; an automation
//! consumer wants the complete event stream.
//!
//! Delivery is fire-and-forget (`tokio::spawn` per URL) so a slow or
//! unreachable endpoint never stalls the event subscriber loop.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::types::RichMessage;

/// One configured webhook sink. Persisted (as a JSON array) under the
/// `chat_event_webhooks` app-metadata key and mirrored on the frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    pub enabled: bool,
}

/// Parse the stored webhook config JSON and return the URLs of ENABLED entries
/// only — the set the event subscriber actually delivers to. Unparseable input
/// yields an empty list (treated as "no webhooks").
pub fn enabled_webhook_urls(json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<WebhookConfig>>(json)
        .map(|list| {
            list.into_iter()
                .filter(|w| w.enabled)
                .map(|w| w.url)
                .collect()
        })
        .unwrap_or_default()
}

/// Build the JSON body POSTed to each webhook for one event.
///
/// Pure (no I/O, no clock) so the wire contract is unit-testable. `title`,
/// `body` and the `fields` labels are localized per the chat message-language
/// setting (same text IM channels receive); `event`, `level` and `source` are
/// stable machine-readable values.
pub fn build_webhook_payload(
    event_type: &str,
    connection_id: &str,
    msg: &RichMessage,
) -> serde_json::Value {
    let fields: Vec<serde_json::Value> = msg
        .fields
        .iter()
        .map(|(label, value)| serde_json::json!({ "label": label, "value": value }))
        .collect();

    serde_json::json!({
        "event": event_type,
        "level": msg.level,
        "title": msg.title,
        "body": msg.body,
        "fields": fields,
        "connection_id": connection_id,
        "source": "iyw-claw",
    })
}

/// Build the shared reqwest client used for webhook delivery. Mirrors the
/// timeout posture of the IM backends.
pub fn make_webhook_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

/// Fan the payload out to every URL on detached tasks. Returns immediately;
/// failures are logged, not surfaced (the event loop must not block on, or be
/// failed by, an unreachable consumer).
pub fn spawn_webhook_delivery(
    client: reqwest::Client,
    urls: Vec<String>,
    payload: serde_json::Value,
) {
    for url in urls {
        let client = client.clone();
        let payload = payload.clone();
        tokio::spawn(async move {
            if let Err(e) = post_one(&client, &url, &payload).await {
                // Redact: webhook URLs often carry secrets in the path/query.
                tracing::error!(
                    "[ChatChannel] webhook delivery to {} failed: {e}",
                    redact_url(&url)
                );
            }
        });
    }
}

/// Reduce a URL to `scheme://host[:port]` for logging, dropping the path,
/// query and any userinfo — webhook URLs frequently embed credentials there
/// (e.g. Slack/Discord tokens) which must not reach logs. Unparseable input
/// collapses to a non-revealing placeholder.
fn redact_url(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(u) => match (u.host_str(), u.port()) {
            (Some(host), Some(port)) => format!("{}://{host}:{port}", u.scheme()),
            (Some(host), None) => format!("{}://{host}", u.scheme()),
            (None, _) => "<webhook>".to_string(),
        },
        Err(_) => "<webhook>".to_string(),
    }
}

/// POST one payload to one URL, mapping transport errors and non-2xx
/// responses to a `String` for logging.
async fn post_one(
    client: &reqwest::Client,
    url: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let resp = client
        .post(url)
        .json(payload)
        .send()
        .await
        // `reqwest::Error`'s Display embeds the request URL ("... for url (...)"),
        // which would re-leak path/query secrets the explicit redaction strips.
        // `without_url()` removes it before stringifying.
        .map_err(|e| e.without_url().to_string())?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    Ok(())
}

