use sea_orm::EntityTrait;

use crate::acp::error::AcpError;
use crate::acp::manager::ConnectionManager;
use crate::acp::AcpEvent;
use crate::db::entities::conversation;
use crate::db::AppDatabase;
use crate::models::AgentType;
use crate::web::event_bridge::emit_with_state;

async fn load_conversation(
    db: &AppDatabase,
    conversation_id: i32,
) -> Result<conversation::Model, AcpError> {
    conversation::Entity::find_by_id(conversation_id)
        .one(&db.conn)
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))?
        .filter(|row| row.deleted_at.is_none())
        .ok_or_else(|| AcpError::protocol("conversation not found".into()))
}

fn validate_identity(
    row: &conversation::Model,
    linked_conversation: Option<i32>,
    agent_type: AgentType,
    conversation_id: i32,
) -> Result<(), AcpError> {
    if linked_conversation.is_some_and(|linked| linked != conversation_id) {
        return Err(AcpError::protocol(
            "connection is linked to a different conversation".into(),
        ));
    }
    let serialized_agent = serde_json::to_value(agent_type)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned));
    if serialized_agent.as_deref() != Some(row.agent_type.as_str()) {
        return Err(AcpError::protocol(
            "conversation agent does not match connection".into(),
        ));
    }
    Ok(())
}

impl ConnectionManager {
    pub async fn resume_agent_input_connection(
        &self,
        db: &AppDatabase,
        conn_id: &str,
        conversation_id: i32,
    ) -> Result<(), AcpError> {
        let row = load_conversation(db, conversation_id).await?;
        let (state, emitter) = self
            .get_state_and_emitter(conn_id)
            .await
            .ok_or_else(|| AcpError::ConnectionNotFound(conn_id.into()))?;
        let (linked_conversation, agent_type) = {
            let snapshot = state.read().await;
            (snapshot.conversation_id, snapshot.agent_type)
        };
        validate_identity(&row, linked_conversation, agent_type, conversation_id)?;
        if linked_conversation.is_none() {
            emit_with_state(
                &state,
                &emitter,
                AcpEvent::ConversationLinked {
                    conversation_id,
                    folder_id: row.folder_id,
                    parent_conversation_id: None,
                    parent_tool_use_id: None,
                },
            )
            .await;
        }
        crate::acp::agent_input_lifecycle::recover_connection(
            &db.conn,
            self,
            conn_id,
            conversation_id,
        )
        .await
        .map_err(|error| AcpError::protocol(error.to_string()))
    }
}
