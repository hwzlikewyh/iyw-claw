use serde_json::{json, Value};

use super::authority::SessionContext;
use super::iyw_service::IywGatewayService;

// 既有成果登记契约每次最多 100 个文件；大批图片分段登记。
const MAX_ARTIFACTS_PER_REGISTRATION: usize = 100;

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
    let mut result = register_urls(service, authority, urls).await;
    if result.get("error").and_then(Value::as_str) == Some("conversation_unavailable") {
        return result;
    }
    let caller = crate::acp::channel_tools::ChannelCaller {
        agent_type: authority.agent_type().to_string(),
        session_ref: authority.connection_id().to_owned(),
        caller_scope: "artifact-notifications".into(),
        working_dir: authority.cwd().to_path_buf(),
    };
    result["notification"] = service
        .listener()
        .channel_tools
        .notify_artifacts(caller, &result, Default::default())
        .await;
    result
}

async fn register_urls(
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
    let mut batches = Vec::new();
    for chunk in urls.chunks(MAX_ARTIFACTS_PER_REGISTRATION) {
        batches.push(
            listener
                .artifacts
                .register_task_artifacts(
                    authority.connection_id(),
                    conversation_id,
                    message_id.clone(),
                    turn_generation,
                    authority.cwd(),
                    chunk.to_vec(),
                    vec![None; chunk.len()],
                )
                .await,
        );
    }
    merge_registrations(batches, message_id)
}

fn merge_registrations(mut batches: Vec<Value>, message_id: Option<String>) -> Value {
    if batches.len() == 1 {
        return batches.pop().expect("one registration result");
    }
    let accepted = registration_items(&batches, "accepted");
    let rejected = registration_items(&batches, "rejected");
    let error = batches.iter().find_map(|batch| batch.get("error").cloned());
    let mut result = json!({"accepted": accepted, "rejected": rejected,
        "message_id": message_id, "batches": batches});
    if let Some(error) = error {
        result["error"] = error;
    }
    result
}

fn registration_items(batches: &[Value], key: &str) -> Vec<Value> {
    batches
        .iter()
        .flat_map(|batch| {
            batch
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .cloned()
        .collect()
}
