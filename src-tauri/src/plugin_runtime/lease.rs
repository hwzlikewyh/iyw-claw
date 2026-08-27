use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::supervisor::RuntimeInstance;
use super::types::PluginInvokeError;

const PERMIT_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct CallPermits {
    _global: OwnedSemaphorePermit,
    _local: OwnedSemaphorePermit,
    instance: Arc<RuntimeInstance>,
}

pub(super) async fn acquire(
    global: &Arc<Semaphore>,
    instance: &Arc<RuntimeInstance>,
) -> Result<CallPermits, PluginInvokeError> {
    let global = acquire_one(global).await?;
    let local = acquire_one(&instance.calls).await?;
    instance.active_leases.fetch_add(1, Ordering::AcqRel);
    Ok(CallPermits {
        _global: global,
        _local: local,
        instance: instance.clone(),
    })
}

async fn acquire_one(
    semaphore: &Arc<Semaphore>,
) -> Result<OwnedSemaphorePermit, PluginInvokeError> {
    tokio::time::timeout(PERMIT_TIMEOUT, semaphore.clone().acquire_owned())
        .await
        .map_err(|_| {
            PluginInvokeError::before_effect("plugin_runtime_busy", "Plugin concurrency limit")
        })?
        .map_err(|_| {
            PluginInvokeError::before_effect("plugin_runtime_unavailable", "Runtime is closed")
        })
}

impl Drop for CallPermits {
    fn drop(&mut self) {
        self.instance.active_leases.fetch_sub(1, Ordering::AcqRel);
        *self
            .instance
            .last_used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Instant::now();
    }
}
