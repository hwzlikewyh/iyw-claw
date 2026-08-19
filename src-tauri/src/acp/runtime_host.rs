use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex as StdMutex,
};
use std::time::{Duration, Instant};

use sacp::schema::InitializeResponse;
use sacp::{Agent, ConnectionTo};
use sacp_tokio::AcpAgent;
use tokio_util::sync::CancellationToken;

use crate::acp::capability_policy::{runtime_enforcer, Capability, CapabilityRevocationMonitor};
use crate::acp::error::AcpError;
use crate::acp::runtime_host_driver;
pub(crate) use crate::acp::runtime_host_policy::RuntimeHostCapabilities;
use crate::acp::runtime_host_registry::startup::{HostStartupEntry, HostStartups};
pub(crate) use crate::acp::runtime_host_registry::{
    RuntimeHostIdentity, RuntimeHostKey, RuntimeHostRegistry,
};
use crate::acp::runtime_host_router::SessionRequestRouter;
pub(crate) use crate::acp::runtime_host_router::{
    RuntimeHostRouteBinding, RuntimeHostRouteLease, RuntimeSessionRoute,
};
use crate::acp::stderr_tail::StderrTail;

mod lifecycle;
mod startup;

pub(crate) use lifecycle::RuntimeHostReservation;
use startup::HostStartupGuard;

pub(crate) const INIT_TIMEOUT_SENTINEL: &str = "__IYW_CLAW_ACP_INIT_TIMEOUT__";
const DEFAULT_HOST_READY_TIMEOUT: Duration = Duration::from_secs(65);
const CODEX_HOST_READY_TIMEOUT: Duration = Duration::from_secs(125);
const HOST_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const HOST_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

const fn host_ready_timeout(agent_type: crate::models::agent::AgentType) -> Duration {
    match agent_type {
        crate::models::agent::AgentType::Codex => CODEX_HOST_READY_TIMEOUT,
        _ => DEFAULT_HOST_READY_TIMEOUT,
    }
}

pub(crate) struct AgentRuntimeHost {
    key: RuntimeHostKey,
    connection: ConnectionTo<Agent>,
    initialize_response: InitializeResponse,
    stderr_tail: Arc<StderrTail>,
    router: SessionRequestRouter,
    shutdown: CancellationToken,
    healthy: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    driver: Arc<HostStartupEntry>,
    created_at: Instant,
    active_routes: AtomicUsize,
    reservations: AtomicUsize,
    route_epoch: AtomicU64,
    route_guard: StdMutex<()>,
    idle_retirement: StdMutex<Option<tokio::task::AbortHandle>>,
    _policy_monitors: Vec<CapabilityRevocationMonitor>,
}

