use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
#[cfg(feature = "tauri-runtime")]
use tokio_util::sync::CancellationToken;

#[cfg(feature = "tauri-runtime")]
use super::cdp_observer::CdpObserverHandle;
use super::control::ControlGate;
use super::control_lease::AgentControlLease;
use super::error::BrowserError;
use super::records::{RuntimeStartDecision, RuntimeTicket, TabTicket};
#[cfg(feature = "tauri-runtime")]
use super::runtime::BrowserRuntime;
use super::state::BrowserState;
#[cfg(feature = "tauri-runtime")]
use super::stream::BrowserStreamRegistry;
#[cfg(feature = "tauri-runtime")]
use super::tabs::BrowserTabRegistry;
use super::types::{BrowserCapability, BrowserStateSnapshot};
#[cfg(feature = "tauri-runtime")]
use super::user_control_lease::UserControlLease;

#[derive(Debug, Clone)]
pub struct BrowserSessionManager {
    pub(super) state: Arc<RwLock<BrowserState>>,
    pub(super) controls: Arc<Mutex<HashMap<String, ControlGate>>>,
    pub(super) snapshot_revision: Arc<AtomicU64>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) shutdown_lock: Arc<Mutex<()>>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) shutdown_cancellation: Arc<Mutex<CancellationToken>>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) runtime_start_lock: Arc<Mutex<()>>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) tab_open_lock: Arc<Mutex<()>>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) shutdown_epoch: Arc<AtomicU64>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) runtime: Option<Arc<BrowserRuntime>>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) tabs: Arc<BrowserTabRegistry>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) streams: Arc<BrowserStreamRegistry>,
    #[cfg(feature = "tauri-runtime")]
    pub(super) observer: Arc<Mutex<Option<CdpObserverHandle>>>,
}

