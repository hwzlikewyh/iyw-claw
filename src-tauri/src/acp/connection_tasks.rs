use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::{Mutex, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};
use tokio::task::JoinHandle;

use super::error::AcpError;

pub(crate) struct ConnectionTaskRegistry {
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
    lifecycle: Arc<RwLock<()>>,
    closed: AtomicBool,
}

pub(crate) struct SpawnGate {
    _guard: OwnedRwLockReadGuard<()>,
}

pub(crate) struct ShutdownGate {
    _guard: OwnedRwLockWriteGuard<()>,
}

impl Default for ConnectionTaskRegistry {
    fn default() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            lifecycle: Arc::new(RwLock::new(())),
            closed: AtomicBool::new(false),
        }
    }
}

impl ConnectionTaskRegistry {
    pub(crate) async fn begin_spawn(&self) -> Result<SpawnGate, AcpError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(shutdown_error());
        }
        let guard = Arc::clone(&self.lifecycle).read_owned().await;
        if self.closed.load(Ordering::Acquire) {
            return Err(shutdown_error());
        }
        Ok(SpawnGate { _guard: guard })
    }

    pub(crate) async fn begin_shutdown(&self) -> ShutdownGate {
        self.closed.store(true, Ordering::Release);
        let guard = Arc::clone(&self.lifecycle).write_owned().await;
        ShutdownGate { _guard: guard }
    }

    pub(crate) async fn register(&self, connection_id: String, task: JoinHandle<()>) {
        self.reap_finished().await;
        let mut tasks = self.tasks.lock().await;
        if tasks.insert(connection_id.clone(), task).is_some() {
            tracing::error!(
                connection_id,
                "[ACP] duplicate connection task registration"
            );
        }
    }

    pub(crate) async fn reap_finished(&self) {
        let finished = {
            let mut tasks = self.tasks.lock().await;
            let ids = tasks
                .iter()
                .filter(|(_, task)| task.is_finished())
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| tasks.remove(&id))
                .collect::<Vec<_>>()
        };
        for task in finished {
            let _ = task.await;
        }
    }

    pub(crate) async fn ids(&self) -> Vec<String> {
        self.tasks.lock().await.keys().cloned().collect()
    }

    pub(crate) async fn abort_and_reap_all(&self) -> bool {
        let mut tasks = self.tasks.lock().await;
        let mut complete = true;
        while let Some(connection_id) = tasks.keys().next().cloned() {
            let Some(task) = tasks.get_mut(&connection_id) else {
                continue;
            };
            task.abort();
            let result = (&mut *task).await;
            tasks.remove(&connection_id);
            if let Err(error) = result {
                if !error.is_cancelled() {
                    complete = false;
                    tracing::error!(
                        connection_id,
                        error = %error,
                        "[ACP] connection task failed during shutdown"
                    );
                }
            }
        }
        complete
    }
}

fn shutdown_error() -> AcpError {
    AcpError::protocol("ACP connection manager is shutting down")
}
