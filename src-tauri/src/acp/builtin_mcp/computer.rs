use rmcp::ErrorData;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::acp::delegation::companion::{ok, SpawnResult};
use crate::computer::tool_dispatch::{execute, ComputerActor, ComputerCall, CALL_ACTOR};

use super::authority::SessionContext;
use super::invocation::ensure_active;

pub(super) struct ComputerInvocation {
    pub request_id: Value,
    pub tool_name: String,
    pub arguments: Value,
    pub cancellation: CancellationToken,
}

pub(super) async fn run(
    request: ComputerInvocation,
    authority: &SessionContext,
) -> Result<SpawnResult, ErrorData> {
    ensure_active(authority, &request.cancellation)?;
    let service = crate::computer::bootstrap::current_service().ok_or_else(|| {
        ErrorData::invalid_request("Computer Use is unavailable on this host", None)
    })?;
    if !service.tools_enabled() {
        return Err(ErrorData::invalid_request("Computer Use is disabled", None));
    }
    let started = std::time::Instant::now();
    tracing::info!(connection_id = authority.connection_id(), agent = %authority.agent_type(), tool = %request.tool_name, "[computer] agent call started");
    let actor = ComputerActor {
        connection_id: authority.connection_id().into(),
        agent: authority.agent_type().to_string(),
    };
    let call = CALL_ACTOR.scope(
        actor,
        execute(
            &service,
            ComputerCall {
                tool: &request.tool_name,
                arguments: &request.arguments,
            },
        ),
    );
    let result = tokio::select! {
        biased;
        _ = request.cancellation.cancelled() => return cancelled(&service).await,
        _ = authority.cancellation().cancelled() => return cancelled(&service).await,
        result = call => result,
    };
    if ensure_active(authority, &request.cancellation).is_err() {
        return cancelled(&service).await;
    }
    let result = result.map_err(|reason| ErrorData::invalid_params(reason, None))?;
    let error_code = result
        .pointer("/structuredContent/error")
        .and_then(Value::as_str);
    tracing::info!(connection_id = authority.connection_id(), tool = %request.tool_name, duration_ms = started.elapsed().as_millis() as u64, error_code, "[computer] agent call finished");
    Ok(SpawnResult {
        response: Some(ok(request.request_id, result)),
        after_relay: None,
    })
}

async fn cancelled(
    service: &crate::commands::computer::ComputerService,
) -> Result<SpawnResult, ErrorData> {
    service.cancel_operation().await;
    Err(ErrorData::invalid_request(
        "Computer Use operation cancelled; verify the target before another action",
        Some(json!({"execution_status": "unknown"})),
    ))
}
