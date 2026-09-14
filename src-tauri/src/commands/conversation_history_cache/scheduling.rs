use std::sync::{Arc, OnceLock};

use tokio::sync::Semaphore;

use crate::models::DbConversationDetail;

const MAX_PENDING_WRITES: usize = 2;

pub(crate) fn schedule(conversation_id: i32, revision: String, detail: DbConversationDetail) {
    static WRITES: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let writes = WRITES.get_or_init(|| Arc::new(Semaphore::new(MAX_PENDING_WRITES)));
    let Ok(permit) = Arc::clone(writes).try_acquire_owned() else {
        // 分页缓存可从原始记录重建，不能让等待写锁的完整历史无界驻留。
        tracing::debug!(
            conversation_id,
            "[conversation-history] cache writer busy; rebuild deferred"
        );
        return;
    };
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        super::store(conversation_id, revision, detail);
    });
}
