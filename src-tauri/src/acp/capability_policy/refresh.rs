use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::dto::CapabilityPolicySnapshot;
use super::error::CapabilityPolicyError;
use super::store::{CapabilityPolicyStore, PolicySnapshotView};

#[derive(Debug)]
pub enum PolicyFetch {
    NotModified,
    Updated {
        snapshot: CapabilityPolicySnapshot,
        etag: Option<String>,
    },
}

#[async_trait]
pub trait CapabilityPolicyFetcher: Send + Sync {
    async fn fetch(&self, etag: Option<&str>) -> Result<PolicyFetch, CapabilityPolicyError>;
}

#[derive(Debug, Clone, Copy)]
pub struct RefreshConfig {
    pub interval: Duration,
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(60),
        }
    }
}

pub struct CapabilityPolicyRefreshRuntime {
    shutdown: CancellationToken,
}

impl CapabilityPolicyRefreshRuntime {
    pub fn start(
        store: CapabilityPolicyStore,
        fetcher: Arc<dyn CapabilityPolicyFetcher>,
        config: RefreshConfig,
    ) -> Self {
        let shutdown = CancellationToken::new();
        start_background_refresh(store, fetcher, shutdown.clone(), config);
        Self { shutdown }
    }
}

impl Drop for CapabilityPolicyRefreshRuntime {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

pub async fn refresh_once(
    store: &CapabilityPolicyStore,
    fetcher: &dyn CapabilityPolicyFetcher,
) -> Result<PolicySnapshotView, CapabilityPolicyError> {
    let etag = store.view().await.etag;
    let result = match fetcher.fetch(etag.as_deref()).await {
        Ok(PolicyFetch::NotModified) => refresh_after_not_modified(store, fetcher).await,
        Ok(PolicyFetch::Updated { snapshot, etag }) => {
            store.accept_remote(snapshot, etag, Utc::now()).await
        }
        Err(error) => Err(error),
    };
    if let Err(error) = &result {
        if error.rejects_revision() {
            store.record_revision_rollback().await;
        } else {
            store.record_refresh_failure().await;
        }
    }
    result
}

async fn refresh_after_not_modified(
    store: &CapabilityPolicyStore,
    fetcher: &dyn CapabilityPolicyFetcher,
) -> Result<PolicySnapshotView, CapabilityPolicyError> {
    let view = store.record_not_modified().await?;
    if !is_expired(&view) {
        return Ok(view);
    }
    match fetcher.fetch(None).await? {
        PolicyFetch::Updated { snapshot, etag } => {
            store.accept_remote(snapshot, etag, Utc::now()).await
        }
        PolicyFetch::NotModified => Err(CapabilityPolicyError::UnconditionalNotModified),
    }
}

fn is_expired(view: &PolicySnapshotView) -> bool {
    view.snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.expires_at <= Utc::now())
}

pub fn start_background_refresh(
    store: CapabilityPolicyStore,
    fetcher: Arc<dyn CapabilityPolicyFetcher>,
    shutdown: CancellationToken,
    config: RefreshConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        log_refresh_result(refresh_once(&store, fetcher.as_ref()).await);
        let mut ticker = tokio::time::interval(config.interval);
        // Do not replay every tick missed while the desktop process was
        // suspended or the runtime was otherwise stalled. A burst would
        // exhaust the Agent Platform rate limit without providing fresher
        // policy data.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = ticker.tick() => log_refresh_result(refresh_once(&store, fetcher.as_ref()).await),
            }
        }
    })
}

fn log_refresh_result(result: Result<PolicySnapshotView, CapabilityPolicyError>) {
    match result {
        Ok(view) => tracing::debug!(
            source = ?view.source,
            revision = view.snapshot.as_ref().map(|snapshot| snapshot.revision),
            "[capability-policy] refreshed trusted snapshot"
        ),
        Err(error) => tracing::warn!(
            error = %error,
            "[capability-policy] refresh failed; retaining last trusted snapshot"
        ),
    }
}
