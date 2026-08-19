use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use sacp::schema::{
    ClientCapabilities, CreateTerminalRequest, CreateTerminalResponse, ElicitationCapabilities,
    ElicitationFormCapabilities, FileSystemCapabilities, InitializeRequest, InitializeResponse,
    KillTerminalRequest, KillTerminalResponse, ProtocolVersion, ReadTextFileRequest,
    ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    RequestPermissionRequest, RequestPermissionResponse, TerminalOutputRequest,
    TerminalOutputResponse, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
    WriteTextFileRequest, WriteTextFileResponse,
};
use sacp::{on_receive_request, Agent, Client, ConnectTo, ConnectionTo, Responder};
use sacp_tokio::AcpAgent;
use tokio_util::sync::CancellationToken;

use crate::acp::capability_policy::Capability;
use crate::acp::deepseek_elicitation::ElicitationCreateRequest;
use crate::acp::error::AcpError;
use crate::acp::runtime_host::{HostReady, INIT_TIMEOUT_SENTINEL};
use crate::acp::runtime_host_policy::RuntimeHostCapabilities;
use crate::acp::runtime_host_registry::startup::RuntimeHostDriverOutcome;
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
    capabilities: RuntimeHostCapabilities,
    agent: AcpAgent,
    router: SessionRequestRouter,
    shutdown: CancellationToken,
    healthy: Arc<AtomicBool>,
    ready: tokio::sync::oneshot::Sender<Result<HostReady, AcpError>>,
    startup_trace: Option<crate::acp::startup_trace::StartupTrace>,
) -> tokio::task::JoinHandle<RuntimeHostDriverOutcome> {
    tokio::spawn(async move {
        let client = build_client(
            router,
            agent_type,
            capabilities,
            shutdown,
            Arc::clone(&healthy),
            ready,
            startup_trace,
        );
        let result = client.connect_to(agent).await;
        healthy.store(false, Ordering::Release);
        let outcome = RuntimeHostDriverOutcome::from_clean(result.is_ok());
        log_exit(agent_type, &fingerprint, result);
        outcome
    })
}

fn build_client(
    router: SessionRequestRouter,
    agent_type: AgentType,
    capabilities: RuntimeHostCapabilities,
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
                async move |request: ElicitationCreateRequest,
                            responder: Responder<serde_json::Value>,
                            _connection: ConnectionTo<Agent>| {
                    router.elicitation(request, responder).await
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
            match initialize_agent(
                &connection,
                agent_type,
                capabilities,
                startup_trace.as_ref(),
            )
            .await
            {
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
    capabilities: RuntimeHostCapabilities,
    startup_trace: Option<&crate::acp::startup_trace::StartupTrace>,
) -> Result<InitializeResponse, sacp::Error> {
    let terminal_enabled = capabilities.contains(Capability::Terminal);
    let read_enabled = capabilities.contains(Capability::HostRead);
    let write_enabled = capabilities.contains(Capability::HostWrite);
    let mut client_capabilities =
        ClientCapabilities::new()
            .terminal(terminal_enabled)
            .fs(FileSystemCapabilities::new()
                .read_text_file(read_enabled)
                .write_text_file(write_enabled));
    if agent_type == AgentType::DeepSeek {
        client_capabilities = client_capabilities
            .elicitation(ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()));
        tracing::info!(
            agent = %agent_type,
            "[ACP][host] advertising form elicitation capability"
        );
    }
    if matches!(agent_type, AgentType::ClaudeCode | AgentType::Codex) {
        let mut meta = serde_json::Map::new();
        meta.insert(
            "jetbrains".to_string(),
            serde_json::json!({
                "air": {
                    "version": 1,
                    "capabilities": ["sessionFailure"]
                }
            }),
        );
        client_capabilities = client_capabilities.meta(meta);
        tracing::info!(
            agent = %agent_type,
            "[ACP][host] advertising AIR session failure capability"
        );
    }
    let request =
        InitializeRequest::new(ProtocolVersion::LATEST).client_capabilities(client_capabilities);
    let started = Instant::now();
    let timeout = initialize_timeout(agent_type);
    let startup_stage = startup_trace.map(|trace| trace.stage("initialize"));
    tracing::info!(
        agent = %agent_type,
        protocol = %ProtocolVersion::LATEST,
        timeout_seconds = timeout.as_secs(),
        host_capabilities = capabilities.bits(),
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
