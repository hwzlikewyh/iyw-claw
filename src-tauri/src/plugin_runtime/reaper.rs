use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::supervisor::SupervisorInner;

const IDLE_TTL: Duration = Duration::from_secs(5 * 60);
const REAP_INTERVAL: Duration = Duration::from_secs(30);

pub(super) fn spawn(inner: &Arc<SupervisorInner>) {
    let weak = Arc::downgrade(inner);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REAP_INTERVAL);
        loop {
            interval.tick().await;
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if inner.shutdown.is_cancelled() {
                return;
            }
            reap_idle(&inner).await;
        }
    });
}

async fn reap_idle(inner: &Arc<SupervisorInner>) {
    let slots = inner
        .slots
        .lock()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for slot in slots {
        let instance = {
            let mut state = slot.lock().await;
            let idle = state.instance.as_ref().is_some_and(|instance| {
                instance.active_leases.load(Ordering::Acquire) == 0
                    && instance
                        .last_used
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .elapsed()
                        >= IDLE_TTL
            });
            idle.then(|| state.instance.take()).flatten()
        };
        if let Some(instance) = instance {
            instance.client.shutdown().await;
        }
    }
}
