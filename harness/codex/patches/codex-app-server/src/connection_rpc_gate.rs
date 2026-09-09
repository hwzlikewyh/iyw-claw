use std::future::Future;

use tokio::sync::Mutex;
use tokio_util::task::TaskTracker;

/// Per-connection gate for initialized RPC handler execution.
///
/// Closing the gate prevents queued handlers from starting while allowing
/// handlers that already acquired a token to finish.
#[derive(Debug)]
pub(crate) struct ConnectionRpcGate {
    accepting: Mutex<bool>,
    tasks: TaskTracker,
}

impl ConnectionRpcGate {
    pub(crate) fn new() -> Self {
        let accepting = true;
        Self {
            accepting: Mutex::new(accepting),
            tasks: TaskTracker::new(),
        }
    }

    pub(crate) async fn run<F>(&self, future: F)
    where
        F: Future<Output = ()>,
    {
        let token = {
            let accepting = self.accepting.lock().await;
            if !*accepting {
                return;
            }
            self.tasks.token()
        };

        future.await;
        drop(token);
    }

    pub(crate) async fn close(&self) {
        let mut accepting = self.accepting.lock().await;
        *accepting = false;
        self.tasks.close();
    }

    pub(crate) async fn shutdown(&self) {
        self.close().await;
        self.tasks.wait().await;
    }




}

impl Default for ConnectionRpcGate {
    fn default() -> Self {
        Self::new()
    }
}
