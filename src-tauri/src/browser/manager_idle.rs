use super::error::BrowserError;
use super::manager::BrowserSessionManager;
use super::types::BrowserRuntimeStatus;

impl BrowserSessionManager {
    pub(super) async fn stop_browser_runtime_if_idle(&self, reason: &'static str) {
        if !self.runtime_is_idle().await {
            return;
        }
        let _shutdown_guard = self.shutdown_lock.lock().await;
        if !self.runtime_is_idle().await {
            return;
        }
        let shutdown_epoch = self.begin_shutdown().await;
        let Some(result) = self.shutdown_resources_if_idle().await else {
            self.finish_shutdown(shutdown_epoch).await;
            return;
        };
        self.finish_shutdown(shutdown_epoch).await;
        log_idle_shutdown_result(shutdown_epoch, reason, &result);
    }

    async fn shutdown_resources_if_idle(&self) -> Option<Result<(), BrowserError>> {
        let _tab_guard = self.tab_open_lock.lock().await;
        let _start_guard = self.runtime_start_lock.lock().await;
        if !self.runtime_is_idle().await {
            return None;
        }
        self.close_all_controls().await;
        self.stop_cdp_observer().await;
        Some(self.stop_runtime_state_and_resources().await)
    }

    async fn runtime_is_idle(&self) -> bool {
        let state_idle = {
            let state = self.state.read().await;
            state.runtime.status == BrowserRuntimeStatus::Running && state.tabs.is_empty()
        };
        state_idle && self.tabs.is_empty().await && self.agent_turn_leases.is_empty().await
    }
}

fn log_idle_shutdown_result(shutdown_epoch: u64, reason: &str, result: &Result<(), BrowserError>) {
    match result {
        Ok(()) => tracing::info!(
            target: "iyw_claw_browser",
            shutdown_epoch,
            shutdown_reason = reason,
            "idle browser runtime stopped"
        ),
        Err(error) => tracing::error!(
            target: "iyw_claw_browser",
            shutdown_epoch,
            shutdown_reason = reason,
            error_code = ?error.code,
            error = %error,
            "idle browser runtime stop failed"
        ),
    }
}
