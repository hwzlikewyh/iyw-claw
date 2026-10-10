use crate::models::DbConversationDetail;
use crate::parsers::codex::billing::Receipt;
use std::collections::{HashMap, HashSet};
mod query;
mod summary;
use query::query;
use summary::{summarize, Source};

pub(super) async fn enrich(conn: &sea_orm::DatabaseConnection, detail: &mut DbConversationDetail) {
    if detail.summary.agent_type != crate::models::agent::AgentType::Codex {
        return;
    }
    let Some((requests, messages)) = receipts(conn, detail).await else {
        return;
    };
    if requests.is_empty() {
        return;
    }
    let ids: Vec<String> = requests
        .iter()
        .map(|receipt| receipt.request_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let records = query(conn, &ids).await;
    detail.backend_consumption = Some(summarize(
        detail,
        Source {
            requests: &requests,
            messages: &messages,
            records: &records,
        },
    ));
}

async fn receipts(
    conn: &sea_orm::DatabaseConnection,
    detail: &DbConversationDetail,
) -> Option<(Vec<Receipt>, HashMap<String, String>)> {
    let mut ids: HashSet<String> = detail.summary.external_id.iter().cloned().collect();
    let segments = crate::db::service::conversation_session_segment_service::list_for_conversation(
        conn,
        detail.summary.id,
    )
    .await
    .unwrap_or_default();
    ids.extend(
        segments
            .into_iter()
            .filter_map(|segment| segment.external_id),
    );
    tokio::task::spawn_blocking(move || {
        let parser = crate::parsers::codex::CodexParser::new();
        let mut requests = Vec::new();
        let mut messages = HashMap::new();
        for id in ids {
            if let Some(receipts) = parser.billing_receipts(&id) {
                requests.extend(receipts.requests);
                messages.extend(
                    receipts
                        .message_turns
                        .into_iter()
                        .map(|(message, turn)| (message, format!("{id}:{turn}"))),
                );
            }
        }
        (requests, messages)
    })
    .await
    .ok()
}
