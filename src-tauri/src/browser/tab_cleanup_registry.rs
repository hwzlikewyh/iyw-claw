use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Mutex;

use super::error::{BrowserError, BrowserErrorCode};
use super::manager::BrowserSessionManager;
use super::process::ProcessRecord;
use super::records::TabTicket;
use super::runtime::BrowserRuntimeContext;
use super::tab_cleanup::{cleanup_pending_owner, PendingTabCleanup};
use super::tabs::TabRuntimeHandle;

type CleanupMap = HashMap<String, Vec<PendingTabCleanup>>;

#[derive(Debug, Clone)]
pub(super) struct TabCleanupToken {
    id: u64,
    session: String,
}

#[derive(Debug, Default)]
pub(super) struct PendingTabCleanupRegistry {
    next_id: AtomicU64,
    operation: Mutex<()>,
    inner: Mutex<CleanupMap>,
}

impl PendingTabCleanupRegistry {
    pub async fn begin_launch(
        &self,
        runtime: &BrowserRuntimeContext,
        ticket: &TabTicket,
        session: String,
        close_target: bool,
    ) -> Result<TabCleanupToken, BrowserError> {
        let _operation = self.operation.lock().await;
        self.retry_session(&session).await?;
        let id = self.next_owner_id();
        let token = TabCleanupToken {
            id,
            session: session.clone(),
        };
        let owner =
            PendingTabCleanup::for_launch(id, runtime, ticket, session.clone(), close_target);
        let mut inner = self.inner.lock().await;
        if inner.get(&session).is_some_and(|owners| !owners.is_empty()) {
            return Err(cleanup_still_pending());
        }
        inner.entry(session).or_default().push(owner);
        Ok(token)
    }

    pub async fn record_target(
        &self,
        token: &TabCleanupToken,
        target_id: String,
    ) -> Result<(), BrowserError> {
        let mut inner = self.inner.lock().await;
        let owner = find_owner_mut(&mut inner, token).ok_or_else(cleanup_owner_missing)?;
        owner.record_target(target_id);
        Ok(())
    }

    pub async fn record_daemon(
        &self,
        token: &TabCleanupToken,
        daemon: ProcessRecord,
    ) -> Result<(), BrowserError> {
        let mut inner = self.inner.lock().await;
        let owner = find_owner_mut(&mut inner, token).ok_or_else(cleanup_owner_missing)?;
        owner.record_daemon(daemon);
        Ok(())
    }

    pub async fn release_launch(&self, token: &TabCleanupToken) -> Result<(), BrowserError> {
        let mut inner = self.inner.lock().await;
        take_owner(&mut inner, token)
            .map(|_| ())
            .ok_or_else(cleanup_owner_missing)
    }

    pub async fn finish_failed_launch(&self, token: TabCleanupToken) {
        let _operation = self.operation.lock().await;
        let mut inner = self.inner.lock().await;
        let owner = take_owner(&mut inner, &token);
        drop(inner);
        let Some(mut owner) = owner else {
            tracing::error!(
                target: "iyw_claw_browser",
                cleanup_owner_id = token.id,
                session = %token.session,
                "failed browser tab launch lost its cleanup owner"
            );
            return;
        };
        if let Err(error) = cleanup_pending_owner(&mut owner).await {
            log_cleanup_retained(&owner, &error, "failed_launch");
            self.restore(vec![owner]).await;
        }
    }

    pub async fn cleanup_handle(
        &self,
        handle: TabRuntimeHandle,
        close_target: bool,
    ) -> Result<(), BrowserError> {
        let _operation = self.operation.lock().await;
        let mut owner = PendingTabCleanup::from_handle(self.next_owner_id(), handle, close_target);
        let result = cleanup_pending_owner(&mut owner).await;
        if let Err(error) = &result {
            log_cleanup_retained(&owner, error, "runtime_handle");
            self.restore(vec![owner]).await;
        }
        result
    }

    pub async fn retain_handles(&self, handles: Vec<TabRuntimeHandle>, close_target: bool) {
        let owners = handles
            .into_iter()
            .map(|handle| {
                PendingTabCleanup::from_handle(self.next_owner_id(), handle, close_target)
            })
            .collect();
        self.restore(owners).await;
    }

