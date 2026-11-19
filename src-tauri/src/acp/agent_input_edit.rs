use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use super::agent_input_dispatch::emit_input;
use super::AgentInputChange;
use super::{error::AcpError, manager::ConnectionManager, AgentInputItem, AgentInputStatus};
use crate::db::entities::agent_input_outbox::{Column, Entity};
use crate::db::service::agent_input_outbox_service;
use crate::db::AppDatabase;

impl ConnectionManager {
    pub(crate) async fn revise_agent_input(
        &self,
        db: &AppDatabase,
        connection_id: &str,
        conversation_id: i32,
        message_id: &str,
        change: AgentInputChange,
    ) -> Result<AgentInputItem, AcpError> {
        let _operation = self.acquire_operation_read().await?;
        let lock = self.agent_input_runtime.dispatch_lock(connection_id).await;
        let _guard = lock.lock().await;
        let (state, emitter) = self.target(connection_id, conversation_id).await?;
        let current = agent_input_outbox_service::get(&db.conn, message_id)
            .await
            .map_err(db_error)?
            .filter(|item| item.conversation_id == conversation_id)
            .ok_or_else(|| AcpError::protocol("agent input not found in this conversation"))?;
        let update = Entity::update_many()
            .filter(Column::Id.eq(message_id))
            .filter(Column::ConversationId.eq(conversation_id))
            .filter(Column::ForceBatchId.is_null())
            .filter(Column::Status.is_in([
                AgentInputStatus::Waiting.as_str(),
                AgentInputStatus::FallbackQueued.as_str(),
            ]));
        let update = match change {
            AgentInputChange::SendNow => {
                if current.force_batch_id.is_some() || current.status == AgentInputStatus::Consumed
                {
                    return Ok(current);
                }
                let generation = state.read().await.turn_generation;
                update
                    .col_expr(
                        Column::Strategy,
                        Expr::value(super::AgentInputStrategy::SafeForceNext.as_str()),
                    )
                    .col_expr(
                        Column::ForceBatchId,
                        Expr::value(uuid::Uuid::new_v4().to_string()),
                    )
                    .col_expr(Column::ForceRequestedAt, Expr::value(Utc::now()))
                    .col_expr(Column::ConnectionId, Expr::value(connection_id.to_owned()))
                    .col_expr(Column::TargetTurnGeneration, Expr::value(generation))
            }
            AgentInputChange::Edit { mut payload } => {
                restore_image_paths(&mut payload, &current.payload);
                payload.delivery = current.payload.delivery;
                payload.mode_id = current.payload.mode_id;
                update.col_expr(
                    Column::PayloadJson,
                    Expr::value(serde_json::to_string(&payload).map_err(db_error)?),
                )
            }
        };
        if update.exec(&db.conn).await.map_err(db_error)?.rows_affected != 1 {
            return Err(AcpError::protocol("agent input is no longer waiting"));
        }
        let item = agent_input_outbox_service::get(&db.conn, message_id)
            .await
            .map_err(db_error)?
            .ok_or_else(|| AcpError::protocol("agent input not found"))?;
        emit_input(&state, &emitter, item.clone()).await;
        self.agent_input_runtime
            .ensure_worker(self.clone_ref(), connection_id.into())
            .await;
        Ok(item)
    }
}

fn restore_image_paths(edited: &mut super::AgentInputPayload, original: &super::AgentInputPayload) {
    use super::types::PromptInputBlock::Image;
    for block in &mut edited.blocks {
        if let Image {
            data,
            uri,
            mime_type,
            local_path,
        } = block
        {
            if local_path.is_none() {
                *local_path = original.blocks.iter().find_map(|original| match original {
                    Image {
                        data: old_data,
                        uri: old_uri,
                        mime_type: old_type,
                        local_path,
                    } if old_data == data && old_uri == uri && old_type == mime_type => {
                        local_path.clone()
                    }
                    _ => None,
                });
            }
        }
    }
}

fn db_error(error: impl ToString) -> AcpError {
    AcpError::protocol(error.to_string())
}
