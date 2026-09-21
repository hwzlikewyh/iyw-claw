use std::collections::BTreeMap;

use sea_orm::{QueryResult, TryGetable};

use super::{harvest_reference, MemoryHarvestRequest};
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

const MAX_DISCOVERY_TURNS: i64 = 256;

pub(super) struct HarvestDiscovery {
    pub requests: Vec<MemoryHarvestRequest>,
    pub skipped_sensitive: u32,
    pub skipped_context_poor: u32,
}

pub(super) async fn discover(
    service: &super::UserMemoryService,
) -> Result<HarvestDiscovery, AppCommandError> {
    let rows = super::harvest_store_sql::query_all(
        &service.db,
        DISCOVERY_SQL,
        [MAX_DISCOVERY_TURNS.into()],
    )
    .await?;
    group_rows(rows)
}

fn group_rows(rows: Vec<QueryResult>) -> Result<HarvestDiscovery, AppCommandError> {
    let mut groups = BTreeMap::<(i32, i64), Vec<QueryResult>>::new();
    for row in rows {
        let conversation = field::<i32>(&row, "conversation_id")?;
        let generation = field::<i64>(&row, "target_turn_generation")?;
        groups
            .entry((conversation, generation))
            .or_default()
            .push(row);
    }
    let mut discovery = HarvestDiscovery {
        requests: Vec::new(),
        skipped_sensitive: 0,
        skipped_context_poor: 0,
    };
    for (key, rows) in groups {
        match build_request(key, &rows)? {
            DiscoveryItem::Request(request) => discovery.requests.push(request),
            DiscoveryItem::Sensitive => discovery.skipped_sensitive += 1,
            DiscoveryItem::ContextPoor => discovery.skipped_context_poor += 1,
            DiscoveryItem::Unavailable => {}
        }
    }
    Ok(discovery)
}

fn build_request(key: (i32, i64), rows: &[QueryResult]) -> Result<DiscoveryItem, AppCommandError> {
    let first = &rows[0];
    let agent = parse_agent(field(first, "agent_type")?)?;
    let expected_agent = super::harvest_store_sql::agent_name(agent);
    if rows
        .iter()
        .any(|row| field::<String>(row, "agent_type").ok().as_deref() != Some(&expected_agent))
    {
        return Ok(DiscoveryItem::Unavailable);
    }
    let combined = rows
        .iter()
        .filter_map(|row| field::<String>(row, "payload_json").ok())
        .filter_map(|json| serde_json::from_str::<crate::acp::AgentInputPayload>(&json).ok())
        .map(|payload| payload.display_text)
        .collect::<Vec<_>>()
        .join("\n");
    if super::helpers::contains_potential_secret(&combined)
        || contains_credential_like_token(&combined)
    {
        return Ok(DiscoveryItem::Sensitive);
    }
    let Some(user_input_ref) = harvest_reference(&combined) else {
        return Ok(DiscoveryItem::Unavailable);
    };
    if user_input_ref.chars().count() < super::harvest::USER_MEMORY_HARVEST_MIN_CONTENT_CHARS {
        return Ok(DiscoveryItem::ContextPoor);
    }
    let workspace = field::<String>(first, "folder_path")?;
    Ok(DiscoveryItem::Request(MemoryHarvestRequest {
        conversation: key.0.to_string(),
        turn_nonce: key.1 as u64,
        agent_type: agent,
        workspace_key: Some(crate::commands::skill_inventory::workspace_key(Some(
            &workspace,
        ))),
        stop_reason: Some("recovered_user_input".into()),
        user_input_ref: Some(user_input_ref),
        assistant_input_ref: None,
        tool_outcome_ref: None,
        submitted_at: field(first, "created_at")?,
    }))
}

enum DiscoveryItem {
    Request(MemoryHarvestRequest),
    Sensitive,
    ContextPoor,
    Unavailable,
}

fn contains_credential_like_token(content: &str) -> bool {
    content.split_whitespace().any(|token| {
        let token = token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        let length = token.len();
        length >= 10
            && length <= 64
            && token.bytes().all(|byte| byte.is_ascii_alphanumeric())
            && token.bytes().any(|byte| byte.is_ascii_lowercase())
            && token.bytes().any(|byte| byte.is_ascii_uppercase())
            && token.bytes().filter(u8::is_ascii_digit).count() >= 4
    })
}

fn parse_agent(value: String) -> Result<AgentType, AppCommandError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| {
        AppCommandError::configuration_invalid("Recovered memory input has an invalid agent")
    })
}

fn field<T: TryGetable>(row: &QueryResult, name: &str) -> Result<T, AppCommandError> {
    row.try_get("", name)
        .map_err(super::index_checkpoint::database_error)
}

const DISCOVERY_SQL: &str = "WITH missing AS (SELECT a.conversation_id,a.target_turn_generation FROM agent_input_outbox a JOIN conversation c ON c.id=a.conversation_id JOIN folder f ON f.id=c.folder_id WHERE a.status='consumed' AND a.deleted_at IS NULL AND a.target_turn_generation>0 AND a.target_turn_generation<=c.last_completed_turn_generation AND c.parent_id IS NULL AND c.deleted_at IS NULL AND f.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM automation_run r WHERE r.conversation_id=c.id) AND NOT EXISTS(SELECT 1 FROM memory_harvest_outbox h WHERE h.conversation_id=CAST(a.conversation_id AS TEXT) AND h.turn_nonce=a.target_turn_generation) GROUP BY a.conversation_id,a.target_turn_generation ORDER BY a.conversation_id,a.target_turn_generation LIMIT ?) SELECT a.conversation_id,a.target_turn_generation,a.agent_type,a.payload_json,a.created_at,f.path AS folder_path FROM missing m JOIN agent_input_outbox a ON a.conversation_id=m.conversation_id AND a.target_turn_generation=m.target_turn_generation JOIN folder f ON f.id=(SELECT c.folder_id FROM conversation c WHERE c.id=m.conversation_id) WHERE a.status='consumed' AND a.deleted_at IS NULL ORDER BY a.conversation_id,a.target_turn_generation,a.sort_index,a.created_at,a.id";