impl AgentRuntimeHost {
    pub(super) async fn start(
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
        startup_cancel: CancellationToken,
        startup_trace: Option<crate::acp::startup_trace::StartupTrace>,
        startups: &HostStartups,
    ) -> Result<Arc<Self>, AcpError> {
        let router = SessionRequestRouter::default();
        let shutdown = CancellationToken::new();
        let healthy = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicU32::new(0));
        let policy_monitors =
            start_policy_monitors(key.agent_type, key.capabilities, &shutdown).await?;
        for monitor in &policy_monitors {
            monitor
                .require_current()
                .await
                .map_err(AcpError::from_capability_error)?;
        }
        let agent = agent.with_spawned_pid({
            let pid = Arc::clone(&pid);
            move |value| pid.store(value, Ordering::Release)
        });
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let driver = runtime_host_driver::spawn(
            key.agent_type,
            key.process_fingerprint.clone(),
            key.capabilities,
            agent,
            router.clone(),
            shutdown.clone(),
            Arc::clone(&healthy),
            ready_tx,
            startup_trace,
        );
        let startup = HostStartupGuard::new(shutdown.clone(), startups.register(driver));
        let ready = match startup::await_host_ready(
            ready_rx,
            host_ready_timeout(key.agent_type),
            startup_cancel,
        )
        .await
        {
            Ok(ready) => ready,
            Err(error) => {
                startup.cancel_and_reap().await;
                return Err(error);
            }
        };
        let driver = startup.into_host();
        Ok(Arc::new(Self {
            key,
            connection: ready.connection,
            initialize_response: ready.initialize_response,
            stderr_tail,
            router,
            shutdown,
            healthy,
            pid,
            driver,
            created_at: Instant::now(),
            active_routes: AtomicUsize::new(0),
            reservations: AtomicUsize::new(1),
            route_epoch: AtomicU64::new(0),
            route_guard: StdMutex::new(()),
            idle_retirement: StdMutex::new(None),
            _policy_monitors: policy_monitors,
        }))
    }

    pub(crate) fn connection(&self) -> ConnectionTo<Agent> {
        self.connection.clone()
    }

    pub(crate) fn initialize_response(&self) -> InitializeResponse {
        self.initialize_response.clone()
    }

    pub(crate) fn capabilities(&self) -> RuntimeHostCapabilities {
        self.key.capabilities
    }

    pub(crate) fn runtime_verified(&self) -> bool {
        self.capabilities().runtime_verified()
    }

    pub(crate) fn stderr_tail(&self) -> Arc<StderrTail> {
        Arc::clone(&self.stderr_tail)
    }

    pub(crate) fn pid(&self) -> Option<u32> {
        match self.pid.load(Ordering::Acquire) {
            0 => None,
            pid => Some(pid),
        }
    }

    fn register_reserved_route(
        self: &Arc<Self>,
        connection_id: String,
        session_id: Option<String>,
        route: RuntimeSessionRoute,
        schedule_idle: bool,
    ) -> Result<RuntimeHostRouteLease, AcpError> {
        let _guard = self
            .route_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.is_healthy() {
            return Err(AcpError::protocol(
                "ACP runtime Host retired before route registration",
            ));
        }
        let reservation_was_available = self
            .reservations
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_sub(1)
            })
            .is_ok();
        if !reservation_was_available {
            return Err(AcpError::protocol(
                "ACP runtime Host route reservation was already consumed",
            ));
        }
        self.cancel_idle_retirement();
        self.active_routes.fetch_add(1, Ordering::AcqRel);
        self.route_epoch.fetch_add(1, Ordering::AcqRel);
        let weak = Arc::downgrade(self);
        Ok(self
            .router
            .register(connection_id, session_id, route)
            .with_on_drop(move || Self::route_released(weak, schedule_idle)))
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
    }

    pub(super) fn active_route_count(&self) -> usize {
        self.active_routes.load(Ordering::Acquire)
    }

    pub(super) fn reserve_route(&self) -> bool {
        let _guard = self
            .route_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.is_healthy() {
            return false;
        }
        self.cancel_idle_retirement();
        self.reservations.fetch_add(1, Ordering::AcqRel);
        self.route_epoch.fetch_add(1, Ordering::AcqRel);
        true
    }

    pub(super) fn has_live_routes(&self) -> bool {
        let _guard = self
            .route_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.active_route_count() > 0 || self.reservations.load(Ordering::Acquire) > 0
    }

    pub(super) fn cancel_idle_retirement(&self) {
        let mut task = self
            .idle_retirement
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(task) = task.take() {
            task.abort();
        }
    }

    pub(super) fn created_at(&self) -> Instant {
        self.created_at
    }

    pub(super) fn mark_published(&self) {
        self.driver.mark_published();
    }
}

async fn start_policy_monitors(
    agent_type: crate::models::agent::AgentType,
    capabilities: RuntimeHostCapabilities,
    shutdown: &CancellationToken,
) -> Result<Vec<CapabilityRevocationMonitor>, AcpError> {
    let enforcer = runtime_enforcer().map_err(AcpError::from_capability_error)?;
    let mut monitored = vec![Capability::AgentLaunch];
    monitored.extend(
        [
            Capability::HostExecution,
            Capability::HostRead,
            Capability::HostWrite,
            Capability::Terminal,
        ]
        .into_iter()
        .filter(|capability| capabilities.contains(*capability)),
    );
    let mut monitors = Vec::with_capacity(monitored.len());
    for capability in monitored {
        let monitor = enforcer
            .monitor_existing_agent(agent_type, capability, true, Some(shutdown.clone()))
            .await
            .map_err(AcpError::from_capability_error)?;
        monitors.push(monitor);
    }
    Ok(monitors)
}

impl Drop for AgentRuntimeHost {
    fn drop(&mut self) {
        self.cancel_idle_retirement();
        self.shutdown.cancel();
        self.driver.shutdown_in_background(HOST_SHUTDOWN_TIMEOUT);
    }
}

pub(super) struct HostReady {
    pub(super) connection: ConnectionTo<Agent>,
    pub(super) initialize_response: InitializeResponse,
}
