use serde_json::{json, Value};

use super::authority::SessionContext;
use super::iyw_service::IywGatewayService;

pub(super) async fn deliver(
    service: &IywGatewayService,
    authority: &SessionContext,
    urls: &[String],
    _display: bool,
    register_artifact: bool,
) -> Value {
    let artifact = if register_artifact {
        Some(register_artifacts(service, authority, urls).await)
    } else {
        None
    };
    json!({"displayed": [], "artifact": artifact})
}

async fn register_artifacts(
    service: &IywGatewayService,
    authority: &SessionContext,
    urls: &[String],
) -> Value {
    let listener = service.listener();
    let Some(conversation_id) = listener
        .parent_lookup
        .current_conversation_id(authority.connection_id())
        .await
    else {
        return json!({"accepted": [], "rejected": [], "error": "conversation_unavailable"});
    };
    let turn_generation = listener
        .parent_lookup
        .current_turn_generation(authority.connection_id())
        .await;
    let message_id = listener
        .parent_lookup
        .current_assistant_message_id(authority.connection_id())
        .await;
    listener
        .artifacts
        .register_task_artifacts(
            authority.connection_id(),
            conversation_id,
            message_id,
            turn_generation,
            authority.cwd(),
            urls.to_vec(),
        )
        .await
}
