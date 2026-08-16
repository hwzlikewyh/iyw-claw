use reqwest::Url;
use tokio_util::sync::CancellationToken;

use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::tab_launch::{cleanup_tab, launch_tab};
use super::tab_metadata::page_metadata;
use super::types::{BrowserGenerations, BrowserStateSnapshot};

mod tab_close;
mod tab_shutdown;

const NAVIGATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

impl BrowserSessionManager {
    pub async fn create_browser_tab(
        &self,
        url: String,
        host_id: Option<String>,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let epoch = self.current_shutdown_epoch();
        let _guard = self.tab_open_lock.lock().await;
        self.ensure_shutdown_epoch(epoch)?;
        let cancellation = self.shutdown_cancellation().await;
        self.create_browser_tab_with_id_unlocked(url, host_id, cancellation)
            .await
            .map(|(state, _)| state)
    }

    pub(super) async fn create_browser_tab_with_id_unlocked(
        &self,
        url: String,
        host_id: Option<String>,
        cancellation: CancellationToken,
    ) -> Result<(BrowserStateSnapshot, String), BrowserError> {
        let url = validated_url(&url)?;
        let runtime = self.ensure_runtime_running(cancellation.clone()).await?;
        let ticket = self.reserve_tab(url.clone(), host_id).await?;
        let launched = match launch_tab(&runtime, &ticket, &url, cancellation).await {
            Ok(launched) => launched,
            Err(error) => {
                let _ = self.rollback_tab(&ticket).await;
                return Err(error);
            }
        };
        let target_id = launched.handle.target_id.clone();
        let watch = match self.tabs.insert(launched.handle).await {
            Ok(watch) => watch,
            Err(handle) => {
                let _ = cleanup_tab(handle, true).await;
                let _ = self.rollback_tab(&ticket).await;
                return Err(BrowserError::new(
                    BrowserErrorCode::BrowserInternal,
                    "The browser tab runtime could not be registered",
                ));
            }
        };
        if let Err(error) = self
            .commit_tab_live(&ticket, target_id, launched.title, launched.url)
            .await
        {
            if let Some(handle) = self.tabs.take(&ticket.tab_id).await {
                let _ = cleanup_tab(handle, true).await;
            }
            let _ = self.rollback_tab(&ticket).await;
            return Err(error);
        }
        self.spawn_tab_watcher(watch);
        Ok((self.snapshot().await, ticket.tab_id))
    }

    pub async fn ensure_initial_browser_tab(
        &self,
        url: String,
        host_id: Option<String>,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let epoch = self.current_shutdown_epoch();
        let _guard = self.tab_open_lock.lock().await;
        self.ensure_shutdown_epoch(epoch)?;
        let cancellation = self.shutdown_cancellation().await;
        let state = self.snapshot().await;
        if !state.tabs.is_empty() {
            tracing::debug!(
                target: "iyw_claw_browser",
                tab_count = state.tabs.len(),
                "initial browser tab already exists"
            );
            return Ok(state);
        }
        tracing::info!(
            target: "iyw_claw_browser",
            "creating initial browser tab"
        );
        self.create_browser_tab_with_id_unlocked(url, host_id, cancellation)
            .await
            .map(|(state, _)| state)
    }

    pub async fn navigate_browser_tab(
        &self,
        tab_id: &str,
        url: String,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let url = validated_url(&url)?;
        self.run_user_navigation(tab_id, &["open", &url]).await
    }

    pub async fn browser_back(&self, tab_id: &str) -> Result<BrowserStateSnapshot, BrowserError> {
        self.run_user_navigation(tab_id, &["back"]).await
    }

    pub async fn browser_forward(
        &self,
        tab_id: &str,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        self.run_user_navigation(tab_id, &["forward"]).await
    }

