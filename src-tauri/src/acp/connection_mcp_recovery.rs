use super::{
    builtin_mcp_unavailable, prepare_companion_launch, AcpError, CompanionLaunchContext,
    CompanionLaunchPreparation, PreparedCompanion,
};

pub(super) async fn prepare(
    cached: &mut Option<CompanionLaunchPreparation>,
    context: CompanionLaunchContext<'_>,
) -> Result<
    (
        crate::user_memory::CompanionHealthSnapshot,
        Option<PreparedCompanion>,
    ),
    AcpError,
> {
    if cached.is_none() {
        *cached = Some(prepare_companion_launch(context).await?);
    } else if cached
        .as_ref()
        .is_some_and(|launch| launch.companion.is_some())
        && !context.agent_http_capable
    {
        return Err(builtin_mcp_unavailable(
            &context,
            "Replacement Agent cannot use HTTP MCP",
        ));
    }
    let prepared = cached.as_ref().expect("companion preparation completed");
    if let Some(monitor) = prepared.policy_monitor.as_ref() {
        monitor
            .require_current()
            .await
            .map_err(AcpError::from_capability_error)?;
    }
    Ok((prepared.health.clone(), prepared.companion.clone()))
}

pub(super) async fn wait_ready(
    ready: Option<&crate::acp::builtin_mcp::ToolReadiness>,
    connection_id: &str,
    cancellation: &tokio_util::sync::CancellationToken,
) -> Result<(), AcpError> {
    let Some(ready) = ready else {
        return Ok(());
    };
    let result = ready.wait(cancellation).await;
    match result {
        Ok(()) => tracing::info!(
            connection_id,
            "[ACP] Agent HTTP MCP tool catalog delivered before session ready"
        ),
        Err(reason) => tracing::warn!(
            connection_id,
            reason,
            "[ACP] Agent HTTP MCP tool catalog is not ready"
        ),
    }
    result.map_err(|reason| AcpError::BuiltinMcpUnavailable(reason.to_string()))
}

pub(super) async fn reset_transport(
    cached: Option<&CompanionLaunchPreparation>,
    client: Option<&crate::acp::builtin_mcp::BuiltinMcpClient>,
    connection_id: &str,
) -> Result<(), AcpError> {
    let Some(companion) = cached.and_then(|prepared| prepared.companion.as_ref()) else {
        return Ok(());
    };
    let client = client.ok_or_else(|| {
        AcpError::BuiltinMcpUnavailable("HTTP MCP service unavailable during recovery".to_string())
    })?;
    client
        .reset_transport(connection_id)
        .await
        .map_err(AcpError::BuiltinMcpUnavailable)?;
    companion.tools_ready.reset();
    Ok(())
}
