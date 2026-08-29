use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;

use super::harvest::{HarvestRecord, UserMemoryHarvestFailureKind, UserMemoryHarvestState};
use super::harvest_store_sql::{agent_name, execute};

pub(super) async fn import(
    conn: &DatabaseConnection,
    records: &[HarvestRecord],
) -> Result<usize, AppCommandError> {
    let mut imported = 0;
    for record in records {
        let request = &record.request;
        let result = execute(
            conn,
            "INSERT OR IGNORE INTO memory_harvest_outbox (dedup_key, conversation_id, turn_nonce, agent_type, workspace_key, stop_reason, user_input_ref, assistant_input_ref, tool_outcome_ref, submitted_at, state, attempts, failure_kind, failure_detail, noop_reason, candidate_ids, experience_ids, processed_at, processing_ms, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [request.dedup_key().into(), request.conversation.clone().into(), (request.turn_nonce as i64).into(), agent_name(request.agent_type).into(), request.workspace_key.clone().into(), request.stop_reason.clone().into(), safe_ref(request.user_input_ref.clone()).into(), safe_ref(request.assistant_input_ref.clone()).into(), safe_ref(request.tool_outcome_ref.clone()).into(), request.submitted_at.clone().into(), state_name(record.state).into(), i64::from(record.attempts).into(), record.failure_kind.map(failure_name).map(str::to_string).into(), record.failure_detail.clone().into(), record.noop_reason.clone().into(), json_ids(record.candidate_ids.clone())?.into(), json_ids(record.experience_ids.clone())?.into(), record.processed_at.clone().into(), record.processing_ms.map(|value| value as i64).into(), chrono::Utc::now().to_rfc3339().into()],
        ).await?;
        imported += usize::from(result.rows_affected() == 1);
    }
    Ok(imported)
}

fn state_name(state: UserMemoryHarvestState) -> &'static str {
    match state {
        UserMemoryHarvestState::Queued | UserMemoryHarvestState::Extracting => "queued",
        UserMemoryHarvestState::Proposed => "proposed",
        UserMemoryHarvestState::Noop => "noop",
        UserMemoryHarvestState::Failed => "failed",
        UserMemoryHarvestState::Dead => "dead",
    }
}

fn failure_name(kind: UserMemoryHarvestFailureKind) -> &'static str {
    match kind {
        UserMemoryHarvestFailureKind::Io => "io",
        UserMemoryHarvestFailureKind::InvalidInput => "invalid_input",
        UserMemoryHarvestFailureKind::SensitiveContent => "sensitive_content",
        UserMemoryHarvestFailureKind::Internal => "internal",
    }
}

fn json_ids(ids: Option<Vec<String>>) -> Result<Option<String>, AppCommandError> {
    match ids.filter(|value| !value.is_empty()) {
        Some(ids) => serde_json::to_string(&ids)
            .map(Some)
            .map_err(|error| AppCommandError::configuration_invalid(error.to_string())),
        None => Ok(None),
    }
}

fn safe_ref(value: Option<String>) -> Option<String> {
    value.filter(|text| !super::helpers::contains_potential_secret(text))
}
