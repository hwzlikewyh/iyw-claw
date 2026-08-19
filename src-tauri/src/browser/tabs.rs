use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::command_runner::AgentBrowserCli;
use super::error::BrowserError;
use super::process::{process_matches, ProcessRecord};

const PROCESS_WATCH_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Default)]
pub(super) struct BrowserTabRegistry {
    inner: Mutex<HashMap<String, TabRuntimeHandle>>,
}

#[derive(Debug)]
pub(super) struct TabRuntimeHandle {
    pub tab_id: String,
    pub session: String,
    pub target_id: String,
    pub runtime_generation: u64,
    pub cli: AgentBrowserCli,
    pub cdp_url: String,
    pub controller_session: String,
    pub daemon: ProcessRecord,
    pub cancellation: CancellationToken,
}

#[derive(Debug, Clone)]
pub(super) struct TabActionTarget {
    pub session: String,
    pub runtime_generation: u64,
    pub cli: AgentBrowserCli,
    pub cdp_url: String,
}

pub(super) struct TabExitWatch {
    tab_id: String,
    session: String,
    runtime_generation: u64,
    daemon: ProcessRecord,
    cancellation: CancellationToken,
}

impl BrowserTabRegistry {
    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }

    pub async fn insert(&self, handle: TabRuntimeHandle) -> Result<TabExitWatch, TabRuntimeHandle> {
        let mut inner = self.inner.lock().await;
        if inner.contains_key(&handle.tab_id)
            || inner.values().any(|existing| {
                existing.session == handle.session || existing.target_id == handle.target_id
            })
        {
            return Err(handle);
        }
        let watch = TabExitWatch::from_handle(&handle);
        inner.insert(handle.tab_id.clone(), handle);
        Ok(watch)
    }

    pub async fn action_target(&self, tab_id: &str) -> Result<TabActionTarget, BrowserError> {
        self.inner
            .lock()
            .await
            .get(tab_id)
            .map(TabActionTarget::from)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))
    }

    pub async fn take(&self, tab_id: &str) -> Option<TabRuntimeHandle> {
        self.inner.lock().await.remove(tab_id).map(|handle| {
            handle.cancellation.cancel();
            handle
        })
    }

    pub async fn take_owned(&self, tab_id: &str, session: &str) -> Option<TabRuntimeHandle> {
        let mut inner = self.inner.lock().await;
        let owned = inner
            .get(tab_id)
            .is_some_and(|handle| handle.session == session);
        owned.then(|| inner.remove(tab_id)).flatten().map(|handle| {
            handle.cancellation.cancel();
            handle
        })
    }

    pub async fn drain(&self) -> Vec<TabRuntimeHandle> {
        let mut inner = self.inner.lock().await;
        inner
            .drain()
            .map(|(_, handle)| {
                handle.cancellation.cancel();
                handle
            })
            .collect()
    }
}

impl TabExitWatch {
    fn from_handle(handle: &TabRuntimeHandle) -> Self {
        Self {
            tab_id: handle.tab_id.clone(),
            session: handle.session.clone(),
            runtime_generation: handle.runtime_generation,
            daemon: handle.daemon.clone(),
            cancellation: handle.cancellation.clone(),
        }
    }

    pub async fn wait(self) -> Option<(String, String, u64)> {
        loop {
            if !process_matches(&self.daemon) {
                return Some((self.tab_id, self.session, self.runtime_generation));
            }
            tokio::select! {
                _ = self.cancellation.cancelled() => return None,
                _ = tokio::time::sleep(PROCESS_WATCH_INTERVAL) => {}
            }
        }
    }
}

impl From<&TabRuntimeHandle> for TabActionTarget {
    fn from(handle: &TabRuntimeHandle) -> Self {
        Self {
            session: handle.session.clone(),
            runtime_generation: handle.runtime_generation,
            cli: handle.cli.clone(),
            cdp_url: handle.cdp_url.clone(),
        }
    }
}
