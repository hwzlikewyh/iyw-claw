use serde_json::{json, Value};

use super::authority::SessionContext;
use super::iyw_service::IywGatewayService;

pub(super) async fn deliver(
    service: &IywGatewayService,
    authority: &SessionContext,
    urls: &[String],
    display: bool,
    register_artifact: bool,
) -> Value {
    let displayed = if display {
        display_images(authority, urls).await
    } else {
        Vec::new()
    };
    let artifact = if register_artifact {
        Some(register_artifacts(service, authority, urls).await)
    } else {
        None
    };
    json!({"displayed": displayed, "artifact": artifact})
}

async fn display_images(authority: &SessionContext, urls: &[String]) -> Vec<Value> {
    let mut displayed = Vec::new();
    for (index, url) in urls.iter().enumerate() {
        displayed.push(
            crate::acp::delegation::image_tool::execute(
                json!({"source": url, "name": format!("generated-{}.png", index + 1)}),
                authority.cwd().to_path_buf(),
            )
            .await,
        );
    }
    displayed
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
    listener
        .artifacts
        .register_task_artifacts(
            conversation_id,
            turn_generation,
            authority.cwd(),
            urls.to_vec(),
        )
        .await
}
