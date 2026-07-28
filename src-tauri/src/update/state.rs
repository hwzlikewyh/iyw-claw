//! Shared, process-lived source of truth for an in-flight / completed app
//! self-update.
//!
//! The upgrade UI used to keep the only copy of the download progress in React
//! component state, so navigating away from the settings page (or closing it)
//! unmounted the component and lost the progress while the download kept
//! running underneath. This module moves that state into the backend — the
//! renderer becomes a pure subscriber that re-syncs from a snapshot on mount —
//! mirroring how `pet_state` lets a freshly-opened window pick up the current
//! ambient state.
//!
//! Both runtimes write here through the same small API:
//!   * desktop drives `tauri-plugin-updater` from `commands::app_update`;
//!   * the standalone server drives the in-place swap from
//!     `web::handlers::app_update`.
//!
//! Both emit the same [`APP_UPDATE_STATE_CHANNEL`] event (the full snapshot on
//! every transition) and answer the same snapshot query, so the frontend has a
//! single mode-agnostic code path.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::update::runtime::UpdateCapability;
use crate::web::event_bridge::{emit_event, EventEmitter};

/// Channel name shared by the live event and the snapshot query.
pub const APP_UPDATE_STATE_CHANNEL: &str = "app_update_state";

/// Minimum gap between emitted `Downloading` frames. The snapshot is updated on
/// every chunk (so a mount-time query is always exact), but the live event is
/// throttled so a multi-MB download doesn't emit thousands of frames.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// Coarse lifecycle the frontend renders. The server's finer phases
/// (verify / extract / swap) and the desktop's post-download install both
/// collapse to [`AppUpdateLifecycle::Installing`] — the UI shows a determinate
/// bar only while `Downloading` and an indeterminate "installing…" otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdateLifecycle {
    Idle,
    Checking,
    Available,
    Downloading,
    Verifying,
    Installing,
    ReadyToRestart,
    Restarting,
    Error,
}

/// Full update snapshot, emitted on every transition and returned by the
/// snapshot query. A flat struct (rather than a tagged enum) keeps the
/// TypeScript mirror a plain discriminated-by-`status` object and dodges
/// serde-flatten edge cases.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateState {
    /// Monotonic across every transition (bumped even for throttled-away
    /// frames). The renderer keeps only the highest-`seq` value it has seen, so
    /// a late-arriving snapshot can't clobber a fresher live event — and a
    /// post-throttle snapshot is always strictly newer than the last event,
    /// never mistaken for stale.
    pub seq: u64,
    pub status: AppUpdateLifecycle,
    /// Bytes downloaded so far (`Downloading` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded: Option<u64>,
    /// Total bytes from `Content-Length`, if the source reported it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Target version once known (desktop: the manifest check; server: filled
    /// in when the swap completes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforce_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pub_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    /// Relaunch delay (ms) for the renderer countdown — set on `ReadyToRestart`
    /// in server mode so it matches the supervisor's actual relaunch delay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_delay_ms: Option<u64>,
    /// Supervisor probation window (s) — set on `ReadyToRestart` in server
    /// mode; the renderer watches the running version across it before
    /// declaring success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trial_seconds: Option<u64>,
    /// How a server restart is carried out (`supervised` / `reexec`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<UpdateCapability>,
    /// Raw error message (`Error` only). The frontend classifies it for
    /// display via `normalizeAppUpdateError`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AppUpdateState {
    fn idle() -> Self {
        Self {
            seq: 0,
            status: AppUpdateLifecycle::Idle,
            downloaded: None,
            total: None,
            version: None,
            release_id: None,
            channel: None,
            update_policy: None,
            enforce_after: None,
            notes: None,
            pub_date: None,
            last_checked_at: None,
            restart_delay_ms: None,
            trial_seconds: None,
            capability: None,
            error: None,
        }
    }

    /// Clear every per-operation field, preserving only `seq` (the caller bumps
    /// it). Used when entering a terminal/clean state so stale progress or a
    /// previous error never leaks into the new status.
    fn clear_operation_fields(&mut self) {
        self.downloaded = None;
        self.total = None;
        self.version = None;
        self.release_id = None;
        self.channel = None;
        self.update_policy = None;
        self.enforce_after = None;
        self.notes = None;
        self.pub_date = None;
        self.restart_delay_ms = None;
        self.trial_seconds = None;
        self.capability = None;
        self.error = None;
    }
}

