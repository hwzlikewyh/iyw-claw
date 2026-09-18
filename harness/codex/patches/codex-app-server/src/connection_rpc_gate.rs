//! Controls RPC admission and draining for each connection.
//! Auth ownership changes reject queued work while started handlers finish cleanup.

use std::future::Future;

use codex_app_server_transport::ConnectionAuth;

use tokio::sync::Mutex;
use tokio_util::task::TaskTracker;

/// Per-connection gate for incoming messages and initialized RPC handler execution.
///
/// Closing the gate prevents queued handlers from starting while allowing
/// handlers that already acquired a token to finish.
#[derive(Debug)]
pub(crate) struct ConnectionRpcGate {
    pub(crate) auth: Option<ConnectionAuth>,
    accepting: Mutex<bool>,
    tasks: TaskTracker,
}

impl ConnectionRpcGate {
    pub(crate) fn new() -> Self {
        let accepting = true;
        Self {
            auth: None,
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
            if !*accepting || self.is_closed() {
                return;
            }
            self.tasks.token()
        };

        future.await;
        drop(token);
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.tasks.is_closed() || self.auth.as_ref().is_some_and(|auth| !auth.is_current())
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
