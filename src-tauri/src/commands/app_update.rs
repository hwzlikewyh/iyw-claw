//! Desktop (Tauri) self-update commands.
//!
//! 发现新版本后由 Rust 驱动预下载并缓存，进度保存在共享
//! [`crate::update::state`] 中，页面卸载、切换或刷新不会中断下载。
//! 用户通过 `perform_app_update` 安装已准备的更新，前端只订阅状态。
//!
//! These mirror the server-mode axum handlers in
//! `web::handlers::app_update` (same command names, so the transport layer
//! routes `perform_app_update` / `restart_app` / `app_update_state` to the
//! right place per runtime), but drive the platform updater instead of the
//! in-place tarball swap.

use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::acp::manager::ConnectionManager;
use crate::app_error::AppCommandError;
use crate::db::AppDatabase;
use crate::update::desktop;
use crate::update::preferences::{self, UpdatePreferences, UpdatePreferencesPatch};
use crate::update::release::{self, AppUpdateInfo, CheckReason};
use crate::update::state::{self as update_state, AppUpdateState, AppUpdateStateHandle};
use crate::web::event_bridge::EventEmitter;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheckResult {
    pub current_version: String,
    pub update: Option<AppUpdateInfo>,
}

/// Current update snapshot, for the renderer to re-sync on mount.
#[tauri::command]
pub fn app_update_state(state: tauri::State<'_, AppUpdateStateHandle>) -> AppUpdateState {
    update_state::snapshot(state.inner())
}

#[tauri::command]
pub async fn check_app_update(
    app: tauri::AppHandle,
    db: tauri::State<'_, AppDatabase>,
    state: tauri::State<'_, AppUpdateStateHandle>,
) -> Result<AppUpdateCheckResult, AppCommandError> {
    check_desktop_update_core(&app, &db.conn, state.inner(), CheckReason::Manual).await
}

pub(crate) async fn check_desktop_update_core(
    app: &tauri::AppHandle,
    conn: &DatabaseConnection,
    state: &AppUpdateStateHandle,
    reason: CheckReason,
) -> Result<AppUpdateCheckResult, AppCommandError> {
    let preferences = preferences::load(conn).await?;
    let checked_channel = preferences.channel;
    let emitter = EventEmitter::Tauri(app.clone());
    let (started, _) = update_state::try_begin_check(state, &emitter);
    if !started {
        return Err(AppCommandError::already_exists(
            "An application update operation is already in progress",
        ));
    }

    let request = release::DesktopUpdateRequest {
        app,
        preferences: &preferences,
        access_token: None,
    };
    let result = release::check_desktop_update(&request, reason).await;
    let update = match result {
        Ok(update) => update,
        Err(error) => {
            record_check_failure(conn, state, &emitter, checked_channel, &error).await;
            return Err(error);
        }
    };
    let info = update.as_ref().map(|(info, _)| info.clone());
    let result =
        finish_successful_check(conn, state, &emitter, checked_channel, reason, info).await;
    if let Err(error) = &result {
        update_state::set_error(state, &emitter, error.to_string());
    }
    if let Some(checked) = update.filter(|_| result.as_ref().is_ok_and(|r| r.update.is_some())) {
        desktop::prefetch(desktop::Task::new(app, conn, state), checked);
    }
    result
}

async fn finish_successful_check(
    conn: &DatabaseConnection,
    state: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    checked_channel: preferences::UpdateChannel,
    reason: CheckReason,
    update: Option<AppUpdateInfo>,
) -> Result<AppUpdateCheckResult, AppCommandError> {
    let release_id = update.as_ref().and_then(|value| value.release_id.clone());
    let (preferences, applies) =
        preferences::record_check_success(conn, checked_channel, release_id).await?;
    let checked_at = preferences.last_checked_at.clone().unwrap_or_default();
    if !applies {
        tracing::info!("[app-update] discarded check after channel changed");
        update_state::set_idle_checked(state, emitter, checked_at);
        return Ok(AppUpdateCheckResult {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            update: None,
        });
    }
    let visible = update.filter(|value| {
        reason == CheckReason::Manual
            || !preferences.suppresses(&value.version, value.update_policy == "required")
    });
    match &visible {
        Some(value) => {
            tracing::info!(
                version = %value.version,
                channel = %value.channel,
                policy = %value.update_policy,
                reason = reason.as_str(),
                "[app-update] update available"
            );
            update_state::set_available(state, emitter, value, checked_at);
        }
        None => {
            tracing::info!(reason = reason.as_str(), "[app-update] no visible update");
            update_state::set_idle_checked(state, emitter, checked_at);
        }
    }
    Ok(AppUpdateCheckResult {
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        update: visible,
    })
}

