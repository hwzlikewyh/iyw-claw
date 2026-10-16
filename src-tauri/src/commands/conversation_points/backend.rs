use crate::models::DbConversationDetail;
use std::collections::HashSet;
mod confirmed_cache;
mod query;
mod sources;
mod summary;
use query::query;
use summary::{summarize, Source};

pub(super) async fn enrich(conn: &sea_orm::DatabaseConnection, detail: &mut DbConversationDetail) {
    if !matches!(
        detail.summary.agent_type,
        crate::models::agent::AgentType::Codex | crate::models::agent::AgentType::ClaudeCode
    ) {
        return;
    }
    detail.backend_consumption = None;
    let source = sources::collect(conn, detail).await;
    let requests = &source.requests;
    if requests.is_empty() {
        return;
    }
    let ids: Vec<String> = requests
        .iter()
        .map(|receipt| receipt.request_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let (records, account_scope) = query(conn, &ids).await;
    let mut result = summarize(
        detail,
        Source {
            requests,
            messages: &source.messages,
            records: &records,
        },
    );
    apply_scopes(&mut result, &source, (&records, account_scope));
    detail.backend_consumption = Some(result);
}

fn apply_scopes(
    result: &mut crate::models::BackendConsumption,
    source: &sources::LedgerSource,
    ledger: (
        &std::collections::HashMap<String, query::RequestRows>,
        Option<String>,
    ),
) {
    let (records, account_scope) = ledger;
    let requests = &source.requests;
    result.session.account_scope = account_scope.clone();
    for turn in result.turns.values_mut() {
        turn.account_scope = account_scope.clone();
    }
    let mut own = summary::select_consumption(requests, &source.own, records);
    own.account_scope = account_scope.clone();
    if source.incomplete && own.state == "confirmed" {
        own.state = "partial".into();
    }
    result.own = Some(own);
    for (scope, ids) in &source.scopes {
        let mut value = summary::select_consumption(requests, ids, records);
        value.account_scope = account_scope.clone();
        if (source.incomplete || scope.starts_with("goal:") && source.unscoped_descendants)
            && value.state == "confirmed"
        {
            value.state = "partial".into();
        }
        result.scopes.insert(scope.clone(), value);
    }
    if source.incomplete && result.session.state == "confirmed" {
        result.session.state = "partial".into();
    }
}
