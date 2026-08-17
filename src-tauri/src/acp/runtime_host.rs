use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex as StdMutex,
};
use std::time::{Duration, Instant};

use sacp::schema::InitializeResponse;
use sacp::{Agent, ConnectionTo};
use sacp_tokio::AcpAgent;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::acp::error::AcpError;
use crate::acp::runtime_host_driver;
pub(crate) use crate::acp::runtime_host_registry::{RuntimeHostKey, RuntimeHostRegistry};
use crate::acp::runtime_host_router::SessionRequestRouter;
pub(crate) use crate::acp::runtime_host_router::{
    RuntimeHostRouteBinding, RuntimeHostRouteLease, RuntimeSessionRoute,
};

mod lifecycle;

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
    router: SessionRequestRouter,
    shutdown: CancellationToken,
    healthy: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    driver: Mutex<Option<tokio::task::JoinHandle<()>>>,
    created_at: Instant,
    active_routes: AtomicUsize,
    reservations: AtomicUsize,
    route_epoch: AtomicU64,
    route_guard: StdMutex<()>,
}

impl AgentRuntimeHost {
    pub(super) async fn start(
        key: RuntimeHostKey,
        agent: AcpAgent,
        startup_trace: Option<crate::acp::startup_trace::StartupTrace>,
    ) -> Result<Arc<Self>, AcpError> {
        let router = SessionRequestRouter::default();
        let shutdown = CancellationToken::new();
        let healthy = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicU32::new(0));
        let agent = agent.with_spawned_pid({
            let pid = Arc::clone(&pid);
            move |value| pid.store(value, Ordering::Release)
        });
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let driver = runtime_host_driver::spawn(
            key.agent_type,
            key.process_fingerprint.clone(),
            agent,
            router.clone(),
            shutdown.clone(),
            Arc::clone(&healthy),
            ready_tx,
            startup_trace,
        );
        let ready_timeout = host_ready_timeout(key.agent_type);
        let ready = match tokio::time::timeout(ready_timeout, ready_rx).await {
            Ok(Ok(Ok(ready))) => ready,
            Ok(Ok(Err(error))) => {
                shutdown.cancel();
                driver.abort();
                return Err(error);
            }
            Ok(Err(_)) => {
                shutdown.cancel();
                driver.abort();
                return Err(AcpError::protocol(
                    "ACP runtime Host exited before initialization",
                ));
            }
            Err(_) => {
                shutdown.cancel();
                driver.abort();
                return Err(AcpError::InitializeTimeout);
            }
        };
        Ok(Arc::new(Self {
            key,
            connection: ready.connection,
            initialize_response: ready.initialize_response,
            router,
            shutdown,
            healthy,
            pid,
            driver: Mutex::new(Some(driver)),
            created_at: Instant::now(),
            active_routes: AtomicUsize::new(0),
            reservations: AtomicUsize::new(1),
            route_epoch: AtomicU64::new(0),
            route_guard: StdMutex::new(()),
        }))
    }

    pub(crate) async fn start_owned(
        key: RuntimeHostKey,
        agent: AcpAgent,
    ) -> Result<Arc<Self>, AcpError> {
        Self::start(key, agent, None).await
    }

    pub(crate) async fn start_owned_traced(
        key: RuntimeHostKey,
        agent: AcpAgent,
        startup_trace: crate::acp::startup_trace::StartupTrace,
    ) -> Result<Arc<Self>, AcpError> {
        startup_trace.bind_host_key(key.fingerprint_prefix());
        Self::start(key, agent, Some(startup_trace)).await
    }

    pub(crate) fn connection(&self) -> ConnectionTo<Agent> {
        self.connection.clone()
    }

    pub(crate) fn initialize_response(&self) -> InitializeResponse {
        self.initialize_response.clone()
    }

    pub(crate) fn pid(&self) -> Option<u32> {
        match self.pid.load(Ordering::Acquire) {
            0 => None,
            pid => Some(pid),
        }
    }

    pub(crate) fn register_route(
        self: &Arc<Self>,
        connection_id: String,
        session_id: Option<String>,
        route: RuntimeSessionRoute,
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
        if self.reservations.fetch_sub(1, Ordering::AcqRel) == 0 {
            return Err(AcpError::protocol(
                "ACP runtime Host route reservation was already consumed",
            ));
        }
        self.active_routes.fetch_add(1, Ordering::AcqRel);
        self.route_epoch.fetch_add(1, Ordering::AcqRel);
        let weak = Arc::downgrade(self);
        Ok(self
            .router
            .register(connection_id, session_id, route)
            .with_on_drop(move || Self::route_released(weak)))
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
        self.reservations.fetch_add(1, Ordering::AcqRel);
        self.route_epoch.fetch_add(1, Ordering::AcqRel);
        true
    }

    pub(super) fn has_live_routes(&self) -> bool {
        self.active_route_count() > 0 || self.reservations.load(Ordering::Acquire) > 0
    }

    pub(super) fn created_at(&self) -> Instant {
        self.created_at
    }
}

impl Drop for AgentRuntimeHost {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

pub(super) struct HostReady {
    pub(super) connection: ConnectionTo<Agent>,
    pub(super) initialize_response: InitializeResponse,
}