async fn record_check_failure(
    conn: &DatabaseConnection,
    state: &AppUpdateStateHandle,
    emitter: &EventEmitter,
    checked_channel: preferences::UpdateChannel,
    error: &AppCommandError,
) {
    match preferences::record_check_failure(conn, checked_channel).await {
        Ok((latest, false)) => {
            tracing::info!("[app-update] ignored failed check after channel changed");
            update_state::set_idle_checked(
                state,
                emitter,
                latest.last_checked_at.unwrap_or_default(),
            );
            return;
        }
        Err(save_error) => {
            tracing::warn!("[app-update] failed to persist check failure: {save_error}");
        }
        Ok(_) => {}
    }
    update_state::set_error(state, emitter, error.to_string());
}

#[tauri::command]
pub async fn get_app_update_preferences(
    db: tauri::State<'_, AppDatabase>,
) -> Result<UpdatePreferences, AppCommandError> {
    preferences::load(&db.conn).await
}

#[tauri::command]
pub async fn update_app_update_preferences(
    patch: UpdatePreferencesPatch,
    db: tauri::State<'_, AppDatabase>,
) -> Result<UpdatePreferences, AppCommandError> {
    preferences::patch(&db.conn, patch).await
}

#[tauri::command]
pub async fn skip_app_update(
    version: String,
    db: tauri::State<'_, AppDatabase>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppUpdateStateHandle>,
) -> Result<UpdatePreferences, AppCommandError> {
    let emitter = EventEmitter::Tauri(app);
    if !crate::update::offer::is_current_optional(state.inner(), &version) {
        return Err(AppCommandError::invalid_input(
            "Only the current optional update can be skipped",
        ));
    }
    let preferences = preferences::skip_version(&db.conn, version.clone()).await?;
    if !update_state::try_dismiss_optional_offer(state.inner(), &emitter, &version) {
        return Err(AppCommandError::invalid_input(
            "The optional update changed before it could be skipped",
        ));
    }
    Ok(preferences)
}

#[tauri::command]
pub async fn remind_app_update_later(
    version: String,
    minutes: u32,
    db: tauri::State<'_, AppDatabase>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppUpdateStateHandle>,
) -> Result<UpdatePreferences, AppCommandError> {
    if !(1..=10_080).contains(&minutes) {
        return Err(AppCommandError::invalid_input(
            "Reminder delay must be between 1 minute and 7 days",
        ));
    }
    let emitter = EventEmitter::Tauri(app);
    if !crate::update::offer::is_current_optional(state.inner(), &version) {
        return Err(AppCommandError::invalid_input(
            "Only the current optional update can be postponed",
        ));
    }
    let preferences = preferences::remind_later(&db.conn, version.clone(), minutes).await?;
    if !update_state::try_dismiss_optional_offer(state.inner(), &emitter, &version) {
        return Err(AppCommandError::invalid_input(
            "The optional update changed before it could be postponed",
        ));
    }
    crate::update::scheduler::wake();
    Ok(preferences)
}

/// Install the cached update, downloading first when preparation failed. Returns
/// immediately with the current snapshot; the work runs detached and reports
/// progress via the `app_update_state` event.
#[tauri::command]
pub async fn perform_app_update(
    app: tauri::AppHandle,
    db: tauri::State<'_, AppDatabase>,
    state: tauri::State<'_, AppUpdateStateHandle>,
) -> Result<AppUpdateState, AppCommandError> {
    desktop::install(desktop::Task::new(&app, &db.conn, state.inner()))
}

/// Relaunch into the freshly-installed bytes. Flips the shared snapshot to
/// `Restarting` first so the UI (and any other window) reflects it, then
/// restarts after a short flush delay. `restart()` never returns.
#[tauri::command]
pub async fn restart_app(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppUpdateStateHandle>,
    manager: tauri::State<'_, ConnectionManager>,
) -> Result<(), AppCommandError> {
    let operation_guard = manager.claim_relaunch().await.map_err(|error| {
        AppCommandError::already_exists(error.to_string()).with_detail("active_agent_operations")
    })?;
    let handle = state.inner().clone();
    let emitter = EventEmitter::Tauri(app.clone());
    // Atomically claim the relaunch (flips to `Restarting`) only if an update is
    // genuinely staged — same authority check as the server `restart_impl`.
    // Guards a stale window / direct IPC call from relaunching during
    // idle/error/downloading/installing, and serializes concurrent clicks.
    if !update_state::try_claim_restart(&handle, &emitter) {
        return Err(AppCommandError::invalid_input(
            "No staged update to restart into",
        ));
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let _operation_guard = operation_guard;
        app.restart();
    });
    Ok(())
}

/// Guard settings/storage-triggered relaunches with the same Agent activity
/// check as update restarts. The delay lets the accepted IPC call return so the
/// renderer can persist its restore route before the process exits.
#[tauri::command]
pub async fn relaunch_app(
    app: tauri::AppHandle,
    manager: tauri::State<'_, ConnectionManager>,
) -> Result<(), AppCommandError> {
    let operation_guard = manager.claim_relaunch().await.map_err(|error| {
        AppCommandError::already_exists(error.to_string()).with_detail("active_agent_operations")
    })?;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let _operation_guard = operation_guard;
        app.restart();
    });
    Ok(())
}
