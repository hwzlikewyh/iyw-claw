use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::Value;

use crate::acp::delegation::companion::{PostRelayAction, SpawnResult};

use super::delivery::RelayDelivery;
use super::receipt::DeliveryReceiptRegistry;

pub(super) struct SpawnResultContext<'a> {
    pub(super) result: SpawnResult,
    pub(super) delivery: Option<RelayDelivery>,
    pub(super) receipts: &'a DeliveryReceiptRegistry,
    pub(super) parent_connection_id: &'a str,
    pub(super) rewrite_delegation_guidance: bool,
}

struct ReceiptContext<'a> {
    delivery: Option<RelayDelivery>,
    receipts: &'a DeliveryReceiptRegistry,
    parent_connection_id: &'a str,
}

struct JsonRpcFailure {
    code: i64,
    message: String,
    data: Option<Value>,
    rewrite_delegation_guidance: bool,
}

pub(super) fn map_spawn_result(
    context: SpawnResultContext<'_>,
) -> Result<CallToolResult, ErrorData> {
    let SpawnResultContext {
        result,
        delivery,
        receipts,
        parent_connection_id,
        rewrite_delegation_guidance,
    } = context;
    let SpawnResult {
        response,
        after_relay,
    } = result;
    let Some(response) = response else {
        return Err(ErrorData::invalid_request("MCP request cancelled", None));
    };
    if let Some(error) = response.error {
        return Err(map_json_rpc_error(JsonRpcFailure {
            code: error.code,
            message: error.message,
            data: error.data,
            rewrite_delegation_guidance,
        }));
    }
    let value = response
        .result
        .ok_or_else(|| ErrorData::internal_error("missing MCP tool result", None))?;
    let mut mapped: CallToolResult = serde_json::from_value(value)
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
    if rewrite_delegation_guidance {
        super::capability_response::rewrite_result(&mut mapped);
    }
    attach_receipt(
        after_relay,
        &mut mapped,
        ReceiptContext {
            delivery,
            receipts,
            parent_connection_id,
        },
    )?;
    Ok(mapped)
}

pub(super) fn catalog_error(error: serde_json::Error) -> ErrorData {
    tracing::error!(
        target: "builtin_mcp",
        error = %error,
        "failed to build HTTP MCP gateway catalog"
    );
    ErrorData::internal_error("failed to build MCP gateway catalog", None)
}

fn attach_receipt(
    after_relay: Option<PostRelayAction>,
    mapped: &mut CallToolResult,
    context: ReceiptContext<'_>,
) -> Result<(), ErrorData> {
    let Some(callback) = after_relay else {
        return Ok(());
    };
    if context.receipts.attach(
        mapped,
        context.delivery,
        context.parent_connection_id,
        callback,
    ) {
        return Ok(());
    }
    tracing::warn!(
        target: "builtin_mcp",
        "HTTP MCP delivery receipt unavailable; feedback delivery rejected"
    );
    Err(ErrorData::internal_error(
        "feedback delivery receipt capacity reached; retry the request",
        None,
    ))
}

fn map_json_rpc_error(failure: JsonRpcFailure) -> ErrorData {
    let (message, data) = if failure.rewrite_delegation_guidance {
        super::capability_response::rewrite_error(failure.message, failure.data)
    } else {
        (failure.message, failure.data)
    };
    match failure.code {
        -32601 => ErrorData::invalid_request(message, data),
        -32602 => ErrorData::invalid_params(message, data),
        _ => ErrorData::internal_error(message, data),
    }
}
