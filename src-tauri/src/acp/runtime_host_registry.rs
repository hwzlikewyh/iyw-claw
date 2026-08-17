use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use sacp_tokio::AcpAgent;
use tokio::sync::Mutex;

use crate::acp::error::AcpError;
use crate::acp::runtime_host::AgentRuntimeHost;
use crate::models::agent::AgentType;

const MAX_WARM_IDLE_HOSTS: usize = 2;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct RuntimeHostKey {
    pub(super) agent_type: AgentType,
    pub(super) process_fingerprint: String,
}

impl RuntimeHostKey {
    pub(crate) fn new(agent_type: AgentType, process_fingerprint: String) -> Self {
        Self {
            agent_type,
            process_fingerprint,
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
        self.entry.users.fetch_sub(1, Ordering::AcqRel);
    }
}

impl RuntimeHostRegistry {
    pub(crate) async fn acquire(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
    ) -> Result<Arc<AgentRuntimeHost>, AcpError> {
        self.acquire_inner(key, agent, None).await
    }

    pub(crate) async fn acquire_traced(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        trace: crate::acp::startup_trace::StartupTrace,
    ) -> Result<Arc<AgentRuntimeHost>, AcpError> {
        self.acquire_inner(key, agent, Some(trace)).await
    }

    async fn acquire_inner(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        trace: Option<crate::acp::startup_trace::StartupTrace>,
    ) -> Result<Arc<AgentRuntimeHost>, AcpError> {
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
        self.spawn_host(key, agent, trace).await
    }

    async fn spawn_host(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        trace: Option<crate::acp::startup_trace::StartupTrace>,
    ) -> Result<Arc<AgentRuntimeHost>, AcpError> {
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
        let host = AgentRuntimeHost::start(key.clone(), agent, trace).await?;
        self.hosts
            .lock()
            .await
            .insert(key.clone(), Arc::clone(&host));
        self.prune_hosts(Some(&key)).await;
        host.log_acquired("spawned", wait_started.elapsed());
        Ok(host)
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

    async fn ready_host(&self, key: &RuntimeHostKey) -> Option<Arc<AgentRuntimeHost>> {
        let mut hosts = self.hosts.lock().await;
        match hosts.get(key) {
            Some(host) if host.is_healthy() && host.reserve_route() => Some(Arc::clone(host)),
            Some(_) => {
                hosts.remove(key);
                None
            }
            None => None,
        }
    }

    async fn prune_hosts(&self, preserve: Option<&RuntimeHostKey>) {
        let removed = self.take_prunable_hosts(preserve).await;
        if removed.is_empty() {
            self.prune_orphan_spawn_locks(preserve).await;
            return;
        }
        let keys = removed.iter().map(|(key, _)| key).collect::<HashSet<_>>();
        self.spawn_locks
            .lock()
            .await
            .retain(|key, entry| !keys.contains(key) || entry.users.load(Ordering::Acquire) != 0);
        for (_, host) in removed {
            tokio::spawn(async move { host.shutdown().await });
        }
        self.prune_orphan_spawn_locks(preserve).await;
    }

    async fn prune_orphan_spawn_locks(&self, preserve: Option<&RuntimeHostKey>) {
        let live_keys = self
            .hosts
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<HashSet<_>>();
        self.spawn_locks.lock().await.retain(|key, entry| {
            preserve == Some(key)
                || live_keys.contains(key)
                || entry.users.load(Ordering::Acquire) != 0
        });
    }

    async fn take_prunable_hosts(
        &self,
        preserve: Option<&RuntimeHostKey>,
    ) -> Vec<(RuntimeHostKey, Arc<AgentRuntimeHost>)> {
        let mut hosts = self.hosts.lock().await;
        let mut keys = hosts
            .iter()
            .filter(|(_, host)| !host.is_healthy())
            .map(|(key, _)| key.clone())
            .collect::<HashSet<_>>();
        let mut idle = hosts
            .iter()
            .filter(|(key, host)| preserve != Some(*key) && !host.has_live_routes())
            .map(|(key, host)| (host.created_at(), key.clone()))
            .collect::<Vec<_>>();
        idle.sort_by_key(|(created_at, _)| *created_at);
        let preserved_idle =
            preserve.is_some_and(|key| hosts.get(key).is_some_and(|host| !host.has_live_routes()));
        let keep_idle = MAX_WARM_IDLE_HOSTS.saturating_sub(usize::from(preserved_idle));
        let excess = idle.len().saturating_sub(keep_idle);
        keys.extend(idle.into_iter().take(excess).map(|(_, key)| key));
        keys.into_iter()
            .filter_map(|key| hosts.remove(&key).map(|host| (key, host)))
            .collect()
    }

    pub(crate) async fn shutdown_all(&self) -> usize {
        let hosts = self
            .hosts
            .lock()
            .await
            .drain()
            .map(|(_, host)| host)
            .collect::<Vec<_>>();
        self.spawn_locks.lock().await.clear();
        let count = hosts.len();
        for host in hosts {
            host.shutdown().await;
        }
        tracing::info!(count, "[ACP][host] shared runtime hosts stopped");
        count
    }
}
