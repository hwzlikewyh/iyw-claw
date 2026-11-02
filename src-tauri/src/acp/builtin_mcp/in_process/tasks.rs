use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const MAX_CONNECTIONS_PER_PARENT: usize = 64;

#[derive(Default)]
pub(in crate::acp::builtin_mcp) struct Tasks {
    parents: StdMutex<HashMap<String, Arc<ParentTasks>>>,
    failed: AtomicBool,
}

struct ParentTasks {
    stop: CancellationToken,
    joins: Mutex<JoinSet<()>>,
}

impl Tasks {
    pub(super) async fn spawn(
        &self,
        parent: &str,
        run: impl FnOnce(CancellationToken) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> std::io::Result<()> {
        let tasks = self
            .parents
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(parent.to_string())
            .or_insert_with(|| {
                Arc::new(ParentTasks {
                    stop: CancellationToken::new(),
                    joins: Mutex::new(JoinSet::new()),
                })
            })
            .clone();
        let mut joins = tasks.joins.lock().await;
        while let Some(result) = joins.try_join_next() {
            self.record(result);
        }
        if tasks.stop.is_cancelled() || joins.len() >= MAX_CONNECTIONS_PER_PARENT {
            return Err(std::io::Error::other(
                "in-process MCP connection capacity reached",
            ));
        }
        joins.spawn(run(tasks.stop.child_token()));
        Ok(())
    }

    pub(in crate::acp::builtin_mcp) async fn close_parent(&self, parent: &str) {
        let tasks = self
            .parents
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(parent)
            .cloned();
        let Some(tasks) = tasks else { return };
        tasks.stop.cancel();
        let mut joins = tasks.joins.lock().await;
        while let Some(result) = joins.join_next().await {
            self.record(result);
        }
        let mut parents = self
            .parents
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if parents
            .get(parent)
            .is_some_and(|current| Arc::ptr_eq(current, &tasks))
        {
            parents.remove(parent);
        }
    }

    pub(in crate::acp::builtin_mcp) async fn close_all(&self) {
        let parents = {
            let parents = self
                .parents
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            for tasks in parents.values() {
                tasks.stop.cancel();
            }
            parents.keys().cloned().collect::<Vec<_>>()
        };
        for parent in parents {
            self.close_parent(&parent).await;
        }
    }

    pub(in crate::acp::builtin_mcp) fn is_clean(&self) -> bool {
        self.parents
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_empty()
            && !self.failed.load(Ordering::Acquire)
    }

    fn record(&self, result: Result<(), tokio::task::JoinError>) {
        if let Err(error) = result {
            self.failed.store(true, Ordering::Release);
            tracing::warn!(target: "builtin_mcp", %error, "in-process MCP task stopped unexpectedly");
        }
    }
}
