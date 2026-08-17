use sacp::schema::SessionId;
use sacp::Responder;

use crate::acp::file_system_runtime::FileSystemRuntimeError;
use crate::acp::terminal_runtime::TerminalRuntimeError;

pub(super) fn respond_missing<T: sacp::JsonRpcResponse>(
    responder: Responder<T>,
    session_id: &SessionId,
) -> Result<(), sacp::Error> {
    responder.respond_with_error(
        sacp::Error::invalid_params().data(format!("unknown ACP session: {session_id}")),
    )
}

pub(super) fn respond_terminal<T: sacp::JsonRpcResponse>(
    responder: Responder<T>,
    result: Result<T, TerminalRuntimeError>,
) -> Result<(), sacp::Error> {
    match result {
        Ok(response) => responder.respond(response),
        Err(error) => responder.respond_with_error(error.into_rpc_error()),
    }
}

pub(super) fn respond_file_system<T: sacp::JsonRpcResponse>(
    responder: Responder<T>,
    result: Result<T, FileSystemRuntimeError>,
) -> Result<(), sacp::Error> {
    match result {
        Ok(response) => responder.respond(response),
        Err(error) => responder.respond_with_error(error.into_rpc_error()),
    }
}
