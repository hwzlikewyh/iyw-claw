use std::sync::{atomic::Ordering, Arc};
use std::time::{Duration, Instant};

use sacp_tokio::AcpAgent;

use super::{registry_closed_error, RuntimeHostKey, RuntimeHostRegistry};
use crate::acp::error::AcpError;
use crate::acp::runtime_host::{AgentRuntimeHost, RuntimeHostReservation};
use crate::acp::startup_trace::StartupTrace;
use crate::acp::stderr_tail::StderrTail;

impl RuntimeHostRegistry {
    pub(crate) async fn prewarm(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
    ) -> Result<bool, AcpError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(registry_closed_error());
        }
        let _lifecycle = self.lifecycle.read().await;
        if self.closed.load(Ordering::Acquire) {
            return Err(registry_closed_error());
        }
        let spawn_lock = self.spawn_lock(&key).await;
        let _spawn = spawn_lock.entry.lock.lock().await;
        self.prune_hosts(Some(&key)).await;
        // 首次点击先启动成功时，稍后抵达的预热不能再启动一份闲置副本。
        if self.has_owned_host(&key).await {
            return Ok(false);
        }
        if self
            .hosts
            .lock()
            .await
            .get(&key)
            .is_some_and(|host| host.is_healthy())
        {
            return Ok(false);
        }
        let host = AgentRuntimeHost::start(
            key.clone(),
            agent,
            stderr_tail,
            self.shutdown.child_token(),
            None,
            &self.startups,
        )
        .await?;
        let reservation = RuntimeHostReservation::new(host);
        if self.closed.load(Ordering::Acquire) {
            reservation.shutdown().await;
            return Err(registry_closed_error());
        }
        // 发布和释放预热预留都在启动锁内，首次点击不会看到半完成的预热。
        self.publish_prewarm(&key, reservation).await;
        self.prune_hosts(Some(&key)).await;
        Ok(true)
    }

    async fn publish_prewarm(&self, key: &RuntimeHostKey, mut reservation: RuntimeHostReservation) {
        self.hosts
            .lock()
            .await
            .insert(key.clone(), reservation.clone_host());
        reservation.mark_published();
        reservation.keep_warm();
    }

    pub(super) async fn start_owned_inner(
        &self,
        key: RuntimeHostKey,
        agent: AcpAgent,
        stderr_tail: Arc<StderrTail>,
        trace: Option<StartupTrace>,
    ) -> Result<RuntimeHostReservation, AcpError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(registry_closed_error());
        }
        let _lifecycle = self.lifecycle.read().await;
        if self.closed.load(Ordering::Acquire) {
            return Err(registry_closed_error());
        }
        if let Some(trace) = &trace {
            trace.bind_host_key(key.fingerprint_prefix());
        }
        let started = Instant::now();
        // 与预热共用同一 key 的启动锁；预热进行中时等待该进程，不重复启动。
        let spawn_lock = self.spawn_lock(&key).await;
        let _spawn = spawn_lock.entry.lock.lock().await;
        if let Some(host) = self.take_warm_owned(&key).await {
            self.track_owned_host(&key, &host).await;
            host.log_acquired("prewarmed_owned", started.elapsed());
            if let Some(trace) = &trace {
                trace.record("initialize", "prewarmed_owned", Duration::ZERO);
            }
            return Ok(host);
        }
        let host = AgentRuntimeHost::start(
            key.clone(),
            agent,
            stderr_tail,
            self.shutdown.child_token(),
            trace,
            &self.startups,
        )
        .await?;
        let reservation = RuntimeHostReservation::new(host);
        self.track_owned_host(&key, &reservation).await;
        Ok(reservation)
    }

    async fn take_warm_owned(&self, key: &RuntimeHostKey) -> Option<RuntimeHostReservation> {
        let mut hosts = self.hosts.lock().await;
        let host = hosts.get(key)?;
        // 仅接管没有任何会话/预留的预热实例；活跃实例绝不借给另一会话。
        if host.has_live_routes() || !host.reserve_route() {
            return None;
        }
        let host = hosts.remove(key)?;
        Some(RuntimeHostReservation::new(host))
    }

    async fn track_owned_host(&self, key: &RuntimeHostKey, host: &RuntimeHostReservation) {
        let mut owned = self.owned_hosts.lock().await;
        owned.retain(|_, hosts| {
            hosts.retain(|host| host.strong_count() > 0);
            !hosts.is_empty()
        });
        owned
            .entry(key.clone())
            .or_default()
            .push(Arc::downgrade(&host.clone_host()));
    }

    async fn has_owned_host(&self, key: &RuntimeHostKey) -> bool {
        self.owned_hosts.lock().await.get(key).is_some_and(|hosts| {
            hosts
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .any(|host| host.is_healthy())
        })
    }

    pub(super) async fn owned_host_snapshots(&self) -> Vec<Arc<AgentRuntimeHost>> {
        self.owned_hosts
            .lock()
            .await
            .values()
            .flatten()
            .filter_map(std::sync::Weak::upgrade)
            .collect()
    }
}
