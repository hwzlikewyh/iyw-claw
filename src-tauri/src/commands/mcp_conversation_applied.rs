use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

static APPLIED: LazyLock<Mutex<BTreeMap<i32, u64>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub(crate) fn record(id: Option<i32>, revision: u64) {
    if let Some(id) = id {
        if let Ok(mut applied) = APPLIED.lock() {
            applied.insert(id, revision);
        }
    }
}

pub(crate) fn needs_reconnect(id: Option<i32>, revision: u64) -> bool {
    id.and_then(|id| APPLIED.lock().ok()?.get(&id).copied())
        .is_some_and(|applied| applied != revision)
}
