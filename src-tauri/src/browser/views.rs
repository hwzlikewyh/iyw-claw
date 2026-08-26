use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};

use super::error::{BrowserError, BrowserErrorCode};
use super::frame_protocol::ensure_frame_generations;
use super::manager::BrowserSessionManager;
use super::types::{
    BrowserFrameSubscriptionSnapshot, BrowserGenerations, BrowserHostKind, BrowserHostRegistration,
    BrowserStateSnapshot, BrowserViewClaimSnapshot,
};

const HIDDEN_STREAM_DELAY: Duration = Duration::from_secs(2);
const CLAIM_TIMEOUT: Duration = Duration::from_secs(15);

impl BrowserSessionManager {
    pub async fn register_browser_host<F>(
        &self,
        window_label: String,
        kind: BrowserHostKind,
        validate_window: F,
    ) -> Result<BrowserHostRegistration, BrowserError>
    where
        F: FnOnce() -> Result<(), BrowserError> + Send,
    {
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        self.ensure_shutdown_epoch(epoch)?;
        validate_window_label(&window_label, kind)?;
        validate_window()?;
        let (host_id, generation, created) =
            self.state.write().await.register_host(window_label, kind)?;
        if created {
            self.spawn_host_monitor(host_id.clone());
        }
        Ok(BrowserHostRegistration {
            host_id,
            generation,
            state: self.snapshot().await,
        })
    }

    pub async fn heartbeat_browser_host(
        &self,
        host_id: &str,
        generation: u64,
        visible: bool,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let became_hidden = self
            .state
            .write()
            .await
            .heartbeat_host(host_id, generation, visible)?;
        if became_hidden {
            self.spawn_hidden_stream_pause(host_id.to_string(), generation);
        }
        Ok(self.snapshot().await)
    }

    pub async fn set_browser_host_visible(
        &self,
        host_id: &str,
        generation: u64,
        visible: bool,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let became_hidden = self
            .state
            .write()
            .await
            .set_host_visible(host_id, generation, visible)?;
        if became_hidden {
            self.spawn_hidden_stream_pause(host_id.to_string(), generation);
        }
        Ok(self.snapshot().await)
    }

    pub async fn activate_browser_tab(
        &self,
        host_id: &str,
        host_generation: u64,
        tab_id: &str,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let previous =
            self.state
                .write()
                .await
                .activate_host_tab(host_id, host_generation, tab_id)?;
        if let Some(previous_id) = previous.filter(|id| id != tab_id) {
            self.streams.close_tab(&previous_id).await;
        }
        Ok(self.snapshot().await)
    }

    pub async fn begin_browser_view_claim(
        &self,
        tab_id: &str,
        source_host_id: Option<String>,
        target_host_id: String,
        target_index: usize,
    ) -> Result<BrowserViewClaimSnapshot, BrowserError> {
        let _tab_guard = self.tab_open_lock.lock().await;
        self.agent_turn_leases.ensure_claimable(tab_id).await?;
        let claim = self.state.write().await.begin_view_claim(
            tab_id,
            source_host_id,
            target_host_id,
            target_index,
        )?;
        self.spawn_claim_timeout(claim.claim_id.clone());
        Ok(claim)
    }

    pub async fn subscribe_browser_claim_frames(
        &self,
        claim_id: &str,
        expected: BrowserGenerations,
        channel: Channel<InvokeResponseBody>,
    ) -> Result<BrowserFrameSubscriptionSnapshot, BrowserError> {
        let claim = self.state.read().await.claim_snapshot(claim_id)?;
        ensure_frame_generations(&claim.generations, &expected)?;
        let action = self.tabs.action_target(&claim.browser_tab_id).await?;
        self.streams
            .subscribe(
                claim.browser_tab_id,
                claim.generations,
                action.session,
                action.cli,
                action.cdp_url,
                channel,
                Some(claim_id.to_string()),
            )
            .await
    }

    pub async fn acknowledge_browser_claim_frame(
        &self,
        claim_id: &str,
        subscription_id: &str,
        generations: BrowserGenerations,
        seq: u64,
    ) -> Result<BrowserViewClaimSnapshot, BrowserError> {
        let claim = self.state.read().await.claim_snapshot(claim_id)?;
        self.streams
            .validate_claim_subscription(
                subscription_id,
                claim_id,
                &claim.browser_tab_id,
                &generations,
            )
            .await?;
        self.streams
            .acknowledge(subscription_id, &generations, seq)
            .await?;
        self.state
            .write()
            .await
            .acknowledge_view_claim(claim_id, &generations, seq)
    }

    pub async fn commit_browser_view_claim(
        &self,
        claim_id: &str,
        subscription_id: &str,
        generations: BrowserGenerations,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let _tab_guard = self.tab_open_lock.lock().await;
        let claim = self.state.read().await.claim_snapshot(claim_id)?;
        self.agent_turn_leases
            .ensure_claimable(&claim.browser_tab_id)
            .await?;
        self.streams
            .validate_claim_subscription(
                subscription_id,
                claim_id,
                &claim.browser_tab_id,
                &generations,
            )
            .await?;
        self.state
            .write()
            .await
            .commit_view_claim(claim_id, &generations)?;
        if let Err(error) = self.streams.promote_claim(subscription_id, claim_id).await {
            self.streams.close_claim(claim_id).await;
            return Err(error.effect_may_have_occurred(true));
        }
        self.streams
            .close_tab_except(&claim.browser_tab_id, subscription_id)
            .await;
        self.cancel_window_open_requests(vec![claim.browser_tab_id.clone()])
            .await;
        Ok(self.snapshot().await)
    }

    pub async fn abort_browser_view_claim(
        &self,
        claim_id: &str,
        generations: BrowserGenerations,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        self.state
            .write()
            .await
            .abort_view_claim(claim_id, &generations)?;
        self.streams.close_claim(claim_id).await;
        Ok(self.snapshot().await)
    }

    fn spawn_claim_timeout(&self, claim_id: String) {
        let manager = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(CLAIM_TIMEOUT).await;
            manager.state.write().await.expire_view_claim(&claim_id);
            manager.streams.close_claim(&claim_id).await;
        });
    }

    fn spawn_hidden_stream_pause(&self, host_id: String, generation: u64) {
        let manager = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(HIDDEN_STREAM_DELAY).await;
            let tabs = manager
                .state
                .read()
                .await
                .hidden_host_tabs(&host_id, generation);
            for tab_id in tabs.unwrap_or_default() {
                manager.streams.close_tab(&tab_id).await;
            }
        });
    }
}

fn validate_window_label(label: &str, kind: BrowserHostKind) -> Result<(), BrowserError> {
    let valid = match kind {
        BrowserHostKind::Docked => label == "main",
        BrowserHostKind::Detached => label
            .strip_prefix("browser-")
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .is_some(),
    };
    valid.then_some(()).ok_or_else(|| {
        BrowserError::new(
            BrowserErrorCode::BrowserViewConflict,
            "The browser window label is invalid",
        )
    })
}
