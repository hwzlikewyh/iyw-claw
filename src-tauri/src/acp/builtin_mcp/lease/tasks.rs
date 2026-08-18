use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use futures_util::FutureExt;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::task::JoinSet;

#[derive(Default)]
pub(super) struct ParentRevocationTasks {
    tasks: Mutex<JoinSet<()>>,
}

#[derive(Default)]
pub(super) struct CleanupTasks {
    active: AtomicUsize,
    failed: AtomicBool,
    changed: Notify,
}

impl ParentRevocationTasks {
    pub(super) async fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        let mut tasks = self.tasks.lock().await;
        reap_ready(&mut tasks);
        tasks.spawn(task);
    }

    pub(super) async fn abort_all(&self) {
        self.tasks.lock().await.abort_all();
    }

    pub(super) async fn reap_all(&self) -> bool {
        let mut tasks = self.tasks.lock().await;
        let mut complete = true;
        while let Some(result) = tasks.join_next().await {
            complete &= record_result(result);
        }
        complete
    }
}

impl CleanupTasks {
    pub(super) fn spawn(self: &Arc<Self>, task: impl Future<Output = ()> + Send + 'static) {
        self.active.fetch_add(1, Ordering::AcqRel);
        let tasks = Arc::clone(self);
        tokio::spawn(async move {
            let _guard = CleanupTaskGuard(Arc::clone(&tasks));
            if AssertUnwindSafe(task).catch_unwind().await.is_err() {
                tasks.failed.store(true, Ordering::Release);
                tracing::error!(
                    target: "builtin_mcp",
                    "HTTP MCP durable cleanup worker panicked"
                );
            }
        });
    }

    pub(super) async fn wait_idle(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.active.load(Ordering::Acquire) == 0 {
                return;
            }
            changed.await;
        }
    }

    pub(super) fn is_idle(&self) -> bool {
        self.active.load(Ordering::Acquire) == 0
    }

    pub(super) fn take_failed(&self) -> bool {
        self.failed.swap(false, Ordering::AcqRel)
    }
}

struct CleanupTaskGuard(Arc<CleanupTasks>);

impl Drop for CleanupTaskGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
        self.0.changed.notify_waiters();
        self.0.changed.notify_one();
    }
}

fn reap_ready(tasks: &mut JoinSet<()>) {
    while let Some(result) = tasks.try_join_next() {
        record_result(result);
    }
}

fn record_result(result: Result<(), tokio::task::JoinError>) -> bool {
    match result {
        Ok(()) => true,
        Err(error) if error.is_cancelled() => true,
        Err(error) => {
            tracing::error!(
                target: "builtin_mcp",
                error = %error,
                "HTTP MCP parent revocation task failed"
            );
            false
        }
    }
}
