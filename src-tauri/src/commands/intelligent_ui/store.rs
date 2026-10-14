use super::{settings_core, validation, STATE_EVENT};
use crate::app_error::AppCommandError;
use crate::commands::conversations::get_folder_conversation_core;
use crate::web::event_bridge::{emit_event, EventEmitter};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, FromQueryResult, Statement};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_BLOCKS_PER_CONVERSATION: usize = 256;

#[derive(Clone, Debug, Serialize, FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub conversation_id: i32,
    pub turn_id: String,
    pub block_id: String,
    pub source_hash: String,
    pub state_json: String,
    pub revision: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiStateInput {
    pub conversation_id: i32,
    pub block_id: String,
    pub source: String,
    pub state: Value,
    pub revision: i64,
}

pub async fn read_states_core(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Vec<UiState>, AppCommandError> {
    if conversation_id <= 0 {
        return Err(validation::invalid("Invalid conversation id"));
    }
    let query = Statement::from_sql_and_values(DbBackend::Sqlite,
        "SELECT conversation_id,turn_id,block_id,source_hash,state_json,revision FROM intelligent_ui_state WHERE conversation_id=? ORDER BY updated_at DESC LIMIT 256", [conversation_id.into()]);
    UiState::find_by_statement(query)
        .all(conn)
        .await
        .map_err(|error| AppCommandError::database_error(error.to_string()))
}

pub async fn save_state_core(
    conn: &DatabaseConnection,
    emitter: &EventEmitter,
    input: UiStateInput,
) -> Result<UiState, AppCommandError> {
    if !settings_core(conn).await?.enabled {
        return Err(validation::invalid("Intelligent UI is disabled"));
    }
    let state_json = state_json(&input)?;
    let (detail, _) = get_folder_conversation_core(conn, input.conversation_id).await?;
    let turn = validation::resolve_turn(&detail.turns, &input.block_id, &input.source)?;
    let hash = validation::source_hash(&input.source);
    let rows = read_states_core(conn, input.conversation_id).await?;
    let existing = rows
        .iter()
        .find(|row| row.block_id == input.block_id && row.source_hash == hash);
    if existing.map_or(0, |row| row.revision) != input.revision {
        return Err(validation::invalid(
            "UI state changed in another view; reload before saving",
        ));
    }
    if existing.is_none() && rows.len() >= MAX_BLOCKS_PER_CONVERSATION {
        return Err(validation::invalid("UI state limit reached"));
    }
    let revision = input.revision.saturating_add(1);
    let row = UiState {
        conversation_id: input.conversation_id,
        turn_id: turn.id.clone(),
        block_id: input.block_id,
        source_hash: hash,
        state_json,
        revision,
    };
    persist(conn, &row, input.revision).await?;
    tracing::debug!(
        conversation_id = row.conversation_id,
        revision,
        "[IntelligentUI] state saved"
    );
    emit_event(emitter, STATE_EVENT, row.clone());
    Ok(row)
}

async fn persist(
    conn: &DatabaseConnection,
    row: &UiState,
    previous: i64,
) -> Result<(), AppCommandError> {
    let values = vec![
        row.conversation_id.into(),
        row.turn_id.clone().into(),
        row.block_id.clone().into(),
        row.source_hash.clone().into(),
        row.state_json.clone().into(),
        row.revision.into(),
        chrono::Utc::now().to_rfc3339().into(),
        row.conversation_id.into(),
        previous.into(),
    ];
    let result = conn.execute_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
        "INSERT INTO intelligent_ui_state (conversation_id,turn_id,block_id,source_hash,state_json,revision,updated_at) SELECT ?,?,?,?,?,?,? WHERE EXISTS (SELECT 1 FROM conversation WHERE id=? AND deleted_at IS NULL) AND EXISTS (SELECT 1 FROM app_metadata WHERE key='intelligent_ui.enabled' AND value='true' AND deleted_at IS NULL) ON CONFLICT(conversation_id,block_id,source_hash) DO UPDATE SET turn_id=excluded.turn_id,state_json=excluded.state_json,revision=excluded.revision,updated_at=excluded.updated_at WHERE intelligent_ui_state.revision=?", values)).await
        .map_err(|error| AppCommandError::database_error(error.to_string()))?;
    if result.rows_affected() != 1 {
        return Err(validation::invalid("UI state revision conflict"));
    }
    Ok(())
}

fn state_json(input: &UiStateInput) -> Result<String, AppCommandError> {
    if input.revision < 0 || input.revision == i64::MAX {
        return Err(validation::invalid("Invalid UI revision"));
    }
    validation::normalize_state(&input.state)
}
