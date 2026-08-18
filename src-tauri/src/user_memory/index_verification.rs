use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::service::IndexRefreshState;
use super::UserMemoryService;

const INDEX_REFRESH_RETRY_COOLDOWN: Duration = Duration::from_secs(5);

impl UserMemoryService {
    pub(super) fn mark_index_unverified(&self) {
        self.index_verified.store(false, Ordering::Release);
    }

    pub(super) fn ensure_index_refresh(&self) {
        if self.index_verified_for_process() {
            return;
        }
        self.schedule_index_refresh_if_due();
    }

    pub(super) fn schedule_degraded_index_refresh_if_due(&self) {
        let now = Instant::now();
        let mut state = self
            .index_refresh_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.requested || state.running {
            return;
        }
        if state.retry_not_before.is_none() {
            state.retry_not_before = Some(now + INDEX_REFRESH_RETRY_COOLDOWN);
            tracing::debug!(
                retry_after_ms = INDEX_REFRESH_RETRY_COOLDOWN.as_millis(),
                "[memory-index] degraded FTS refresh retry armed"
            );
            return;
        }
        if let Some(remaining) = refresh_cooldown_remaining(&state, now) {
            tracing::debug!(
                retry_after_ms = remaining.as_millis(),
                "[memory-index] degraded FTS refresh deferred by retry cooldown"
            );
            return;
        }
        drop(state);
        self.schedule_index_refresh_if_due();
    }

    pub(super) fn index_verified_for_process(&self) -> bool {
        self.index_verified.load(Ordering::Acquire)
    }

    pub(super) fn mark_index_verified_if_idle(&self) {
        let mut state = self
            .index_refresh_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.retry_not_before = None;
        if !state.requested {
            self.index_verified.store(true, Ordering::Release);
        }
    }

    pub(super) fn mark_index_refresh_failed(&self) {
        let mut state = self
            .index_refresh_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.index_verified.store(false, Ordering::Release);
        state.retry_not_before = Some(Instant::now() + INDEX_REFRESH_RETRY_COOLDOWN);
    }

    pub(super) fn request_index_refresh(&self, force: bool) -> bool {
        let now = Instant::now();
        let mut state = self
            .index_refresh_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.index_verified.store(false, Ordering::Release);
        if !force && (state.requested || state.running) {
            return false;
        }
        let cooldown_remaining = (!force)
            .then(|| refresh_cooldown_remaining(&state, now))
            .flatten();
        if let Some(remaining) = cooldown_remaining {
            tracing::debug!(
                retry_after_ms = remaining.as_millis(),
                "[memory-index] source refresh deferred by retry cooldown"
            );
            return false;
        }
        state.requested = true;
        if state.running {
            false
        } else {
            state.running = true;
            true
        }
    }
}

fn refresh_cooldown_remaining(state: &IndexRefreshState, now: Instant) -> Option<Duration> {
    state
        .retry_not_before
        .filter(|retry_at| *retry_at > now)
        .map(|retry_at| retry_at.saturating_duration_since(now))
}