pub type AppUpdateStateHandle = Arc<RwLock<AppUpdateState>>;

pub fn new_handle() -> AppUpdateStateHandle {
    Arc::new(RwLock::new(AppUpdateState::idle()))
}

/// Read the current snapshot. Recovers a poisoned lock (a writer panicked
/// mid-update): the worst case is a momentarily stale snapshot, never a wedged
/// reader.
pub fn snapshot(handle: &AppUpdateStateHandle) -> AppUpdateState {
    handle
        .read()
        .map(|g| g.clone())
        .unwrap_or_else(|p| p.into_inner().clone())
}

/// Atomically claim the single system-op slot for a relaunch: transition to
/// `Restarting` IFF an update is staged (`ReadyToRestart`). Returns true if
/// claimed. Used as the authority check by BOTH the desktop `restart_app`
/// command and the server `restart_impl` handler. Because the check-and-set is
/// one `update_state` critical section, it also serializes concurrent restart
/// clicks and excludes a racing rollback/perform (whoever claims first wins).
pub fn try_claim_restart(handle: &AppUpdateStateHandle, emitter: &EventEmitter) -> bool {
    claim_restarting(handle, emitter, &[AppUpdateLifecycle::ReadyToRestart])
}

/// Atomically claim the single system-op slot for a rollback: transition to
/// `Restarting` IFF the lifecycle is settled (`Idle`/`Error`). Returns true if
/// claimed. Serializing against [`try_begin`] in one critical section is what
/// guarantees a rollback and a download can never both proceed — whoever claims
/// the state first wins, so the loser backs off before touching the op-lock or
/// the `.bak`.
pub fn try_claim_rollback(handle: &AppUpdateStateHandle, emitter: &EventEmitter) -> bool {
    claim_restarting(
        handle,
        emitter,
        &[
            AppUpdateLifecycle::Idle,
            AppUpdateLifecycle::Available,
            AppUpdateLifecycle::Error,
        ],
    )
}

fn claim_restarting(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    allowed_from: &[AppUpdateLifecycle],
) -> bool {
    let snap = {
        let mut g = handle.write().unwrap_or_else(|p| p.into_inner());
        if !allowed_from.contains(&g.status) {
            return false;
        }
        g.seq += 1;
        g.clear_operation_fields();
        g.status = AppUpdateLifecycle::Restarting;
        g.clone()
    };
    emit_event(emitter, APP_UPDATE_STATE_CHANNEL, &snap);
    true
}

/// Apply `f` under the write lock, bump `seq`, then emit the resulting
/// snapshot. Used for every discrete transition (the per-chunk progress path
/// is throttled separately by [`ProgressEmitter`]).
fn mutate(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    f: impl FnOnce(&mut AppUpdateState),
) -> AppUpdateState {
    let snap = {
        let mut g = handle.write().unwrap_or_else(|p| p.into_inner());
        g.seq += 1;
        f(&mut g);
        g.clone()
    };
    emit_event(emitter, APP_UPDATE_STATE_CHANNEL, &snap);
    snap
}

/// Atomically start a fresh download **iff** the current state is terminal
/// (`Idle` / `Error`). Returns `(started, snapshot)`:
///   * `started == true` — the caller now owns the download and must drive it;
///   * `started == false` — a download is already in flight (or staged), so a
///     second click / second tab simply attaches to the in-flight snapshot.
///
/// The check-and-set happens in one write-lock critical section, so two
/// concurrent callers can never both start.
pub fn try_begin(handle: &AppUpdateStateHandle, emitter: &EventEmitter) -> (bool, AppUpdateState) {
    let snap = {
        let mut g = handle.write().unwrap_or_else(|p| p.into_inner());
        if !matches!(
            g.status,
            AppUpdateLifecycle::Idle | AppUpdateLifecycle::Available | AppUpdateLifecycle::Error
        ) {
            return (false, g.clone());
        }
        g.seq += 1;
        g.clear_operation_fields();
        g.status = AppUpdateLifecycle::Downloading;
        g.downloaded = Some(0);
        g.clone()
    };
    emit_event(emitter, APP_UPDATE_STATE_CHANNEL, &snap);
    (true, snap)
}

