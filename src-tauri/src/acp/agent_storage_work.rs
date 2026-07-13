use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

static ACTIVE_AGENT_STORAGE_WORK: AtomicUsize = AtomicUsize::new(0);

static STORAGE_WORK_LOCK: OnceLock<Arc<RwLock<()>>> = OnceLock::new();

pub struct AgentStorageWorkGuard {
    _guard: OwnedRwLockReadGuard<()>,
}

pub struct AgentStorageMigrationGuard {
    _guard: OwnedRwLockWriteGuard<()>,
}

impl Drop for AgentStorageWorkGuard {
    fn drop(&mut self) {
        ACTIVE_AGENT_STORAGE_WORK.fetch_sub(1, Ordering::AcqRel);
    }
}

pub async fn begin_agent_storage_work() -> AgentStorageWorkGuard {
    let guard = storage_work_lock().read_owned().await;
    ACTIVE_AGENT_STORAGE_WORK.fetch_add(1, Ordering::AcqRel);
    AgentStorageWorkGuard { _guard: guard }
}

pub fn has_active_agent_storage_work() -> bool {
    ACTIVE_AGENT_STORAGE_WORK.load(Ordering::Acquire) > 0
}

pub fn try_begin_agent_storage_migration() -> Option<AgentStorageMigrationGuard> {
    storage_work_lock()
        .try_write_owned()
        .ok()
        .map(|guard| AgentStorageMigrationGuard { _guard: guard })
}

fn storage_work_lock() -> Arc<RwLock<()>> {
    STORAGE_WORK_LOCK
        .get_or_init(|| Arc::new(RwLock::new(())))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn work_guard_tracks_active_storage_mutation() {
        assert!(!has_active_agent_storage_work());
        let guard = begin_agent_storage_work().await;
        assert!(has_active_agent_storage_work());
        assert!(try_begin_agent_storage_migration().is_none());
        drop(guard);
        assert!(!has_active_agent_storage_work());
        assert!(try_begin_agent_storage_migration().is_some());
    }
}
