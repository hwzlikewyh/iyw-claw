use std::collections::HashSet;
use std::sync::{atomic::Ordering, Arc};

use super::{RuntimeHostKey, RuntimeHostRegistry};
use crate::acp::runtime_host::AgentRuntimeHost;

impl RuntimeHostRegistry {
    pub(super) async fn take_prunable_hosts(
        &self,
        preserve: Option<&RuntimeHostKey>,
        limit: usize,
    ) -> Vec<(RuntimeHostKey, Arc<AgentRuntimeHost>)> {
        // 与启动锁的注册互斥，避免星河等待接管时被当成无人使用的预热实例回收。
        let spawning = self.spawn_locks.lock().await;
        let mut hosts = self.hosts.lock().await;
        let mut keys = hosts
            .iter()
            .filter(|(_, host)| !host.is_healthy())
            .map(|(key, _)| key.clone())
            .collect::<HashSet<_>>();
        let mut idle = hosts
            .iter()
            .filter(|(key, host)| {
                preserve != Some(*key)
                    && !host.has_live_routes()
                    && !spawning
                        .get(*key)
                        .is_some_and(|entry| entry.users.load(Ordering::Acquire) > 0)
            })
            .map(|(key, host)| (host.created_at(), key.clone()))
            .collect::<Vec<_>>();
        idle.sort_by_key(|(created_at, _)| *created_at);
        let preserved_idle =
            preserve.is_some_and(|key| hosts.get(key).is_some_and(|host| !host.has_live_routes()));
        let keep_idle = limit.saturating_sub(usize::from(preserved_idle));
        let excess = idle.len().saturating_sub(keep_idle);
        keys.extend(idle.into_iter().take(excess).map(|(_, key)| key));
        keys.into_iter()
            .filter_map(|key| hosts.remove(&key).map(|host| (key, host)))
            .collect()
    }
}
