use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::listener::DelegationListener;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const READINESS_POLL: Duration = Duration::from_millis(10);

/// Lifecycle owner for the legacy UDS/named-pipe accept loop.
pub struct DelegationListenerService {
    shutdown: CancellationToken,
    join: Mutex<Option<JoinHandle<io::Result<()>>>>,
    socket_path: PathBuf,
}

impl DelegationListenerService {
    pub async fn start(
        listener: Arc<DelegationListener>,
        socket_path: PathBuf,
    ) -> io::Result<Arc<Self>> {
        let tokens = Arc::clone(&listener.tokens);
        let shutdown = CancellationToken::new();
        let run_shutdown = shutdown.child_token();
        let run_path = socket_path.clone();
        let mut join = tokio::spawn(async move {
            let result = listener.run(run_path, run_shutdown).await;
            if let Err(error) = &result {
                tracing::error!(error = %error, "[delegation] listener exited");
            }
            result
        });
        let ready = tokio::time::timeout(STARTUP_TIMEOUT, async {
            while !tokens.listener_ready() && !join.is_finished() {
                tokio::time::sleep(READINESS_POLL).await;
            }
            tokens.listener_ready()
        })
        .await
        .unwrap_or(false);
        if !ready {
            shutdown.cancel();
            let outcome = reap_startup_task(&mut join).await;
            if let Err(error) = cleanup_socket(&socket_path).await {
                tracing::warn!(
                    path = %socket_path.display(),
                    error = %error,
                    "failed to clean up delegation socket after startup failure"
                );
            }
            return Err(startup_error(outcome));
        }
        Ok(Arc::new(Self {
            shutdown,
            join: Mutex::new(Some(join)),
            socket_path,
        }))
    }

    pub fn quiesce(&self) {
        self.shutdown.cancel();
    }

    pub async fn shutdown(&self) -> bool {
        self.quiesce();
        // Retain ownership of the task until it is reaped. If a process-level
        // timeout cancels this future while the listener is stopping, a later
        // forced shutdown must still be able to await or abort the same task.
        let task_completed = {
            let mut join_slot = self.join.lock().await;
            if let Some(join) = join_slot.as_mut() {
                let completed = reap_listener_task(join).await;
                *join_slot = None;
                completed
            } else {
                true
            }
        };
        let socket_completed = match cleanup_socket(&self.socket_path).await {
            Ok(()) => true,
            Err(error) => {
                tracing::error!(
                    path = %self.socket_path.display(),
                    error = %error,
                    "failed to clean up delegation socket"
                );
                false
            }
        };
        task_completed && socket_completed
    }
}

async fn reap_startup_task(
    join: &mut JoinHandle<io::Result<()>>,
) -> Result<io::Result<()>, tokio::task::JoinError> {
    match tokio::time::timeout(SHUTDOWN_TIMEOUT, &mut *join).await {
        Ok(outcome) => outcome,
        Err(_) => {
            tracing::warn!(
                timeout_ms = SHUTDOWN_TIMEOUT.as_millis(),
                "delegation listener startup shutdown timed out; aborting task"
            );
            join.abort();
            (&mut *join).await
        }
    }
}

async fn reap_listener_task(join: &mut JoinHandle<io::Result<()>>) -> bool {
    match tokio::time::timeout(SHUTDOWN_TIMEOUT, &mut *join).await {
        Ok(Ok(Ok(()))) => true,
        Ok(Ok(Err(error))) => {
            tracing::error!(error = %error, "delegation listener exited with an error");
            false
        }
        Ok(Err(error)) => {
            tracing::error!(error = %error, "delegation listener task failed");
            false
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = SHUTDOWN_TIMEOUT.as_millis(),
                "delegation listener shutdown timed out; aborting task"
            );
            join.abort();
            match (&mut *join).await {
                Ok(Ok(())) => true,
                Ok(Err(error)) => {
                    tracing::error!(error = %error, "aborted delegation listener returned an error");
                    false
                }
                Err(error) if error.is_cancelled() => true,
                Err(error) => {
                    tracing::error!(error = %error, "aborted delegation listener task failed");
                    false
                }
            }
        }
    }
}

fn startup_error(outcome: Result<io::Result<()>, tokio::task::JoinError>) -> io::Error {
    match outcome {
        Ok(Err(error)) => error,
        Ok(Ok(())) => io::Error::new(
            io::ErrorKind::NotConnected,
            "delegation listener stopped before becoming ready",
        ),
        Err(error) => io::Error::other(format!("delegation listener task failed: {error}")),
    }
}

#[cfg(unix)]
async fn cleanup_socket(path: &Path) -> io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
async fn cleanup_socket(_path: &Path) -> io::Result<()> {
    Ok(())
}
