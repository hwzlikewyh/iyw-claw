use sacp::schema::SessionId;
use sacp::Responder;

use crate::acp::file_system_runtime::FileSystemRuntimeError;
use crate::acp::terminal_runtime::TerminalRuntimeError;
use crate::app_error::AppCommandError;

const CAPABILITY_DENIED_RPC_CODE: i32 = -32003;

pub(super) fn respond_missing<T: sacp::JsonRpcResponse>(
    responder: Responder<T>,
    session_id: &SessionId,
) -> Result<(), sacp::Error> {
    let method = responder.method().to_string();
    tracing::warn!(
        session_id = %session_id,
        method,
        "[ACP] runtime host request has no session route"
    );
    let result = responder.respond_with_error(
        sacp::Error::invalid_params().data(format!("unknown ACP session: {session_id}")),
    );
    if let Err(error) = &result {
        tracing::warn!(
            session_id = %session_id,
            method,
            error = %error,
            "[ACP] failed to deliver missing-session response"
        );
    }
    result
}

pub(super) fn respond_capability_denied<T: sacp::JsonRpcResponse>(
    responder: Responder<T>,
    session_id: &SessionId,
    error: AppCommandError,
) -> Result<(), sacp::Error> {
    tracing::warn!(
        session_id = %session_id,
        method = responder.method(),
        denial_code = error.detail.as_deref().unwrap_or("remote_policy_denied"),
        "[ACP] runtime host request denied by capability policy"
    );
    let data = serde_json::to_value(&error).unwrap_or_else(|_| {
        serde_json::json!({
            "code": "permission_denied",
            "detail": error.detail,
        })
    });
    responder.respond_with_error(
        sacp::Error::new(CAPABILITY_DENIED_RPC_CODE, "Capability is disabled").data(data),
    )
}

pub(super) fn respond_terminal<T: sacp::JsonRpcResponse>(
    responder: Responder<T>,
    session_id: &SessionId,
    result: Result<T, TerminalRuntimeError>,
) -> Result<(), sacp::Error> {
    match result {
        Ok(response) => responder.respond(response),
        Err(error) => {
            let error_kind = match &error {
                TerminalRuntimeError::InvalidParams(_) => "invalid_params",
                TerminalRuntimeError::Internal(_) => "internal",
            };
            tracing::warn!(
                session_id = %session_id,
                method = responder.method(),
                error_kind,
                "[ACP] terminal tool request failed"
            );
            responder.respond_with_error(error.into_rpc_error())
        }
    }
}

pub(super) fn respond_file_system<T: sacp::JsonRpcResponse>(
    responder: Responder<T>,
    session_id: &SessionId,
    result: Result<T, FileSystemRuntimeError>,
) -> Result<(), sacp::Error> {
    match result {
        Ok(response) => responder.respond(response),
        Err(error) => {
            let error_kind = match &error {
                FileSystemRuntimeError::InvalidParams(_) => "invalid_params",
                FileSystemRuntimeError::Internal(_) => "internal",
            };
            tracing::warn!(
                session_id = %session_id,
                method = responder.method(),
                error_kind,
                "[ACP] file-system tool request failed"
            );
            responder.respond_with_error(error.into_rpc_error())
        }
    }
}
