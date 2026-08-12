use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};

pub use super::agent_input_force_service::{
    active_force_batch, fail_force_batch, list_force_batch, mark_force_batch_dispatching,
    transition_force_batch,
};
pub use super::agent_input_outbox_mutation_service::{
    consume_native_started_turn, delete_waiting, mark_dispatching, retry_failed, transition_status,
};

use crate::acp::{AgentInputItem, AgentInputPayload, AgentInputStatus, AgentInputStrategy};
use crate::db::entities::agent_input_outbox;
use crate::db::error::DbError;
use crate::models::AgentType;

struct SerializedInput {
    id: String,
    conversation_id: i32,
    agent_type: String,
    payload_json: String,
}

impl SerializedInput {
    fn active_model(&self, sort_index: i64) -> agent_input_outbox::ActiveModel {
        agent_input_outbox::ActiveModel {
            id: Set(self.id.clone()),
            conversation_id: Set(self.conversation_id),
            agent_type: Set(self.agent_type.clone()),
            payload_json: Set(self.payload_json.clone()),
            status: Set(AgentInputStatus::Waiting.as_str().into()),
            dispatch_attempt: Set(0),
            sort_index: Set(sort_index),
            created_at: Set(Utc::now()),
            ..Default::default()
        }
    }
}

fn serialize_agent_type(agent_type: AgentType) -> Result<String, DbError> {
    serde_json::to_value(agent_type)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| DbError::Validation("invalid agent type".into()))
}

fn deserialize_agent_type(value: &str) -> Result<AgentType, DbError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|error| DbError::Validation(format!("invalid stored agent type: {error}")))
}

pub(super) fn parse_model(model: agent_input_outbox::Model) -> Result<AgentInputItem, DbError> {
    let payload = serde_json::from_str(&model.payload_json)
        .map_err(|error| DbError::Validation(format!("invalid stored input payload: {error}")))?;
    let status = AgentInputStatus::parse(&model.status)
        .ok_or_else(|| DbError::Validation("invalid stored input status".into()))?;
    let strategy = match model.strategy.as_deref() {
        Some(value) => Some(
            AgentInputStrategy::parse(value)
                .ok_or_else(|| DbError::Validation("invalid stored input strategy".into()))?,
        ),
        None => None,
    };
    Ok(AgentInputItem {
        id: model.id,
        conversation_id: model.conversation_id,
        connection_id: model.connection_id,
        target_turn_generation: model.target_turn_generation,
        agent_type: deserialize_agent_type(&model.agent_type)?,
        payload,
        strategy,
        status,
        dispatch_attempt: model.dispatch_attempt,
        last_error: model.last_error,
        sort_index: model.sort_index,
        force_batch_id: model.force_batch_id,
        force_requested_at: model.force_requested_at,
        created_at: model.created_at,
        dispatched_at: model.dispatched_at,
        consumed_at: model.consumed_at,
    })
}

pub async fn create(
    conn: &DatabaseConnection,
    id: String,
    conversation_id: i32,
    agent_type: AgentType,
    payload: AgentInputPayload,
) -> Result<AgentInputItem, DbError> {
    if payload.blocks.is_empty() {
        return Err(DbError::Validation(
            "agent input blocks cannot be empty".into(),
        ));
    }
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| DbError::Validation(format!("invalid input payload: {error}")))?;
    let serialized = SerializedInput {
        id,
        conversation_id,
        agent_type: serialize_agent_type(agent_type)?,
        payload_json,
    };
    if let Some(existing) = agent_input_outbox::Entity::find_by_id(&serialized.id)
        .one(conn)
        .await?
    {
        return parse_existing_or_reject(existing, &serialized);
    }
    let _guard = super::agent_input_ordering_service::lock().await;
    let txn = conn.begin().await?;
    if let Some(existing) = agent_input_outbox::Entity::find_by_id(&serialized.id)
        .one(&txn)
        .await?
    {
        txn.commit().await?;
        return parse_existing_or_reject(existing, &serialized);
    }
    let sort_index =
        super::agent_input_ordering_service::next_sort_index(&txn, serialized.conversation_id)
            .await?;
    match serialized.active_model(sort_index).insert(&txn).await {
        Ok(model) => {
            txn.commit().await?;
            parse_model(model)
        }
        Err(error) => {
            txn.rollback().await?;
            resolve_insert_conflict(conn, &serialized, error).await
        }
    }
}

fn parse_existing_or_reject(
    existing: agent_input_outbox::Model,
    serialized: &SerializedInput,
) -> Result<AgentInputItem, DbError> {
    if existing.conversation_id != serialized.conversation_id
        || existing.agent_type != serialized.agent_type
        || existing.payload_json != serialized.payload_json
    {
        return Err(DbError::Validation(
            "agent input message id is already bound to different content".into(),
        ));
    }
    parse_model(existing)
}

