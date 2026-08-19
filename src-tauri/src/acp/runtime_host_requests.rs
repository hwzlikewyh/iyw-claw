use sacp::schema::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalRequest, KillTerminalResponse,
    ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    RequestPermissionRequest, RequestPermissionResponse, TerminalOutputRequest,
    TerminalOutputResponse, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
    WriteTextFileRequest, WriteTextFileResponse,
};
use sacp::Responder;
use std::sync::Arc;
use std::time::Duration;

use crate::acp::capability_policy::{
    require_runtime_agent, runtime_enforcer, Capability, CapabilityRevocationMonitor,
};
use crate::acp::connection::handle_permission_request;
use crate::acp::runtime_host_responses::{
    respond_capability_denied, respond_file_system, respond_missing, respond_terminal,
};
use crate::acp::runtime_host_router::{RuntimeSessionRoute, SessionRequestRouter};
use crate::acp::terminal_runtime::TerminalRuntime;
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

const TERMINAL_MONITOR_INTERVAL: Duration = Duration::from_secs(1);

struct TerminalPolicyWatch {
    terminal: Arc<TerminalRuntime>,
    session_id: String,
    terminal_id: String,
    monitor: CapabilityRevocationMonitor,
}

impl SessionRequestRouter {
    pub(super) async fn permission(
        &self,
        request: RequestPermissionRequest,
        responder: Responder<RequestPermissionResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        handle_permission_request(
            &route.state,
            &route.emitter,
            &route.permissions,
            &route.cwd,
            request,
            responder,
        )
        .await;
        Ok(())
    }

    pub(super) async fn read_file(
        &self,
        request: ReadTextFileRequest,
        responder: Responder<ReadTextFileResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        if let Err(error) = require_host_capability(&route, Capability::HostRead).await {
            return respond_capability_denied(responder, &session_id, error);
        }
        respond_file_system(
            responder,
            &session_id,
            route.file_system.read_text_file(request).await,
        )
    }

    pub(super) async fn write_file(
        &self,
        request: WriteTextFileRequest,
        responder: Responder<WriteTextFileResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        if let Err(error) = require_host_capability(&route, Capability::HostWrite).await {
            return respond_capability_denied(responder, &session_id, error);
        }
        respond_file_system(
            responder,
            &session_id,
            route.file_system.write_text_file(request).await,
        )
    }

    pub(super) async fn create_terminal(
        &self,
        request: CreateTerminalRequest,
        responder: Responder<CreateTerminalResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        let monitor = match monitor_host_capability(&route, Capability::Terminal).await {
            Ok(monitor) => monitor,
            Err(error) => return respond_capability_denied(responder, &session_id, error),
        };
        let result = route.terminal.create_terminal(request).await;
        if let Ok(response) = &result {
            monitor_terminal_policy(TerminalPolicyWatch {
                terminal: Arc::clone(&route.terminal),
                session_id: session_id.to_string(),
                terminal_id: response.terminal_id.to_string(),
                monitor,
            });
        }
        respond_terminal(responder, &session_id, result)
    }

    pub(super) async fn terminal_output(
        &self,
        request: TerminalOutputRequest,
        responder: Responder<TerminalOutputResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        if let Err(error) = require_host_capability(&route, Capability::Terminal).await {
            return respond_capability_denied(responder, &session_id, error);
        }
        respond_terminal(
            responder,
            &session_id,
            route.terminal.terminal_output(request).await,
        )
    }

    pub(super) async fn wait_terminal(
        &self,
        request: WaitForTerminalExitRequest,
        responder: Responder<WaitForTerminalExitResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        let monitor = match monitor_host_capability(&route, Capability::Terminal).await {
            Ok(monitor) => monitor,
            Err(error) => return respond_capability_denied(responder, &session_id, error),
        };
        let revoked = monitor.cancellation();
        let result = tokio::select! {
            result = route.terminal.wait_for_terminal_exit(request) => result,
            _ = revoked.cancelled() => {
                route.terminal.release_all_for_session(session_id.0.as_ref()).await;
                let error = monitor.error_if_revoked().err().unwrap_or_else(|| {
                    AppCommandError::permission_denied("Capability is disabled")
                });
                return respond_capability_denied(responder, &session_id, error);
            }
        };
        respond_terminal(responder, &session_id, result)
    }

    pub(super) async fn kill_terminal(
        &self,
        request: KillTerminalRequest,
        responder: Responder<KillTerminalResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_terminal(
            responder,
            &session_id,
            route.terminal.kill_terminal(request).await,
        )
    }

    pub(super) async fn release_terminal(
        &self,
        request: ReleaseTerminalRequest,
        responder: Responder<ReleaseTerminalResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_terminal(
            responder,
            &session_id,
            route.terminal.release_terminal(request).await,
        )
    }
}

async fn route_agent_type(route: &RuntimeSessionRoute) -> AgentType {
    route.state.read().await.agent_type
}

async fn require_host_capability(
    route: &RuntimeSessionRoute,
    capability: Capability,
) -> Result<(), AppCommandError> {
    require_advertised_host_capability(route, capability)?;
    require_runtime_agent(
        route_agent_type(route).await,
        capability,
        route.runtime_verified,
    )
    .await
}

async fn monitor_host_capability(
    route: &RuntimeSessionRoute,
    capability: Capability,
) -> Result<CapabilityRevocationMonitor, AppCommandError> {
    require_advertised_host_capability(route, capability)?;
    runtime_enforcer()?
        .monitor_existing_agent(
            route_agent_type(route).await,
            capability,
            route.runtime_verified,
            None,
        )
        .await
}

fn require_advertised_host_capability(
    route: &RuntimeSessionRoute,
    capability: Capability,
) -> Result<(), AppCommandError> {
    if route.runtime_verified && route.host_capabilities.contains(capability) {
        return Ok(());
    }
    tracing::warn!(
        capability = capability.key(),
        runtime_verified = route.runtime_verified,
        host_capabilities = route.host_capabilities.bits(),
        "[capability-policy] Runtime Host did not advertise requested capability"
    );
    Err(
        AppCommandError::permission_denied("Capability is not advertised by runtime Host")
            .with_detail("runtime_host_capability_not_advertised"),
    )
}

fn monitor_terminal_policy(watch: TerminalPolicyWatch) {
    let TerminalPolicyWatch {
        terminal,
        session_id,
        terminal_id,
        monitor,
    } = watch;
    let revoked = monitor.cancellation();
    tokio::spawn(async move {
        let _monitor = monitor;
        loop {
            tokio::select! {
                _ = revoked.cancelled() => {
                    terminal.release_all_for_session(&session_id).await;
                    tracing::warn!(
                        session_id,
                        terminal_id,
                        "[capability-policy] Revoked terminal process tree terminated"
                    );
                    return;
                }
                _ = tokio::time::sleep(TERMINAL_MONITOR_INTERVAL) => {
                    let request = TerminalOutputRequest::new(
                        session_id.clone(),
                        terminal_id.clone(),
                    );
                    match terminal.terminal_output(request).await {
                        Ok(response) if response.exit_status.is_some() => return,
                        Err(_) => return,
                        _ => {}
                    }
                }
            }
        }
    });
}