impl BrowserSessionManager {
    pub fn new(capability: BrowserCapability) -> Self {
        Self {
            state: Arc::new(RwLock::new(BrowserState::new(capability))),
            controls: Arc::new(Mutex::new(HashMap::new())),
            snapshot_revision: Arc::new(AtomicU64::new(0)),
            #[cfg(feature = "tauri-runtime")]
            shutdown_lock: Arc::new(Mutex::new(())),
            #[cfg(feature = "tauri-runtime")]
            shutdown_cancellation: Arc::new(Mutex::new(CancellationToken::new())),
            #[cfg(feature = "tauri-runtime")]
            runtime_start_lock: Arc::new(Mutex::new(())),
            #[cfg(feature = "tauri-runtime")]
            tab_open_lock: Arc::new(Mutex::new(())),
            #[cfg(feature = "tauri-runtime")]
            shutdown_epoch: Arc::new(AtomicU64::new(0)),
            #[cfg(feature = "tauri-runtime")]
            runtime: None,
            #[cfg(feature = "tauri-runtime")]
            tabs: Arc::new(BrowserTabRegistry::default()),
            #[cfg(feature = "tauri-runtime")]
            streams: Arc::new(BrowserStreamRegistry::default()),
            #[cfg(feature = "tauri-runtime")]
            observer: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_capability(&self, capability: BrowserCapability) {
        self.state.write().await.set_capability(capability);
    }

    #[cfg(feature = "tauri-runtime")]
    pub(super) fn current_shutdown_epoch(&self) -> u64 {
        self.shutdown_epoch.load(Ordering::Acquire)
    }

    #[cfg(feature = "tauri-runtime")]
    pub(super) fn ensure_shutdown_epoch(&self, observed: u64) -> Result<(), BrowserError> {
        if observed % 2 == 0 && self.current_shutdown_epoch() == observed {
            return Ok(());
        }
        Err(BrowserError::new(
            super::error::BrowserErrorCode::BrowserCancelled,
            "The browser closed while the tab was waiting to open",
        ))
    }

    #[cfg(feature = "tauri-runtime")]
    pub(super) async fn shutdown_cancellation(&self) -> CancellationToken {
        self.shutdown_cancellation.lock().await.clone()
    }

    #[cfg(feature = "tauri-runtime")]
    pub async fn run_browser_window_creation<T, F>(&self, create: F) -> Result<T, BrowserError>
    where
        T: Send,
        F: FnOnce() -> Result<T, BrowserError> + Send,
    {
        let epoch = self.current_shutdown_epoch();
        let _guard = self.tab_open_lock.lock().await;
        self.ensure_shutdown_epoch(epoch)?;
        create()
    }

    pub async fn snapshot(&self) -> BrowserStateSnapshot {
        let mut snapshot = {
            let state = self.state.read().await;
            let mut snapshot = state.snapshot();
            snapshot.state_revision = self
                .snapshot_revision
                .fetch_add(1, Ordering::AcqRel)
                .saturating_add(1);
            snapshot
        };
        let controls: Vec<(String, ControlGate)> = {
            let controls = self.controls.lock().await;
            controls
                .iter()
                .map(|(id, gate)| (id.clone(), gate.clone()))
                .collect()
        };
        for (tab_id, gate) in controls {
            if let Some(tab) = snapshot
                .tabs
                .iter_mut()
                .find(|tab| tab.browser_tab_id == tab_id)
            {
                let control = gate.snapshot().await;
                tab.control_status = control.status;
                tab.generations.control_epoch = control.epoch;
            }
        }
        snapshot
    }

    pub(super) async fn begin_runtime_start(&self) -> Result<RuntimeStartDecision, BrowserError> {
        self.state.write().await.begin_runtime_start()
    }

    pub(super) async fn complete_runtime_start(
        &self,
        ticket: &RuntimeTicket,
    ) -> Result<(), BrowserError> {
        self.state.write().await.complete_runtime_start(ticket)
    }

    pub(super) async fn fail_runtime_start(
        &self,
        ticket: &RuntimeTicket,
        failure_code: impl Into<String>,
    ) -> Result<(), BrowserError> {
        self.state
            .write()
            .await
            .fail_runtime_start(ticket, failure_code)
    }

    pub(super) async fn record_runtime_exit(&self, generation: u64, failure_code: String) -> bool {
        self.state
            .write()
            .await
            .record_runtime_exit(generation, failure_code)
    }

    pub(super) async fn close_all_controls(&self) {
        let controls = {
            let mut controls = self.controls.lock().await;
            controls.drain().map(|(_, gate)| gate).collect::<Vec<_>>()
        };
        for gate in controls {
            gate.close().await;
        }
    }

    pub(super) async fn close_control(&self, tab_id: &str) {
        let gate = self.controls.lock().await.remove(tab_id);
        if let Some(gate) = gate {
            gate.close().await;
        }
    }

    pub(super) async fn reset_control(&self, tab_id: &str) {
        let previous = self
            .controls
            .lock()
            .await
            .insert(tab_id.to_string(), ControlGate::new());
        if let Some(previous) = previous {
            previous.close().await;
        }
    }

    pub(super) async fn reserve_tab(
        &self,
        url: String,
        host_id: Option<String>,
    ) -> Result<TabTicket, BrowserError> {
        let ticket = self.state.write().await.reserve_tab(url, host_id)?;
        self.controls
            .lock()
            .await
            .insert(ticket.tab_id.clone(), ControlGate::new());
        Ok(ticket)
    }

    pub(super) async fn commit_tab_live(
        &self,
        ticket: &TabTicket,
        target_id: String,
        title: String,
        url: String,
    ) -> Result<(), BrowserError> {
        self.state
            .write()
            .await
            .commit_tab_live(ticket, target_id, title, url)
    }

    pub(super) async fn rollback_tab(&self, ticket: &TabTicket) -> Result<(), BrowserError> {
        let result = self.state.write().await.rollback_tab(ticket);
        let gate = { self.controls.lock().await.remove(&ticket.tab_id) };
        if let Some(gate) = gate {
            gate.close().await;
        }
        result
    }

    pub(super) async fn begin_tab_close(&self, tab_id: &str) -> Result<TabTicket, BrowserError> {
        let ticket = self.state.write().await.begin_tab_close(tab_id)?;
        if let Some(gate) = self.control_gate(tab_id).await {
            gate.close().await;
        }
        Ok(ticket)
    }

    pub(super) async fn finish_tab_close(&self, ticket: &TabTicket) -> Result<(), BrowserError> {
        let result = self.state.write().await.finish_tab_close(ticket);
        self.controls.lock().await.remove(&ticket.tab_id);
        result
    }

    pub async fn record_user_input(
        &self,
        tab_id: &str,
        semantic: bool,
    ) -> Result<(), BrowserError> {
        let gate = self
            .control_gate(tab_id)
            .await
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        gate.record_user_input(semantic).await;
        Ok(())
    }

    #[cfg(feature = "tauri-runtime")]
    pub(super) async fn acquire_user_control(
        &self,
        tab_id: &str,
    ) -> Result<UserControlLease, BrowserError> {
        let gate = self
            .control_gate(tab_id)
            .await
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        gate.acquire_user().await
    }

    pub async fn set_user_held(&self, tab_id: &str, held: bool) -> Result<(), BrowserError> {
        let gate = self
            .control_gate(tab_id)
            .await
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        gate.set_user_held(held).await;
        Ok(())
    }

    pub async fn acquire_agent_control(
        &self,
        tab_id: &str,
    ) -> Result<AgentControlLease, BrowserError> {
        let gate = self
            .control_gate(tab_id)
            .await
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        gate.acquire_agent().await
    }

    async fn control_gate(&self, tab_id: &str) -> Option<ControlGate> {
        self.controls.lock().await.get(tab_id).cloned()
    }
}
