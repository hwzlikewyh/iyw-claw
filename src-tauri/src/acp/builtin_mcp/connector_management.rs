use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::acp::delegation::listener::DelegationListener;

use super::authority::SessionContext;
use super::delivery::RelayDelivery;
use super::handler::BuiltinMcpHandler;
use super::invocation::{ensure_active, execute_invocation, InvocationContext};
use super::plugin_control_support::approved;

mod dispatch;
mod market;
mod publish;
mod request;

use request::Request;

pub(super) async fn execute(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    arguments: Value,
    request_id: Value,
    request_cancel: CancellationToken,
) -> Result<CallToolResult, ErrorData> {
    let request = serde_json::from_value::<Request>(arguments)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
    if request.is_mutation() {
        super::policy::require_call(&authority).await?;
    }
    dispatch::execute(
        handler,
        authority,
        delivery,
        request,
        request_id,
        request_cancel,
    )
    .await
}

async fn set_current_conversation(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: Value,
    request_cancel: CancellationToken,
    server_id: String,
    enabled: bool,
) -> Result<CallToolResult, ErrorData> {
    ensure_active(&authority, &request_cancel)?;
    let conversation_id = active_conversation_id(handler, &authority).await?;
    let database = &handler.gateway_service().conn;
    let current = crate::commands::mcp_conversation::read(database, conversation_id)
        .await
        .map_err(command_error)?;
    let choice = current
        .connectors
        .iter()
        .find(|choice| choice.id == server_id)
        .ok_or_else(|| ErrorData::invalid_params("未检测到该连接器", None))?;
    if enabled && !choice.compatible {
        return Err(ErrorData::invalid_params(
            choice
                .reason
                .clone()
                .unwrap_or_else(|| "当前对话不支持该连接器".into()),
            None,
        ));
    }
    let selected_legacy_connector = current.selected.iter().any(|selected_id| {
        (*selected_id != server_id || enabled)
            && current
                .connectors
                .iter()
                .any(|item| item.id == *selected_id && item.requires_migration)
    });
    if enabled && (choice.requires_migration || selected_legacy_connector) {
        let question = format!(
            "Move connector '{}' out of Agent global configuration and enable it only for this conversation?",
            choice.name
        );
        if !confirm(
            handler,
            authority.clone(),
            delivery.clone(),
            request_id.clone(),
            request_cancel.clone(),
            question,
            "Move and enable",
        )
        .await?
        {
            return Ok(cancelled("set_current_conversation"));
        }
    }
    ensure_active(&authority, &request_cancel)?;
    let result =
        update_current_selection(database, conversation_id, current, &server_id, enabled).await?;
    Ok(CallToolResult::structured(json!({
        "success": true,
        "operation": "set_current_conversation",
        "server_id": server_id,
        "enabled": enabled,
        "migrated": result.migrated,
        "reconnect_required": result.reconnect_required
    })))
}

async fn active_conversation_id(
    handler: &BuiltinMcpHandler,
    authority: &SessionContext,
) -> Result<i32, ErrorData> {
    let listener: &DelegationListener = handler.gateway_service().listener();
    listener
        .parent_lookup
        .current_conversation_id(authority.connection_id())
        .await
        .ok_or_else(|| ErrorData::invalid_params("当前连接没有活动对话", None))
}

async fn update_current_selection(
    database: &sea_orm::DatabaseConnection,
    conversation_id: i32,
    current: crate::commands::mcp_conversation::ConversationConnectors,
    server_id: &str,
    enabled: bool,
) -> Result<crate::commands::mcp_conversation::ConversationConnectors, ErrorData> {
    let mut selected = current.selected;
    if enabled {
        selected.insert(server_id.to_string());
    } else {
        selected.remove(server_id);
    }
    crate::commands::mcp_conversation::update(
        database,
        crate::commands::mcp_conversation::UpdateConnectors {
            conversation_id,
            revision: current.revision,
            selected,
        },
    )
    .await
    .map_err(command_error)
}

async fn confirm(
    handler: &BuiltinMcpHandler,
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: Value,
    request_cancel: CancellationToken,
    question: String,
    approve_label: &'static str,
) -> Result<bool, ErrorData> {
    ensure_active(&authority, &request_cancel)?;
    let invocation = super::capability::ResolvedCapability {
        tool_name: "ask_user_question".into(),
        arguments: json!({"questions": [{
            "id": "connector-management-approval",
            "header": "Manage connectors",
            "question": question,
            "options": [
                {"label": approve_label, "description": "Apply the requested connector change."},
                {"label": "Cancel", "description": "Leave connector settings unchanged."}
            ]
        }]}),
        delivery_ack: None,
    };
    let result = execute_invocation(
        handler.invocation_dependencies(),
        InvocationContext {
            authority,
            delivery,
            invocation,
            request_id,
            request_cancel,
        },
    )
    .await?;
    Ok(approved(&result, approve_label))
}

pub(super) fn command_error(error: crate::app_error::AppCommandError) -> ErrorData {
    ErrorData::internal_error(error.message, error.detail.map(Value::String))
}

pub(super) fn cancelled(operation: &str) -> CallToolResult {
    CallToolResult::structured(json!({"success": false, "cancelled": true, "operation": operation}))
}

pub(super) fn success(operation: &str, value: Value) -> CallToolResult {
    CallToolResult::structured(json!({"success": true, "operation": operation, "result": value}))
}

pub(super) use confirm as request_confirmation;
