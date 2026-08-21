use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::manager::ChatChannelManager;
use super::types::ChannelMessageTarget;

const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct TypingKey {
    channel_id: i32,
    route_key: String,
}

struct TypingLease {
    generation: u64,
    owner_connection_id: String,
    target: ChannelMessageTarget,
    paused: bool,
    refresh_cancel: CancellationToken,
}

#[derive(Clone, Default)]
pub struct TypingController {
    next_generation: Arc<AtomicU64>,
    leases: Arc<Mutex<HashMap<TypingKey, TypingLease>>>,
    updates: Arc<Mutex<()>>,
}

impl TypingController {
    pub async fn start(
        &self,
        manager: ChatChannelManager,
        target: ChannelMessageTarget,
        route_key: String,
        owner_connection_id: String,
    ) {
        let key = TypingKey {
            channel_id: target.channel_id,
            route_key,
        };
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = CancellationToken::new();
        let old_cancel = {
            let mut leases = self.leases.lock().await;
            let old_cancel = leases.get(&key).map(|lease| lease.refresh_cancel.clone());
            leases.insert(
                key.clone(),
                TypingLease {
                    generation,
                    owner_connection_id,
                    target: target.clone(),
                    paused: false,
                    refresh_cancel: cancel.clone(),
                },
            );
            old_cancel
        };
        if let Some(old_cancel) = old_cancel {
            old_cancel.cancel();
        }

        let controller = self.clone();
        tokio::spawn(async move {
            controller
                .run(manager, key, generation, target, cancel, "start")
                .await;
        });
    }

    pub async fn pause(
        &self,
        manager: &ChatChannelManager,
        channel_id: i32,
        route_key: &str,
        owner_connection_id: &str,
    ) {
        let key = key(channel_id, route_key);
        let (generation, target, refresh_cancel) = {
            let mut leases = self.leases.lock().await;
            let Some(lease) = leases.get_mut(&key) else {
                return;
            };
            if lease.owner_connection_id != owner_connection_id {
                return;
            }
            if lease.paused {
                return;
            }
            lease.paused = true;
            (
                lease.generation,
                lease.target.clone(),
                lease.refresh_cancel.clone(),
            )
        };
        refresh_cancel.cancel();
        self.set_paused_status(manager, &key, generation, &target)
            .await;
    }

    pub async fn resume(
        &self,
        manager: &ChatChannelManager,
        channel_id: i32,
        route_key: &str,
        owner_connection_id: &str,
    ) {
        let key = key(channel_id, route_key);
        let cancel = CancellationToken::new();
        let (generation, target) = {
            let mut leases = self.leases.lock().await;
            let Some(lease) = leases.get_mut(&key) else {
                return;
            };
            if lease.owner_connection_id != owner_connection_id {
                return;
            }
            if !lease.paused {
                return;
            }
            lease.paused = false;
            lease.refresh_cancel = cancel.clone();
            (lease.generation, lease.target.clone())
        };
        let controller = self.clone();
        let manager = manager.clone_ref();
        tokio::spawn(async move {
            controller
                .run(manager, key, generation, target, cancel, "resume")
                .await;
        });
    }

    pub async fn stop(
        &self,
        manager: &ChatChannelManager,
        channel_id: i32,
        route_key: &str,
        owner_connection_id: &str,
    ) {
        let key = key(channel_id, route_key);
        let lease = {
            let mut leases = self.leases.lock().await;
            if !leases
                .get(&key)
                .is_some_and(|lease| lease.owner_connection_id == owner_connection_id)
            {
                return;
            }
            leases.remove(&key)
        };
        let Some(lease) = lease else { return };
        lease.refresh_cancel.cancel();
        self.set_stopped_status(manager, &key, lease.generation, &lease.target, "stop")
            .await;
    }

    pub async fn stop_channel(&self, manager: &ChatChannelManager, channel_id: i32) {
        let leases = {
            let mut guard = self.leases.lock().await;
            let keys: Vec<_> = guard
                .keys()
                .filter(|key| key.channel_id == channel_id)
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| guard.remove(&key).map(|lease| (key, lease)))
                .collect::<Vec<_>>()
        };
        for (key, lease) in leases {
            lease.refresh_cancel.cancel();
            self.set_stopped_status(
                manager,
                &key,
                lease.generation,
                &lease.target,
                "channel_stop",
            )
            .await;
        }
    }

    async fn run(
        &self,
        manager: ChatChannelManager,
        key: TypingKey,
        generation: u64,
        target: ChannelMessageTarget,
        cancel: CancellationToken,
        initial_phase: &'static str,
    ) {
        self.set_active_status(&manager, &key, generation, &target, initial_phase)
            .await;
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = interval.tick() => {
                    self.set_active_status(&manager, &key, generation, &target, "refresh").await;
                }
            }
        }
    }

    async fn is_active(&self, key: &TypingKey, generation: u64) -> bool {
        self.leases
            .lock()
            .await
            .get(key)
            .is_some_and(|lease| lease.generation == generation && !lease.paused)
    }

    async fn set_active_status(
        &self,
        manager: &ChatChannelManager,
        key: &TypingKey,
        generation: u64,
        target: &ChannelMessageTarget,
        phase: &'static str,
    ) {
        let _update = self.updates.lock().await;
        if self.is_active(key, generation).await {
            self.set_status(manager, target, true, phase).await;
        }
    }

    async fn set_paused_status(
        &self,
        manager: &ChatChannelManager,
        key: &TypingKey,
        generation: u64,
        target: &ChannelMessageTarget,
    ) {
        let _update = self.updates.lock().await;
        let paused = self
            .leases
            .lock()
            .await
            .get(key)
            .is_some_and(|lease| lease.generation == generation && lease.paused);
        if paused {
            self.set_status(manager, target, false, "pause").await;
        }
    }

    async fn set_stopped_status(
        &self,
        manager: &ChatChannelManager,
        key: &TypingKey,
        generation: u64,
        target: &ChannelMessageTarget,
        phase: &'static str,
    ) {
        let _update = self.updates.lock().await;
        let replaced = self
            .leases
            .lock()
            .await
            .get(key)
            .is_some_and(|lease| lease.generation != generation);
        if !replaced {
            self.set_status(manager, target, false, phase).await;
        }
    }

    async fn set_status(
        &self,
        manager: &ChatChannelManager,
        target: &ChannelMessageTarget,
        is_typing: bool,
        phase: &'static str,
    ) {
        if let Err(error) = manager.set_typing(target, is_typing).await {
            tracing::debug!(
                channel_id = target.channel_id,
                phase,
                is_typing,
                error = %error,
                "[ChatChannel] typing update unavailable"
            );
        }
    }
}

fn key(channel_id: i32, route_key: &str) -> TypingKey {
    TypingKey {
        channel_id,
        route_key: route_key.to_string(),
    }
}
