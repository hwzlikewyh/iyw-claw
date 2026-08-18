use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use sacp::schema::{
    ClientCapabilities, CreateTerminalRequest, CreateTerminalResponse, FileSystemCapabilities,
    InitializeRequest, InitializeResponse, KillTerminalRequest, KillTerminalResponse,
    ProtocolVersion, ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest,
    ReleaseTerminalResponse, RequestPermissionRequest, RequestPermissionResponse,
    TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse, WriteTextFileRequest, WriteTextFileResponse,
};
use sacp::{on_receive_request, Agent, Client, ConnectTo, ConnectionTo, Responder};
use sacp_tokio::AcpAgent;
use tokio_util::sync::CancellationToken;

use crate::acp::error::AcpError;
use crate::acp::runtime_host::{HostReady, INIT_TIMEOUT_SENTINEL};
use crate::acp::runtime_host_router::SessionRequestRouter;
use crate::models::agent::AgentType;

const DEFAULT_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(60);
const CODEX_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(120);

const fn initialize_timeout(agent_type: AgentType) -> Duration {
    match agent_type {
        AgentType::Codex => CODEX_INITIALIZE_TIMEOUT,
        _ => DEFAULT_INITIALIZE_TIMEOUT,
    }
}

pub(super) fn spawn(
    agent_type: AgentType,
    fingerprint: String,
    agent: AcpAgent,
    router: SessionRequestRouter,
    shutdown: CancellationToken,
    healthy: Arc<AtomicBool>,
    ready: tokio::sync::oneshot::Sender<Result<HostReady, AcpError>>,
    startup_trace: Option<crate::acp::startup_trace::StartupTrace>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let client = build_client(
            router,
            agent_type,
            shutdown,
            Arc::clone(&healthy),
            ready,
            startup_trace,
        );
        let result = client.connect_to(agent).await;
        healthy.store(false, Ordering::Release);
        log_exit(agent_type, &fingerprint, result);
    })
}

fn build_client(
    router: SessionRequestRouter,
    agent_type: AgentType,
    shutdown: CancellationToken,
    healthy: Arc<AtomicBool>,
    ready: tokio::sync::oneshot::Sender<Result<HostReady, AcpError>>,
    startup_trace: Option<crate::acp::startup_trace::StartupTrace>,
) -> impl ConnectTo<Agent> {
    Client
        .builder()
        .name("iyw-claw-runtime-host")
        .on_receive_request(
            {
                let router = router.clone();
                async move |request: RequestPermissionRequest,
                            responder: Responder<RequestPermissionResponse>,
                            _connection: ConnectionTo<Agent>| {
                    router.permission(request, responder).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let router = router.clone();
                async move |request: ReadTextFileRequest,
                            responder: Responder<ReadTextFileResponse>,
                            _connection: ConnectionTo<Agent>| {
                    router.read_file(request, responder).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let router = router.clone();
                async move |request: WriteTextFileRequest,
                            responder: Responder<WriteTextFileResponse>,
                            _connection: ConnectionTo<Agent>| {
                    router.write_file(request, responder).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let router = router.clone();
                async move |request: CreateTerminalRequest,
                            responder: Responder<CreateTerminalResponse>,
                            _connection: ConnectionTo<Agent>| {
                    router.create_terminal(request, responder).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let router = router.clone();
                async move |request: TerminalOutputRequest,
                            responder: Responder<TerminalOutputResponse>,
                            _connection: ConnectionTo<Agent>| {
                    router.terminal_output(request, responder).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let router = router.clone();
                async move |request: WaitForTerminalExitRequest,
                            responder: Responder<WaitForTerminalExitResponse>,
                            _connection: ConnectionTo<Agent>| {
                    router.wait_terminal(request, responder).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let router = router.clone();
                async move |request: KillTerminalRequest,
                            responder: Responder<KillTerminalResponse>,
                            _connection: ConnectionTo<Agent>| {
                    router.kill_terminal(request, responder).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ReleaseTerminalRequest,
                        responder: Responder<ReleaseTerminalResponse>,
                        _connection: ConnectionTo<Agent>| {
                router.release_terminal(request, responder).await
            },
            on_receive_request!(),
        )
        .with_spawned(move |connection| async move {
            match initialize_agent(&connection, agent_type, startup_trace.as_ref()).await {
                Ok(initialize_response) => {
                    healthy.store(true, Ordering::Release);
                    let _ = ready.send(Ok(HostReady {
                        connection: connection.clone(),
                        initialize_response,
                    }));
                }
                Err(error) => {
                    let raw = error.to_string();
                    let mapped = if raw.contains(INIT_TIMEOUT_SENTINEL) {
                        AcpError::InitializeTimeout
                    } else {
                        AcpError::protocol(raw)
                    };
                    let _ = ready.send(Err(mapped));
                    return Err(error);
                }
            }
            shutdown.cancelled().await;
            Ok(())
        })
}

async fn initialize_agent(
    connection: &ConnectionTo<Agent>,
    agent_type: AgentType,
    startup_trace: Option<&crate::acp::startup_trace::StartupTrace>,
) -> Result<InitializeResponse, sacp::Error> {
    let request = InitializeRequest::new(ProtocolVersion::LATEST).client_capabilities(
        ClientCapabilities::new()
            .terminal(true)
            .fs(FileSystemCapabilities::new()
                .read_text_file(true)
                .write_text_file(true)),
    );
    let started = Instant::now();
    let timeout = initialize_timeout(agent_type);
    let startup_stage = startup_trace.map(|trace| trace.stage("initialize"));
    tracing::info!(
        agent = %agent_type,
        protocol = %ProtocolVersion::LATEST,
        timeout_seconds = timeout.as_secs(),
        "[ACP][host] initialize started"
    );
    match tokio::time::timeout(
        timeout,
        connection.send_request_to(Agent, request).block_task(),
    )
    .await
    {
        Ok(Ok(response)) => {
            if let Some(stage) = startup_stage {
                stage.finish("ok");
            }
            tracing::info!(
                agent = %agent_type,
                elapsed_ms = started.elapsed().as_millis(),
                "[ACP][host] initialize completed"
            );
            Ok(response)
        }
        Ok(Err(error)) => {
            if let Some(stage) = startup_stage {
                stage.finish("error");
            }
            tracing::error!(
                agent = %agent_type,
                elapsed_ms = started.elapsed().as_millis(),
                error = %error,
                "[ACP][host] initialize failed"
            );
            Err(error)
        }
        Err(_) => {
            if let Some(stage) = startup_stage {
                stage.finish("timeout");
            }
            tracing::error!(
                agent = %agent_type,
                elapsed_ms = started.elapsed().as_millis(),
                "[ACP][host] initialize timed out"
            );
            Err(sacp::util::internal_error(INIT_TIMEOUT_SENTINEL))
        }
    }
}

fn log_exit(agent_type: AgentType, fingerprint: &str, result: Result<(), sacp::Error>) {
    let fingerprint = fingerprint.get(..12).unwrap_or(fingerprint);
    match result {
        Ok(()) => tracing::info!(
            agent = %agent_type,
            fingerprint,
            "[ACP][host] runtime Host stopped"
        ),
        Err(error) => tracing::error!(
            agent = %agent_type,
            fingerprint,
            error = %error,
            "[ACP][host] runtime Host exited"
        ),
    }
}