    pub async fn reload_browser_tab(
        &self,
        tab_id: &str,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let status = self
            .snapshot()
            .await
            .tabs
            .into_iter()
            .find(|tab| tab.browser_tab_id == tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?
            .status;
        let lease = self.acquire_user_control(tab_id).await?;
        let result = if matches!(
            status,
            super::types::BrowserTabStatus::Crashed | super::types::BrowserTabStatus::Gone
        ) {
            self.restore_browser_tab(tab_id).await
        } else {
            self.run_navigation(tab_id, &["reload"], CancellationToken::new())
                .await
        };
        lease.finish().await;
        result
    }

    pub async fn resize_browser_viewport(
        &self,
        tab_id: &str,
        expected: BrowserGenerations,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        validate_viewport(width, height, scale)?;
        let snapshot = self.snapshot().await;
        let tab = snapshot
            .tabs
            .iter()
            .find(|tab| tab.browser_tab_id == tab_id)
            .ok_or_else(|| BrowserError::tab_not_found(tab_id))?;
        super::frame_protocol::ensure_frame_generations(&tab.generations, &expected)?;
        let action = self.tabs.action_target(tab_id).await?;
        let width = width.to_string();
        let height = height.to_string();
        let scale = scale.to_string();
        action
            .cli
            .run_pinned(
                &action.session,
                &action.cdp_url,
                &["set", "viewport", &width, &height, &scale],
                NAVIGATION_TIMEOUT,
                CancellationToken::new(),
            )
            .await?;
        Ok(self.snapshot().await)
    }

    async fn run_navigation(
        &self,
        tab_id: &str,
        args: &[&str],
        cancellation: CancellationToken,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let action = self.tabs.action_target(tab_id).await?;
        let ticket = self.state.write().await.begin_tab_navigation(tab_id)?;
        if ticket.runtime_generation != action.runtime_generation {
            let _ = self.state.write().await.fail_tab_navigation(&ticket);
            return Err(BrowserError::new(
                BrowserErrorCode::BrowserStaleGeneration,
                "The browser tab belongs to an obsolete runtime",
            ));
        }
        let result = action
            .cli
            .run_pinned(
                &action.session,
                &action.cdp_url,
                args,
                NAVIGATION_TIMEOUT,
                cancellation.clone(),
            )
            .await;
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                self.finish_failed_navigation(&ticket, &error).await;
                return Err(error);
            }
        };
        let (title, url) = match page_metadata(
            &action.cli,
            &action.session,
            &action.cdp_url,
            &response,
            cancellation,
        )
        .await
        {
            Ok(metadata) => metadata,
            Err(error) => {
                self.finish_failed_navigation(&ticket, &error).await;
                return Err(error);
            }
        };
        self.state
            .write()
            .await
            .finish_tab_navigation(&ticket, title, url)?;
        Ok(self.snapshot().await)
    }

    async fn run_user_navigation(
        &self,
        tab_id: &str,
        args: &[&str],
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let lease = self.acquire_user_control(tab_id).await?;
        let result = self
            .run_navigation(tab_id, args, CancellationToken::new())
            .await;
        lease.finish().await;
        result
    }

    pub(super) async fn navigate_browser_tab_as_agent(
        &self,
        tab_id: &str,
        url: String,
        cancellation: CancellationToken,
    ) -> Result<BrowserStateSnapshot, BrowserError> {
        let url = validated_url(&url)?;
        self.run_navigation(tab_id, &["open", &url], cancellation)
            .await
    }
}

pub(super) fn validated_url(value: &str) -> Result<String, BrowserError> {
    let url = Url::parse(value).map_err(|_| invalid_navigation())?;
    let allowed = matches!(url.scheme(), "http" | "https")
        || (url.scheme() == "about" && url.path() == "blank");
    if !allowed || !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_navigation());
    }
    Ok(url.to_string())
}

fn invalid_navigation() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserNavigationFailed,
        "Only HTTP, HTTPS, and about:blank navigation is supported",
    )
}

fn validate_viewport(width: u32, height: u32, scale: f64) -> Result<(), BrowserError> {
    if (320..=4096).contains(&width)
        && (240..=4096).contains(&height)
        && scale.is_finite()
        && (0.5..=3.0).contains(&scale)
    {
        return Ok(());
    }
    Err(BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The browser viewport is invalid",
    ))
}
