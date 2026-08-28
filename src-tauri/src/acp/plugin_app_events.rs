use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Map, Value};
use tokio::sync::RwLock;

use super::session_state::SessionState;
use crate::models::message::{ContentBlock, MessageTurn};
use crate::plugin_runtime::app_launch_broker::TICKET_META_KEY;

pub(crate) const APP_META_KEY: &str = "iyw-claw.plugin-app";

pub(crate) struct PluginAppEventInput<'a> {
    pub state: &'a Arc<RwLock<SessionState>>,
    pub tool_call_id: &'a str,
    pub meta: Option<Value>,
    pub raw_output: Option<&'a Value>,
}

struct SessionAppIdentity {
    connection_id: String,
    conversation_id: i64,
    workspace_key: String,
}

pub(crate) async fn enrich_tool_meta(input: PluginAppEventInput<'_>) -> Option<Value> {
    let existing = existing_app_meta(input.state, input.tool_call_id).await;
    let ticket = extract_ticket(input.meta.as_ref()).or_else(|| extract_ticket(input.raw_output));
    let mut meta = merge_existing(input.meta, existing);
    remove_ticket(&mut meta);
    let Some(ticket) = ticket else {
        return meta;
    };
    let Some(identity) = session_identity(input.state).await else {
        tracing::warn!(
            tool_call_id = input.tool_call_id,
            "[plugin-app] session identity unavailable"
        );
        return meta;
    };
    let instance = create_instance(&identity, input.tool_call_id, &ticket).await;
    match instance {
        Ok(instance_id) => insert_app_meta(meta, &instance_id),
        Err(error) => {
            tracing::error!(tool_call_id = input.tool_call_id, error = %error.message,
                error_code = error.code, "[plugin-app] launch ticket claim failed");
            meta
        }
    }
}

pub(crate) async fn inject_history_meta(
    conn: &sea_orm::DatabaseConnection,
    conversation_id: i32,
    turns: &mut [MessageTurn],
) {
    let instances = crate::db::service::plugin_app_instance_service::list_for_conversation(
        conn,
        i64::from(conversation_id),
    )
    .await
    .unwrap_or_default();
    let by_tool = instances
        .into_iter()
        .map(|instance| (instance.tool_call_id, instance.instance_id))
        .collect::<HashMap<_, _>>();
    for turn in turns {
        for block in &mut turn.blocks {
            inject_block_meta(block, &by_tool);
        }
    }
}

fn inject_block_meta(block: &mut ContentBlock, by_tool: &HashMap<String, String>) {
    let ContentBlock::ToolUse {
        tool_use_id: Some(tool_call_id),
        meta,
        ..
    } = block
    else {
        return;
    };
    let Some(instance_id) = by_tool.get(tool_call_id) else {
        return;
    };
    *meta = insert_app_meta(meta.take(), instance_id);
}

async fn session_identity(state: &Arc<RwLock<SessionState>>) -> Option<SessionAppIdentity> {
    let state = state.read().await;
    let conversation_id = i64::from(state.conversation_id?);
    let workspace = state.working_dir.as_ref()?.to_string_lossy();
    Some(SessionAppIdentity {
        connection_id: state.connection_id.clone(),
        conversation_id,
        workspace_key: crate::commands::skill_inventory::workspace_key(Some(&workspace)),
    })
}

async fn existing_app_meta(state: &Arc<RwLock<SessionState>>, tool_call_id: &str) -> Option<Value> {
    state
        .read()
        .await
        .active_tool_calls
        .get(tool_call_id)
        .and_then(|tool| tool.meta.as_ref())
        .and_then(|meta| meta.get(APP_META_KEY))
        .cloned()
}

async fn create_instance(
    identity: &SessionAppIdentity,
    tool_call_id: &str,
    ticket: &str,
) -> Result<String, crate::plugin_runtime::types::PluginInvokeError> {
    let intent = crate::plugin_runtime::global::app_launch_broker().claim(
        ticket,
        &identity.connection_id,
        &identity.workspace_key,
    )?;
    let payload = json!({
        "arguments": intent.launch_payload["arguments"].clone(),
        "result": intent.launch_payload["result"].clone(),
        "permissionRevision": intent.permission_revision,
        "displayMode": intent.display_mode,
    });
    let apps = crate::plugin_runtime::global::apps().ok_or_else(|| {
        app_error(
            "plugin_app_unavailable",
            "Plugin app registry is unavailable",
        )
    })?;
    let database = crate::plugin_runtime::global::database()
        .ok_or_else(|| app_error("plugin_app_unavailable", "Plugin database is unavailable"))?;
    let launch = apps
        .create_persisted(
            &database,
            crate::plugin_runtime::app_host::PluginAppLaunchInput {
                conversation_id: identity.conversation_id,
                tool_call_id: tool_call_id.to_string(),
                plugin_slug: intent.plugin_slug,
                plugin_version: intent.plugin_version,
                app_key: intent.app_key,
                resource_uri: intent.resource_uri,
                display_mode: intent.display_mode,
                workspace_key: intent.workspace_key,
                launch_payload: payload,
            },
        )
        .await?;
    Ok(launch.instance_id)
}

fn extract_ticket(value: Option<&Value>) -> Option<String> {
    find_ticket(value?, 0)
}

fn find_ticket(value: &Value, depth: usize) -> Option<String> {
    if depth > 8 {
        return None;
    }
    match value {
        Value::Object(object) => object
            .get(TICKET_META_KEY)
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                object
                    .values()
                    .find_map(|value| find_ticket(value, depth + 1))
            }),
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_ticket(value, depth + 1)),
        Value::String(text) if text.len() <= 64 * 1024 => serde_json::from_str(text)
            .ok()
            .and_then(|value| find_ticket(&value, depth + 1)),
        _ => None,
    }
}

fn merge_existing(meta: Option<Value>, existing: Option<Value>) -> Option<Value> {
    let mut object = match meta {
        Some(Value::Object(object)) => object,
        _ => Map::new(),
    };
    if let Some(existing) = existing {
        object.entry(APP_META_KEY.to_string()).or_insert(existing);
    }
    (!object.is_empty()).then_some(Value::Object(object))
}

fn remove_ticket(meta: &mut Option<Value>) {
    if let Some(Value::Object(object)) = meta {
        object.remove(TICKET_META_KEY);
    }
}

fn insert_app_meta(meta: Option<Value>, instance_id: &str) -> Option<Value> {
    let mut object = match meta {
        Some(Value::Object(object)) => object,
        _ => Map::new(),
    };
    object.insert(APP_META_KEY.to_string(), json!({"instanceId": instance_id}));
    Some(Value::Object(object))
}

fn app_error(
    code: &'static str,
    message: impl Into<String>,
) -> crate::plugin_runtime::types::PluginInvokeError {
    crate::plugin_runtime::types::PluginInvokeError::before_effect(code, message)
}
