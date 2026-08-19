use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use futures::future::join_all;
use tokio::sync::Mutex;
use tokio::task::{JoinError, JoinHandle};

use crate::acp::runtime_host::{AgentRuntimeHost, RuntimeHostReservation};
use crate::acp::runtime_host_registry::startup::HostDriverShutdownReport;

use super::{RuntimeHostKey, RuntimeHostRegistry, RuntimeHostShutdownReport, MAX_WARM_IDLE_HOSTS};

const RETIREMENT_PENDING: u8 = 0;
const RETIREMENT_COMPLETED: u8 = 1;
const RETIREMENT_FAILED: u8 = 2;

#[derive(Default)]
pub(super) struct HostRetirements {
    tasks: StdMutex<Vec<Arc<RetiringHost>>>,
    unclean: Arc<AtomicBool>,
}

struct RetiringHost {
    state: AtomicU8,
    join: Mutex<Option<JoinHandle<HostDriverShutdownReport>>>,
    unclean: Arc<AtomicBool>,
}

pub(super) struct HostRetirementShutdownReport {
    pub(super) tracked: usize,
    pub(super) reaped: bool,
    pub(super) clean: bool,
}

struct RetiringHostReport {
    reaped: bool,
    clean: bool,
}

impl RetiringHost {
    fn new(host: Arc<AgentRuntimeHost>, unclean: Arc<AtomicBool>) -> Arc<Self> {
        let join = tokio::spawn(async move { host.shutdown().await });
        Arc::new(Self {
            state: AtomicU8::new(RETIREMENT_PENDING),
            join: Mutex::new(Some(join)),
            unclean,
        })
    }

    fn reap_in_background(self: &Arc<Self>) {
        let task = Arc::clone(self);
        tokio::spawn(async move {
            let _ = task.reap().await;
        });
    }

    async fn reap(&self) -> RetiringHostReport {
        let mut join = self.join.lock().await;
        let Some(task) = join.as_mut() else {
            return self.stored_report();
        };
        // Keep the handle registered across await for cancellation-safe retry.
        let result = (&mut *task).await;
        *join = None;
        self.record_result(result)
    }

    fn stored_report(&self) -> RetiringHostReport {
        let state = self.state.load(Ordering::Acquire);
        RetiringHostReport {
            reaped: state != RETIREMENT_PENDING,
            clean: state == RETIREMENT_COMPLETED,
        }
    }

    fn record_result(
        &self,
        result: Result<HostDriverShutdownReport, JoinError>,
    ) -> RetiringHostReport {
        let clean = match result {
            Ok(report) => report.reaped && report.clean,
            Err(error) => {
                tracing::error!(error = %error, "[ACP][host] retired runtime Host task failed");
                false
            }
        };
        if !clean {
            self.unclean.store(true, Ordering::Release);
        }
        self.state.store(
            if clean {
                RETIREMENT_COMPLETED
            } else {
                RETIREMENT_FAILED
            },
            Ordering::Release,
        );
        RetiringHostReport {
            reaped: true,
            clean,
        }
    }

    fn requires_tracking(&self) -> bool {
        self.state.load(Ordering::Acquire) == RETIREMENT_PENDING
    }
}

impl HostRetirements {
    pub(super) fn retire(&self, hosts: Vec<Arc<AgentRuntimeHost>>) {
        let added = hosts
            .into_iter()
            .map(|host| RetiringHost::new(host, Arc::clone(&self.unclean)))
            .collect::<Vec<_>>();
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        tasks.retain(|task| task.requires_tracking());
        tasks.extend(added.iter().cloned());
        drop(tasks);
        for task in added {
            task.reap_in_background();
        }
    }

    pub(super) async fn reap_all(&self) -> HostRetirementShutdownReport {
        let tasks = {
            let mut registered = self
                .tasks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            registered.retain(|task| task.requires_tracking());
            registered.clone()
        };
        let tracked = tasks.len();
        let results = join_all(tasks.iter().map(|task| task.reap())).await;
        let mut registered = self
            .tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registered.retain(|task| task.requires_tracking());
        let background_unclean = self.unclean.swap(false, Ordering::AcqRel);
        HostRetirementShutdownReport {
            tracked,
            reaped: results.iter().all(|report| report.reaped) && registered.is_empty(),
            clean: results.iter().all(|report| report.clean) && !background_unclean,
        }
    }
}

impl RuntimeHostRegistry {
    pub(super) async fn ready_host(&self, key: &RuntimeHostKey) -> Option<RuntimeHostReservation> {
        let retired = {
            let mut hosts = self.hosts.lock().await;
            match hosts.get(key) {
                Some(host) if host.is_healthy() && host.reserve_route() => {
                    return Some(RuntimeHostReservation::new_shared(Arc::clone(host)));
                }
                Some(_) => hosts.remove(key),
                None => None,
            }
        };
        if let Some(host) = retired {
            self.retirements.retire(vec![host]);
        }
        None
    }

    pub(super) async fn prune_hosts(&self, preserve: Option<&RuntimeHostKey>) {
        let removed = self.take_prunable_hosts(preserve).await;
        if removed.is_empty() {
            self.prune_orphan_spawn_locks(preserve).await;
            return;
        }
        let keys = removed.iter().map(|(key, _)| key).collect::<HashSet<_>>();
        self.retirements
            .retire(removed.into_iter().map(|(_, host)| host).collect());
        self.spawn_locks
            .lock()
            .await
            .retain(|key, entry| !keys.contains(key) || entry.users.load(Ordering::Acquire) != 0);
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

    pub(crate) async fn shutdown_all(&self) -> RuntimeHostShutdownReport {
        self.closed.store(true, Ordering::Release);
        self.shutdown.cancel();
        let _lifecycle = self.lifecycle.write().await;
        let startup_report = self.startups.reap_all().await;
        // Retain Hosts until their driver handle is reaped. Cancellation keeps
        // the handle retryable; an unclean terminal outcome must not retain it.
        let hosts = self
            .hosts
            .lock()
            .await
            .iter()
            .map(|(key, host)| (key.clone(), Arc::clone(host)))
            .collect::<Vec<_>>();
        let count = hosts.len();
        let host_results = join_all(hosts.iter().map(|(_, host)| host.shutdown())).await;
        let hosts_reaped = host_results.iter().all(|report| report.reaped);
        let hosts_clean = host_results.iter().all(|report| report.clean);
        let mut registered = self.hosts.lock().await;
        for ((key, host), report) in hosts.into_iter().zip(host_results) {
            if report.reaped
                && registered
                    .get(&key)
                    .is_some_and(|current| Arc::ptr_eq(current, &host))
            {
                registered.remove(&key);
            }
        }
        drop(registered);
        let retirement_report = self.retirements.reap_all().await;
        self.spawn_locks.lock().await.clear();
        let reaped = startup_report.reaped && hosts_reaped && retirement_report.reaped;
        let clean = startup_report.clean && hosts_clean && retirement_report.clean;
        let completed = reaped && clean;
        tracing::info!(
            count,
            retired = retirement_report.tracked,
            startup_tasks_reaped = startup_report.tracked,
            reaped,
            clean,
            completed,
            "[ACP][host] shared runtime hosts stopped"
        );
        RuntimeHostShutdownReport {
            stopped_hosts: count,
            startup_tasks_reaped: startup_report.tracked,
            completed,
        }
    }
}
