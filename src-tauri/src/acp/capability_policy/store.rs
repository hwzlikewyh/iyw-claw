use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{watch, Mutex, RwLock};

use super::cache::{normalize_etag, CachedCapabilityPolicy, CapabilityPolicyCache};
use super::dto::{CapabilityPolicySnapshot, SnapshotValidationRules};
use super::error::CapabilityPolicyError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicySnapshotSource {
    Remote,
    TrustedCache,
    RevisionRollback,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySnapshotView {
    pub snapshot: Option<CapabilityPolicySnapshot>,
    pub etag: Option<String>,
    pub source: PolicySnapshotSource,
}

#[derive(Debug, Clone)]
struct StoreState {
    cached: Option<CachedCapabilityPolicy>,
    source: PolicySnapshotSource,
}

#[derive(Clone)]
pub struct CapabilityPolicyStore {
    cache: Arc<dyn CapabilityPolicyCache>,
    changes: watch::Sender<u64>,
    mutation_lock: Arc<Mutex<()>>,
    rules: SnapshotValidationRules,
    state: Arc<RwLock<StoreState>>,
}

impl CapabilityPolicyStore {
    pub fn new(cache: Arc<dyn CapabilityPolicyCache>, rules: SnapshotValidationRules) -> Self {
        let (changes, _) = watch::channel(0);
        Self {
            cache,
            changes,
            mutation_lock: Arc::new(Mutex::new(())),
            rules,
            state: Arc::new(RwLock::new(StoreState {
                cached: None,
                source: PolicySnapshotSource::Missing,
            })),
        }
    }

    pub async fn restore_cache(&self) -> Result<(), CapabilityPolicyError> {
        let _mutation = self.mutation_lock.lock().await;
        let Some(cached) = self.cache.load().await? else {
            return Ok(());
        };
        let mut state = self.state.write().await;
        state.cached = Some(cached);
        state.source = PolicySnapshotSource::TrustedCache;
        drop(state);
        self.notify_change();
        Ok(())
    }

    pub async fn view(&self) -> PolicySnapshotView {
        let state = self.state.read().await;
        PolicySnapshotView {
            snapshot: state.cached.as_ref().map(|value| value.snapshot.clone()),
            etag: state.cached.as_ref().and_then(|value| value.etag.clone()),
            source: state.source,
        }
    }

    pub async fn accept_remote(
        &self,
        snapshot: CapabilityPolicySnapshot,
        etag: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<PolicySnapshotView, CapabilityPolicyError> {
        snapshot.validate_at(now, self.rules)?;
        let next = CachedCapabilityPolicy {
            snapshot,
            etag: normalize_etag(etag)?,
        };
        let _mutation = self.mutation_lock.lock().await;
        let current = self.state.read().await.cached.clone();
        validate_revision(current.as_ref(), &next)?;
        self.cache.save(&next).await?;
        let mut state = self.state.write().await;
        state.cached = Some(next);
        state.source = PolicySnapshotSource::Remote;
        let view = view_from_state(&state);
        drop(state);
        self.notify_change();
        Ok(view)
    }

    pub async fn record_not_modified(&self) -> Result<PolicySnapshotView, CapabilityPolicyError> {
        let _mutation = self.mutation_lock.lock().await;
        let mut state = self.state.write().await;
        if state.cached.is_none() {
            return Err(CapabilityPolicyError::NotModifiedWithoutSnapshot);
        }
        state.source = PolicySnapshotSource::Remote;
        let view = view_from_state(&state);
        Ok(view)
    }

    pub async fn record_refresh_failure(&self) {
        let _mutation = self.mutation_lock.lock().await;
        let mut state = self.state.write().await;
        if state.source != PolicySnapshotSource::RevisionRollback {
            state.source = if state.cached.is_some() {
                PolicySnapshotSource::TrustedCache
            } else {
                PolicySnapshotSource::Missing
            };
        }
    }

    pub async fn record_revision_rollback(&self) {
        let _mutation = self.mutation_lock.lock().await;
        let mut state = self.state.write().await;
        state.source = if state.cached.is_some() {
            PolicySnapshotSource::RevisionRollback
        } else {
            PolicySnapshotSource::Missing
        };
    }

    pub(super) fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub(super) fn notify_change(&self) {
        self.changes
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }
}

fn validate_revision(
    current: Option<&CachedCapabilityPolicy>,
    next: &CachedCapabilityPolicy,
) -> Result<(), CapabilityPolicyError> {
    let Some(current) = current else {
        return Ok(());
    };
    if next.snapshot.revision < current.snapshot.revision {
        return Err(CapabilityPolicyError::RevisionRollback);
    }
    if next.snapshot.revision == current.snapshot.revision
        && !next.snapshot.same_permissions(&current.snapshot)
    {
        return Err(CapabilityPolicyError::RevisionCollision);
    }
    Ok(())
}

fn view_from_state(state: &StoreState) -> PolicySnapshotView {
    PolicySnapshotView {
        snapshot: state.cached.as_ref().map(|value| value.snapshot.clone()),
        etag: state.cached.as_ref().and_then(|value| value.etag.clone()),
        source: state.source,
    }
}
