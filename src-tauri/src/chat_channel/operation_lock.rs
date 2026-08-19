use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};

use tokio::sync::{Mutex, OwnedMutexGuard};

type ChannelLock = Mutex<()>;
type LockRegistry = StdMutex<HashMap<i32, Weak<ChannelLock>>>;

static CHANNEL_LOCKS: OnceLock<LockRegistry> = OnceLock::new();

fn registry() -> &'static LockRegistry {
    CHANNEL_LOCKS.get_or_init(|| StdMutex::new(HashMap::new()))
}

pub(crate) async fn lock_channel(channel_id: i32) -> OwnedMutexGuard<()> {
    let lock = {
        let mut registry = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = registry.get(&channel_id).and_then(Weak::upgrade) {
            lock
        } else {
            let lock = Arc::new(ChannelLock::new(()));
            registry.insert(channel_id, Arc::downgrade(&lock));
            lock
        }
    };
    lock.lock_owned().await
}