    pub async fn retry_tab(&self, tab_id: &str) -> Option<Result<(), BrowserError>> {
        let _operation = self.operation.lock().await;
        let owners = self.take_tab(tab_id).await;
        if owners.is_empty() {
            return None;
        }
        let mut failures = Vec::new();
        let mut first_error = None;
        for mut owner in owners {
            if let Err(error) = cleanup_pending_owner(&mut owner).await {
                log_cleanup_retained(&owner, &error, "tab_retry");
                first_error.get_or_insert_with(|| error.clone());
                failures.push(owner);
            }
        }
        self.restore(failures).await;
        Some(first_error.map_or(Ok(()), Err))
    }

    pub async fn lock_operation(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.operation.lock().await
    }

    pub async fn drain(&self) -> Vec<PendingTabCleanup> {
        self.inner
            .lock()
            .await
            .drain()
            .flat_map(|(_, owners)| owners)
            .collect()
    }

    pub async fn restore(&self, owners: Vec<PendingTabCleanup>) {
        let mut inner = self.inner.lock().await;
        for owner in owners {
            let siblings = inner.entry(owner.session.clone()).or_default();
            if !siblings.iter().any(|current| current.id == owner.id) {
                siblings.push(owner);
            }
        }
    }

    async fn retry_session(&self, session: &str) -> Result<(), BrowserError> {
        let owners = self.inner.lock().await.remove(session).unwrap_or_default();
        let mut failures = Vec::new();
        let mut first_error = None;
        for mut owner in owners {
            if let Err(error) = cleanup_pending_owner(&mut owner).await {
                log_cleanup_retained(&owner, &error, "launch_retry");
                first_error.get_or_insert_with(|| error.clone());
                failures.push(owner);
            }
        }
        self.restore(failures).await;
        first_error.map_or(Ok(()), Err)
    }

    async fn take_tab(&self, tab_id: &str) -> Vec<PendingTabCleanup> {
        let mut inner = self.inner.lock().await;
        let sessions = inner.keys().cloned().collect::<Vec<_>>();
        let mut owners = Vec::new();
        for session in sessions {
            let Some(mut siblings) = inner.remove(&session) else {
                continue;
            };
            let (matched, remaining): (Vec<_>, Vec<_>) =
                siblings.drain(..).partition(|owner| owner.tab_id == tab_id);
            owners.extend(matched);
            if !remaining.is_empty() {
                inner.insert(session, remaining);
            }
        }
        owners
    }

    fn next_owner_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    }
}

impl BrowserSessionManager {
    pub(super) async fn close_control(&self, tab_id: &str) {
        let gate = self.controls.lock().await.remove(tab_id);
        if let Some(gate) = gate {
            gate.close().await;
        }
    }

    pub(super) async fn cleanup_or_retain_tab_handle(
        &self,
        handle: TabRuntimeHandle,
        close_target: bool,
    ) -> Result<(), BrowserError> {
        self.tab_cleanups.cleanup_handle(handle, close_target).await
    }
}

fn find_owner_mut<'a>(
    inner: &'a mut CleanupMap,
    token: &TabCleanupToken,
) -> Option<&'a mut PendingTabCleanup> {
    inner
        .get_mut(&token.session)?
        .iter_mut()
        .find(|owner| owner.id == token.id)
}

fn take_owner(inner: &mut CleanupMap, token: &TabCleanupToken) -> Option<PendingTabCleanup> {
    let owners = inner.get_mut(&token.session)?;
    let position = owners.iter().position(|owner| owner.id == token.id)?;
    let owner = owners.remove(position);
    let remove_session = owners.is_empty();
    if remove_session {
        inner.remove(&token.session);
    }
    Some(owner)
}

fn log_cleanup_retained(owner: &PendingTabCleanup, error: &BrowserError, stage: &str) {
    tracing::warn!(
        target: "iyw_claw_browser",
        cleanup_owner_id = owner.id,
        browser_tab_id = %owner.tab_id,
        session = %owner.session,
        runtime_generation = owner.runtime_generation,
        target_id = owner.target_id.as_deref(),
        cleanup_stage = stage,
        error_code = ?error.code,
        error_message = %error.message,
        "browser tab cleanup owner retained for retry"
    );
}

fn cleanup_owner_missing() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserInternal,
        "The browser tab cleanup owner was not available",
    )
}

fn cleanup_still_pending() -> BrowserError {
    BrowserError::new(
        BrowserErrorCode::BrowserRuntimeUnavailable,
        "A previous browser tab session is still being cleaned up",
    )
    .retryable(true)
}
