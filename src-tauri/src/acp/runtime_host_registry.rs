use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use sacp_tokio::AcpAgent;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::acp::error::AcpError;
use crate::acp::runtime_host::{AgentRuntimeHost, RuntimeHostReservation};
use crate::acp::runtime_host_policy::{RuntimeHostCapabilities, RuntimeHostPolicy};
use crate::acp::stderr_tail::StderrTail;
use crate::models::agent::AgentType;

mod retirement;
pub(crate) mod startup;

use retirement::HostRetirements;
use startup::HostStartups;

const MAX_WARM_IDLE_HOSTS: usize = 2;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct RuntimeHostKey {
    pub(super) agent_type: AgentType,
    pub(super) process_fingerprint: String,
    pub(super) definition_fingerprint: String,
    pub(super) runtime_version: String,
    pub(super) policy_revision: Option<u64>,
    pub(super) capabilities: RuntimeHostCapabilities,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct RuntimeHostIdentity {
    pub(crate) definition_fingerprint: String,
    pub(crate) runtime_version: String,
    pub(crate) policy: RuntimeHostPolicy,
}

impl RuntimeHostKey {
    pub(crate) fn new(
        agent_type: AgentType,
        process_fingerprint: String,
        identity: RuntimeHostIdentity,
    ) -> Self {
        Self {
            agent_type,
            process_fingerprint,
            definition_fingerprint: identity.definition_fingerprint,
            runtime_version: identity.runtime_version,
            policy_revision: identity.policy.revision,
            capabilities: identity.policy.capabilities,
        }
    }

    pub(crate) fn fingerprint_prefix(&self) -> &str {
        self.process_fingerprint
            .get(..12)
            .unwrap_or(&self.process_fingerprint)
    }
}

#[derive(Default)]
pub(crate) struct RuntimeHostRegistry {
    hosts: Mutex<HashMap<RuntimeHostKey, Arc<AgentRuntimeHost>>>,
    spawn_locks: Mutex<HashMap<RuntimeHostKey, Arc<SpawnLockEntry>>>,
    lifecycle: RwLock<()>,
    closed: AtomicBool,
    shutdown: CancellationToken,
    retirements: HostRetirements,
    startups: HostStartups,
}

pub(crate) struct RuntimeHostShutdownReport {
    pub(crate) stopped_hosts: usize,
    pub(crate) startup_tasks_reaped: usize,
    pub(crate) completed: bool,
}

struct SpawnLockEntry {
    lock: Mutex<()>,
    users: AtomicUsize,
}

impl SpawnLockEntry {
    fn new() -> Self {
        Self {
            lock: Mutex::new(()),
            users: AtomicUsize::new(0),
        }
    }
}

struct SpawnLockLease {
    entry: Arc<SpawnLockEntry>,
}

impl Drop for SpawnLockLease {
    fn drop(&mut self) {
        let _ = self
            .entry
            .users
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |users| {
                users.checked_sub(1)
            });
    }
}

impl RuntimeHostRegistry {
    pub(crate) async fn acquire(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
    ) -> Result<RuntimeHostReservation, AcpError> {
        self.acquire_inner(key, agent, stderr_tail, None).await
    }

    pub(crate) async fn acquire_traced(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
        trace: crate::acp::startup_trace::StartupTrace,
    ) -> Result<RuntimeHostReservation, AcpError> {
        self.acquire_inner(key, agent, stderr_tail, Some(trace))
            .await
    }

    pub(crate) async fn start_owned(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
    ) -> Result<RuntimeHostReservation, AcpError> {
        self.start_owned_inner(key, agent, stderr_tail, None).await
    }

    pub(crate) async fn start_owned_traced(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
        trace: crate::acp::startup_trace::StartupTrace,
    ) -> Result<RuntimeHostReservation, AcpError> {
        self.start_owned_inner(key, agent, stderr_tail, Some(trace))
            .await
    }

    async fn start_owned_inner(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
        trace: Option<crate::acp::startup_trace::StartupTrace>,
    ) -> Result<RuntimeHostReservation, AcpError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(registry_closed_error());
        }
        let _lifecycle = self.lifecycle.read().await;
        if self.closed.load(Ordering::Acquire) {
            return Err(registry_closed_error());
        }
        if let Some(trace) = trace.as_ref() {
            trace.bind_host_key(key.fingerprint_prefix());
        }
        let host = AgentRuntimeHost::start(
            key,
            agent,
            stderr_tail,
            self.shutdown.child_token(),
            trace,
            &self.startups,
        )
        .await?;
        Ok(RuntimeHostReservation::new(host))
    }

    async fn acquire_inner(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
        trace: Option<crate::acp::startup_trace::StartupTrace>,
    ) -> Result<RuntimeHostReservation, AcpError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(registry_closed_error());
        }
        let _lifecycle = self.lifecycle.read().await;
        if self.closed.load(Ordering::Acquire) {
            return Err(registry_closed_error());
        }
        self.prune_hosts(Some(&key)).await;
        if let Some(trace) = trace.as_ref() {
            trace.bind_host_key(key.fingerprint_prefix());
        }
        if let Some(host) = self.ready_host(&key).await {
            host.log_acquired("hit", Duration::ZERO);
            if let Some(trace) = trace.as_ref() {
                trace.record("initialize", "reused", Duration::ZERO);
            }
            return Ok(host);
        }
        self.spawn_host(key, agent, stderr_tail, trace).await
    }

    async fn spawn_host(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
        trace: Option<crate::acp::startup_trace::StartupTrace>,
    ) -> Result<RuntimeHostReservation, AcpError> {
        let wait_started = Instant::now();
        let spawn_lock = self.spawn_lock(&key).await;
        let _spawn_guard = spawn_lock.entry.lock.lock().await;
        if let Some(host) = self.ready_host(&key).await {
            host.log_acquired("waited", wait_started.elapsed());
            if let Some(trace) = trace.as_ref() {
                trace.record("initialize", "shared_wait", wait_started.elapsed());
            }
            return Ok(host);
        }
        let cancel = self.shutdown.child_token();
        let host = AgentRuntimeHost::start(
            key.clone(),
            agent,
            stderr_tail,
            cancel,
            trace,
            &self.startups,
        )
        .await?;
        let mut reservation = RuntimeHostReservation::new(host);
        let mut hosts = self.hosts.lock().await;
        if self.closed.load(Ordering::Acquire) {
            drop(hosts);
            reservation.shutdown().await;
            return Err(registry_closed_error());
        }
        hosts.insert(key.clone(), reservation.clone_host());
        // Do not await between durable registration and releasing startup tracking.
        reservation.mark_published();
        drop(hosts);
        self.prune_hosts(Some(&key)).await;
        reservation.log_acquired("spawned", wait_started.elapsed());
        Ok(reservation)
    }

    async fn spawn_lock(&self, key: &RuntimeHostKey) -> SpawnLockLease {
        let mut locks = self.spawn_locks.lock().await;
        let entry = locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(SpawnLockEntry::new()))
            .clone();
        entry.users.fetch_add(1, Ordering::AcqRel);
        SpawnLockLease { entry }
    }
}

fn registry_closed_error() -> AcpError {
    AcpError::protocol("ACP runtime Host registry is shutting down")
}
