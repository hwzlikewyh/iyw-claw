use serde_json::json;

use super::cdp_records::PopupSeed;
use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::records::TabTicket;
use super::runtime::BrowserRuntimeContext;
use super::tab_actions::validated_url;
use super::tab_launch::bind_existing_tab;

impl BrowserSessionManager {
    pub(super) async fn adopt_popup(
        &self,
        generation: u64,
        target_id: String,
        opener_id: String,
        url: String,
    ) {
        let epoch = self.current_shutdown_epoch();
        let _tab_guard = self.tab_open_lock.lock().await;
        if self.ensure_shutdown_epoch(epoch).is_err() {
            return;
        }
        let cancellation = self.shutdown_cancellation().await;
        let Some(seed) = self.state.read().await.popup_seed(&opener_id) else {
            self.close_orphan_target(&target_id).await;
            return;
        };
        if validated_url(&url).is_err() {
            self.close_orphan_target(&target_id).await;
            return;
        }
        let Some(runtime) = self.popup_runtime(generation).await else {
            self.close_orphan_target(&target_id).await;
            return;
        };
        let Ok(ticket) = self.reserve_tab(url, seed.host_id.clone()).await else {
            self.close_orphan_target(&target_id).await;
            return;
        };
        if let Err(error) = self
            .bind_popup(
                &runtime,
                &ticket,
                &opener_id,
                &seed,
                target_id.clone(),
                cancellation.clone(),
            )
            .await
        {
            let _ = self.rollback_tab(&ticket).await;
            if !cancellation.is_cancelled() {
                self.close_orphan_target(&target_id).await;
            }
            tracing::warn!(
                target: "iyw_claw_browser",
                error_code = ?error.code,
                "popup adoption failed"
            );
        }
    }

    async fn bind_popup(
        &self,
        runtime: &BrowserRuntimeContext,
        ticket: &TabTicket,
        opener_id: &str,
        seed: &PopupSeed,
        target_id: String,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<(), BrowserError> {
        let launched = bind_existing_tab(
            &self.tab_cleanups,
            runtime,
            ticket,
            &target_id,
            cancellation,
        )
        .await?;
        let watch = match self.tabs.insert(launched.handle).await {
            Ok(watch) => watch,
            Err(handle) => {
                let _ = self.cleanup_or_retain_tab_handle(handle, true).await;
                return Err(BrowserError::new(
                    BrowserErrorCode::BrowserInternal,
                    "The popup browser tab could not be registered",
                ));
            }
        };
        let commit = self.state.write().await.commit_popup_live(
            ticket,
            opener_id,
            seed,
            target_id,
            launched.title,
            launched.url,
        );
        if let Err(error) = commit {
            if let Some(handle) = self.tabs.take(&ticket.tab_id).await {
                let _ = self.cleanup_or_retain_tab_handle(handle, true).await;
            }
            return Err(error);
        }
        self.spawn_tab_watcher(watch);
        Ok(())
    }

    async fn popup_runtime(&self, generation: u64) -> Option<BrowserRuntimeContext> {
        let runtime = self.runtime.as_ref()?.context().await?;
        (runtime.generation == generation).then_some(runtime)
    }

    async fn close_orphan_target(&self, target_id: &str) {
        let _ = self
            .cdp_call("Target.closeTarget", json!({ "targetId": target_id }), None)
            .await;
    }
}