pub fn try_begin_check(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
) -> (bool, AppUpdateState) {
    let snap = {
        let mut state = handle.write().unwrap_or_else(|error| error.into_inner());
        if !matches!(
            state.status,
            AppUpdateLifecycle::Idle | AppUpdateLifecycle::Available | AppUpdateLifecycle::Error
        ) {
            return (false, state.clone());
        }
        state.seq += 1;
        state.status = AppUpdateLifecycle::Checking;
        state.downloaded = None;
        state.total = None;
        state.restart_delay_ms = None;
        state.trial_seconds = None;
        state.capability = None;
        state.error = None;
        state.clone()
    };
    emit_event(emitter, APP_UPDATE_STATE_CHANNEL, &snap);
    (true, snap)
}

pub fn set_idle_checked(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    checked_at: String,
) -> AppUpdateState {
    mutate(handle, emitter, |state| {
        state.clear_operation_fields();
        state.status = AppUpdateLifecycle::Idle;
        state.last_checked_at = Some(checked_at);
    })
}

/// Dismiss the exact optional offer currently visible. The status, policy and
/// version are checked and cleared under one lock so a stale renderer cannot
/// suppress a required, replaced, downloading, or already-staged update.
pub fn try_dismiss_optional_offer(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    version: &str,
) -> bool {
    let snap = {
        let mut state = handle.write().unwrap_or_else(|error| error.into_inner());
        if state.status != AppUpdateLifecycle::Available
            || state.update_policy.as_deref() != Some("optional")
            || state.version.as_deref() != Some(version)
        {
            return false;
        }
        state.seq += 1;
        state.clear_operation_fields();
        state.status = AppUpdateLifecycle::Idle;
        state.clone()
    };
    emit_event(emitter, APP_UPDATE_STATE_CHANNEL, &snap);
    true
}

pub fn set_available(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    info: &crate::update::release::AppUpdateInfo,
    checked_at: String,
) -> AppUpdateState {
    mutate(handle, emitter, |state| {
        state.clear_operation_fields();
        state.status = AppUpdateLifecycle::Available;
        state.version = Some(info.version.clone());
        state.release_id = info.release_id.clone();
        state.channel = Some(info.channel.clone());
        state.update_policy = Some(info.update_policy.clone());
        state.enforce_after = info.enforce_after.clone();
        state.notes = Some(info.body.clone());
        state.pub_date = info.date.clone();
        state.last_checked_at = Some(checked_at);
    })
}

pub fn set_download_target(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    version: String,
) -> AppUpdateState {
    mutate(handle, emitter, |state| {
        state.status = AppUpdateLifecycle::Downloading;
        state.version = Some(version);
    })
}

/// Revert a just-claimed download back to `Idle` — but only if the state is
/// still exactly the one `try_begin` claimed (`claimed_seq` matches). Used when
/// a caller wins the [`try_begin`] race but then cannot acquire the op-lock (a
/// restart/rollback is in flight): it must yield the claim without clobbering
/// whatever that other operation has since written.
pub fn abort_claim(handle: &AppUpdateStateHandle, emitter: &EventEmitter, claimed_seq: u64) {
    let snap = {
        let mut g = handle.write().unwrap_or_else(|p| p.into_inner());
        if g.seq != claimed_seq {
            // A concurrent operation already moved the state on — leave it.
            return;
        }
        g.seq += 1;
        g.clear_operation_fields();
        g.status = AppUpdateLifecycle::Idle;
        g.clone()
    };
    emit_event(emitter, APP_UPDATE_STATE_CHANNEL, &snap);
}