async fn resolve_insert_conflict(
    conn: &DatabaseConnection,
    serialized: &SerializedInput,
    error: sea_orm::DbErr,
) -> Result<AgentInputItem, DbError> {
    // Two windows can submit the same client id concurrently. Validate the
    // winning row before treating the competing insert as idempotent success.
    match agent_input_outbox::Entity::find_by_id(&serialized.id)
        .one(conn)
        .await
    {
        Ok(Some(existing)) => parse_existing_or_reject(existing, serialized),
        _ => Err(DbError::Database(error)),
    }
}

pub async fn get(conn: &DatabaseConnection, id: &str) -> Result<Option<AgentInputItem>, DbError> {
    agent_input_outbox::Entity::find_by_id(id)
        .one(conn)
        .await?
        .map(parse_model)
        .transpose()
}

pub async fn list_visible(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Vec<AgentInputItem>, DbError> {
    let rows = agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ConversationId.eq(conversation_id))
        .filter(agent_input_outbox::Column::DeletedAt.is_null())
        .filter(
            Condition::any()
                .add(agent_input_outbox::Column::Status.ne(AgentInputStatus::Consumed.as_str()))
                .add(
                    agent_input_outbox::Column::Strategy
                        .eq(AgentInputStrategy::CooperativeFeedback.as_str()),
                ),
        )
        .order_by_asc(agent_input_outbox::Column::SortIndex)
        .order_by_asc(agent_input_outbox::Column::CreatedAt)
        .order_by_asc(agent_input_outbox::Column::Id)
        .all(conn)
        .await?;
    rows.into_iter().map(parse_model).collect()
}

pub async fn next_dispatchable(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Option<AgentInputItem>, DbError> {
    let statuses = [
        AgentInputStatus::Waiting.as_str(),
        AgentInputStatus::FallbackQueued.as_str(),
    ];
    agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ConversationId.eq(conversation_id))
        .filter(agent_input_outbox::Column::Status.is_in(statuses))
        .filter(agent_input_outbox::Column::DeletedAt.is_null())
        .order_by_asc(agent_input_outbox::Column::SortIndex)
        .order_by_asc(agent_input_outbox::Column::CreatedAt)
        .order_by_asc(agent_input_outbox::Column::Id)
        .one(conn)
        .await?
        .map(parse_model)
        .transpose()
}

pub async fn next_unsettled(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Option<AgentInputItem>, DbError> {
    let statuses = [
        AgentInputStatus::Waiting.as_str(),
        AgentInputStatus::Dispatching.as_str(),
        AgentInputStatus::FallbackQueued.as_str(),
    ];
    agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ConversationId.eq(conversation_id))
        .filter(agent_input_outbox::Column::Status.is_in(statuses))
        .filter(agent_input_outbox::Column::DeletedAt.is_null())
        .order_by_asc(agent_input_outbox::Column::SortIndex)
        .order_by_asc(agent_input_outbox::Column::CreatedAt)
        .order_by_asc(agent_input_outbox::Column::Id)
        .one(conn)
        .await?
        .map(parse_model)
        .transpose()
}

pub async fn list_recoverable(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Vec<AgentInputItem>, DbError> {
    let statuses = [
        AgentInputStatus::Waiting.as_str(),
        AgentInputStatus::Dispatching.as_str(),
        AgentInputStatus::FallbackQueued.as_str(),
        AgentInputStatus::Failed.as_str(),
    ];
    let rows = agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ConversationId.eq(conversation_id))
        .filter(agent_input_outbox::Column::Status.is_in(statuses))
        .filter(agent_input_outbox::Column::DeletedAt.is_null())
        .order_by_asc(agent_input_outbox::Column::SortIndex)
        .order_by_asc(agent_input_outbox::Column::CreatedAt)
        .order_by_asc(agent_input_outbox::Column::Id)
        .all(conn)
        .await?;
    rows.into_iter().map(parse_model).collect()
}

pub async fn list_dispatching_for_turn(
    conn: &DatabaseConnection,
    connection_id: &str,
    turn_generation: i64,
) -> Result<Vec<AgentInputItem>, DbError> {
    let rows = agent_input_outbox::Entity::find()
        .filter(agent_input_outbox::Column::ConnectionId.eq(connection_id))
        .filter(agent_input_outbox::Column::TargetTurnGeneration.eq(turn_generation))
        .filter(agent_input_outbox::Column::Status.eq(AgentInputStatus::Dispatching.as_str()))
        .order_by_asc(agent_input_outbox::Column::SortIndex)
        .order_by_asc(agent_input_outbox::Column::CreatedAt)
        .order_by_asc(agent_input_outbox::Column::Id)
        .all(conn)
        .await?;
    rows.into_iter().map(parse_model).collect()
}
