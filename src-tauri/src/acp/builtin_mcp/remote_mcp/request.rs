use std::time::Duration;

use rmcp::model::{CallToolRequest, CallToolRequestParams, CallToolResult, ServerResult};
use rmcp::service::{PeerRequestOptions, RequestHandle, RoleClient, ServiceError};
use rmcp::ErrorData;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::connection::RemoteConnection;
use super::{failure, RemoteContext};

const DIRECTORY_TIMEOUT: Duration = Duration::from_secs(10);
const INVOKE_TIMEOUT: Duration = Duration::from_secs(120);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) struct RemoteRequest<'a> {
    pub name: &'static str,
    pub arguments: Value,
    pub context: RemoteContext<'a>,
    pub account_cancel: &'a CancellationToken,
    pub execution: bool,
}

pub(super) async fn call(
    connection: &RemoteConnection,
    request: RemoteRequest<'_>,
) -> Result<CallToolResult, ErrorData> {
    let result = match send(connection, &request).await {
        Ok(handle) => receive(handle, &request).await,
        Err(error) => Err(error),
    };
    if result
        .as_ref()
        .err()
        .and_then(|error| error.data.as_ref())
        .and_then(|data| data.get("code"))
        .and_then(Value::as_str)
        .is_some_and(|code| code.starts_with("remote_transport") || code == "remote_queue_timeout")
    {
        connection.stop();
    }
    result
}

async fn send(
    connection: &RemoteConnection,
    request: &RemoteRequest<'_>,
) -> Result<RequestHandle<RoleClient>, ErrorData> {
    if request.context.request_cancel.is_cancelled()
        || request.context.authority_cancel.is_cancelled()
        || request.account_cancel.is_cancelled()
    {
        return Err(failure(
            "remote_cancelled",
            "Remote request cancelled before dispatch",
            false,
        ));
    }
    let arguments = request.arguments.as_object().cloned().ok_or_else(|| {
        failure(
            "remote_arguments_invalid",
            "Remote arguments must be an object",
            false,
        )
    })?;
    let params = CallToolRequestParams::new(request.name).with_arguments(arguments);
    let send = connection.peer.send_cancellable_request(
        CallToolRequest::new(params).into(),
        PeerRequestOptions::no_options(),
    );
    tokio::time::timeout(CANCEL_TIMEOUT, send)
        .await
        .map_err(|_| {
            failure(
                "remote_queue_timeout",
                "Remote dispatch outcome is unknown",
                request.execution,
            )
        })?
        .map_err(|error| service_error(error, request.execution))
}

async fn receive(
    mut handle: RequestHandle<RoleClient>,
    request: &RemoteRequest<'_>,
) -> Result<CallToolResult, ErrorData> {
    let timeout = if request.execution {
        INVOKE_TIMEOUT
    } else {
        DIRECTORY_TIMEOUT
    };
    let response = tokio::select! {
        biased;
        _ = request.context.request_cancel.cancelled() => None,
        _ = request.context.authority_cancel.cancelled() => None,
        _ = request.account_cancel.cancelled() => None,
        _ = tokio::time::sleep(timeout) => None,
        result = &mut handle.rx => Some(result),
    };
    let Some(response) = response else {
        let _ = tokio::time::timeout(
            CANCEL_TIMEOUT,
            handle.cancel(Some("host request ended".into())),
        )
        .await;
        return Err(failure(
            "remote_interrupted",
            "Remote request cancelled or timed out; verify original task state before retrying",
            request.execution,
        ));
    };
    let response = response
        .map_err(|_| {
            failure(
                "remote_transport_closed",
                "Remote response channel closed",
                request.execution,
            )
        })?
        .map_err(|error| service_error(error, request.execution))?;
    match response {
        ServerResult::CallToolResult(result) => Ok(result),
        _ => Err(failure(
            "remote_response_invalid",
            "Remote server returned an unexpected response",
            request.execution,
        )),
    }
}

pub(super) fn service_error(error: ServiceError, execution: bool) -> ErrorData {
    match error {
        ServiceError::McpError(error) => error,
        ServiceError::Timeout { .. } => {
            failure("remote_timeout", "Remote MCP request timed out", execution)
        }
        ServiceError::Cancelled { .. } => failure(
            "remote_cancelled",
            "Remote MCP request cancelled",
            execution,
        ),
        _ => failure(
            "remote_transport_failed",
            "Remote MCP transport failed; no request was replayed",
            execution,
        ),
    }
}

pub(super) fn payload(result: CallToolResult) -> Result<Value, ErrorData> {
    let value = result
        .structured_content
        .or_else(|| {
            result
                .content
                .iter()
                .filter_map(|content| content.as_text())
                .find_map(|text| serde_json::from_str::<Value>(&text.text).ok())
        })
        .ok_or_else(|| {
            failure(
                "remote_catalog_invalid",
                "Remote directory returned no structured metadata",
                false,
            )
        })?;
    if result.is_error == Some(true)
        || value.get("success") == Some(&Value::Bool(false))
        || value.get("error").is_some_and(|error| !error.is_null())
    {
        return Err(ErrorData::invalid_request(
            "Remote directory rejected the metadata request",
            Some(value),
        ));
    }
    Ok(value)
}