/// Transition to the indeterminate finalize phase (server verify/extract/swap,
/// or desktop install after the bytes have landed).
pub fn set_installing(handle: &AppUpdateStateHandle, emitter: &EventEmitter) -> AppUpdateState {
    mutate(handle, emitter, |s| {
        s.status = AppUpdateLifecycle::Installing;
        s.downloaded = None;
        s.total = None;
    })
}

/// The updater has received all bytes and is validating the signed artifact.
pub fn set_verifying(handle: &AppUpdateStateHandle, emitter: &EventEmitter) -> AppUpdateState {
    mutate(handle, emitter, |state| {
        state.status = AppUpdateLifecycle::Verifying;
        state.downloaded = None;
        state.total = None;
    })
}

/// The new bytes are staged and the app is ready to relaunch into them.
/// `restart_delay_ms` / `trial_seconds` / `capability` are server-only. Desktop
/// reaches this state on platforms where the updater can stage without exiting;
/// Windows NSIS exits from `Installing` instead.
pub fn set_ready(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    version: Option<String>,
    restart_delay_ms: Option<u64>,
    trial_seconds: Option<u64>,
    capability: Option<UpdateCapability>,
) -> AppUpdateState {
    mutate(handle, emitter, |s| {
        s.status = AppUpdateLifecycle::ReadyToRestart;
        s.downloaded = None;
        s.total = None;
        s.error = None;
        s.version = version;
        s.restart_delay_ms = restart_delay_ms;
        s.trial_seconds = trial_seconds;
        s.capability = capability;
    })
}

/// The operation failed. The message is surfaced verbatim; the frontend
/// classifies it for the toast.
pub fn set_error(
    handle: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    message: impl Into<String>,
) -> AppUpdateState {
    mutate(handle, emitter, |s| {
        s.status = AppUpdateLifecycle::Error;
        s.downloaded = None;
        s.total = None;
        s.restart_delay_ms = None;
        s.trial_seconds = None;
        s.capability = None;
        s.error = Some(message.into());
    })
}

/// Throttled writer for the per-chunk download progress. Owns the last-emit
/// timestamp so a hot download loop updates the snapshot on every chunk but
/// only emits a live frame every [`PROGRESS_EMIT_INTERVAL`].
pub struct ProgressEmitter {
    handle: AppUpdateStateHandle,
    emitter: EventEmitter,
    last_emit: std::sync::Mutex<Option<Instant>>,
}

impl ProgressEmitter {
    pub fn new(handle: AppUpdateStateHandle, emitter: EventEmitter) -> Self {
        Self {
            handle,
            emitter,
            last_emit: std::sync::Mutex::new(None),
        }
    }

    /// Record download progress. Always updates the snapshot; emits a live
    /// frame on the first byte, on completion, and at most once per interval in
    /// between. A late frame after the operation already left `Downloading` /
    /// `Installing` is ignored so it can't resurrect the bar.
    pub fn downloading(&self, downloaded: u64, total: Option<u64>) {
        let snap = {
            let mut g = self.handle.write().unwrap_or_else(|p| p.into_inner());
            if !matches!(
                g.status,
                AppUpdateLifecycle::Downloading | AppUpdateLifecycle::Installing
            ) {
                return;
            }
            g.seq += 1;
            g.status = AppUpdateLifecycle::Downloading;
            g.downloaded = Some(downloaded);
            if total.is_some() {
                g.total = total;
            }
            g.clone()
        };

        let complete = matches!(total, Some(t) if downloaded >= t && t > 0);
        let due = {
            let mut last = self.last_emit.lock().unwrap_or_else(|p| p.into_inner());
            let now = Instant::now();
            let due = downloaded == 0
                || complete
                || last.is_none_or(|t| now.duration_since(t) >= PROGRESS_EMIT_INTERVAL);
            if due {
                *last = Some(now);
            }
            due
        };
        if due {
            emit_event(&self.emitter, APP_UPDATE_STATE_CHANNEL, &snap);
        }
    }

    /// Convenience: transition to the install phase (the download-finished
    /// callback).
    pub fn installing(&self) {
        set_installing(&self.handle, &self.emitter);
    }

    pub fn verifying(&self) {
        set_verifying(&self.handle, &self.emitter);
    }
}
