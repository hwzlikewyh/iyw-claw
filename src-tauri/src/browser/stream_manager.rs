use tauri::ipc::{Channel, InvokeResponseBody};

use super::error::BrowserError;
use super::frame_protocol::ensure_frame_generations;
use super::manager::BrowserSessionManager;
use super::stream_input::{validate_input_batch, BrowserInputEvent};
use super::types::{BrowserFrameSubscriptionSnapshot, BrowserGenerations};

impl BrowserSessionManager {
    pub async fn subscribe_browser_frames(
        &self,
        tab_id: &str,
        expected: BrowserGenerations,
        channel: Channel<InvokeResponseBody>,
    ) -> Result<BrowserFrameSubscriptionSnapshot, BrowserError> {
        let snapshot = self.snapshot().await;
        let tab = snapshot
            .tabs
            .iter()
            .find(|tab| tab.browser_tab_id == tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        ensure_frame_generations(&tab.generations, &expected)?;
        let action = self.tabs.action_target(tab_id).await?;
        self.streams
            .subscribe(
                tab_id.to_string(),
                tab.generations.clone(),
                action.session,
                action.cli,
                channel,
                None,
            )
            .await
    }

    pub async fn acknowledge_browser_frame(
        &self,
        subscription_id: &str,
        generations: BrowserGenerations,
        seq: u64,
    ) -> Result<(), BrowserError> {
        self.streams
            .acknowledge(subscription_id, &generations, seq)
            .await
    }

    pub async fn browser_frame_subscription(
        &self,
        subscription_id: &str,
        generations: BrowserGenerations,
    ) -> Result<BrowserFrameSubscriptionSnapshot, BrowserError> {
        self.streams.snapshot(subscription_id, &generations).await
    }

    pub async fn send_browser_input(
        &self,
        subscription_id: &str,
        generations: BrowserGenerations,
        events: Vec<BrowserInputEvent>,
    ) -> Result<(), BrowserError> {
        let (messages, semantic) = validate_input_batch(&events)?;
        let tab_id = self.streams.tab_id(subscription_id, &generations).await?;
        self.record_user_input(&tab_id, semantic).await?;
        self.streams
            .input(subscription_id, &generations, messages)
            .await
    }

    pub async fn unsubscribe_browser_frames(
        &self,
        subscription_id: &str,
        generations: BrowserGenerations,
    ) -> Result<(), BrowserError> {
        self.streams
            .unsubscribe(subscription_id, &generations)
            .await
    }
}
