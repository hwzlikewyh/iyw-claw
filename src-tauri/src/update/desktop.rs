//! 桌面更新先预下载并缓存，由用户操作或受保护的后台空闲流程安装。

use std::sync::Mutex;

use sea_orm::DatabaseConnection;
use tauri_plugin_updater::Update;

use crate::app_error::AppCommandError;
use crate::web::event_bridge::EventEmitter;

use super::desktop_cache;
use super::offer::{self, Identity};
use super::preferences;
use super::release::{self, AppUpdateInfo, CheckReason, DesktopUpdateRequest};
use super::state::{self, AppUpdateState, AppUpdateStateHandle};

// 只保存 SDK 的安装上下文，完整更新包留在磁盘，不常驻内存。
static PREPARED_UPDATE: Mutex<Option<(Identity, Update, bool)>> = Mutex::new(None);

pub struct Task {
    pub(super) app: tauri::AppHandle,
    pub(super) conn: DatabaseConnection,
    pub(super) handle: AppUpdateStateHandle,
}

impl Task {
    pub fn new(
        app: &tauri::AppHandle,
        conn: &DatabaseConnection,
        handle: &AppUpdateStateHandle,
    ) -> Self {
        Self {
            app: app.clone(),
            conn: conn.clone(),
            handle: handle.clone(),
        }
    }

    pub(super) fn emitter(&self) -> EventEmitter {
        EventEmitter::Tauri(self.app.clone())
    }

    pub(super) fn fail(&self, error: String) {
        tracing::error!(%error, "[app-update] desktop update failed");
        state::set_error(&self.handle, &self.emitter(), error);
    }
}

pub(super) async fn install_silently(task: &Task, identity: &Identity) -> Result<bool, String> {
    let update = checked_silent_update(task, identity).await?;
    let Some(bytes) = desktop_cache::load(&task.app, &update).await? else {
        return Err("Prepared update is missing or invalid; check for updates again".to_string());
    };
    let data_dir = crate::system_skills::data_dir_from_env();
    if let Err(error) = super::digest::record_before_update(&data_dir).await {
        tracing::warn!(%error, "[app-update] failed to record pre-update managed-root digest");
    }
    let Some(_guards) = super::silent::claim(task, identity).await? else {
        return Ok(false);
    };
    if !offer::try_begin_prepared(&task.handle, &task.emitter(), identity) {
        return Ok(false);
    }
    tracing::info!(version = %identity.version, "[app-update] silent installation started");
    if let Err(error) = update
        .install(bytes)
        .map_err(|error| error.to_string())
        .and_then(|_| finish_install(task, update.version))
    {
        task.fail(error);
        return Ok(true);
    }
    restart_after_silent_install(task).await?;
    Ok(true)
}

async fn restart_after_silent_install(task: &Task) -> Result<(), String> {
    // Windows 由 NSIS /R 重启；其他平台安装后重新检查，恢复使用就延后。
    #[cfg(not(target_os = "windows"))]
    {
        let preferences = preferences::load(&task.conn)
            .await
            .map_err(|error| error.to_string())?;
        if preferences.silent_auto_update
            && super::silent_safety::blocker(&task.app).is_none()
            && super::silent_safety::task_blocker(&task.app)
                .await?
                .is_none()
            && state::try_claim_restart(&task.handle, &task.emitter())
        {
            tracing::info!("[app-update] restarting after silent installation");
            task.app.restart();
        }
    }
    #[cfg(target_os = "windows")]
    let _ = task;
    Ok(())
}

async fn checked_silent_update(task: &Task, identity: &Identity) -> Result<Update, String> {
    let prepared = PREPARED_UPDATE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .filter(|(expected, _, silent)| expected == identity && *silent)
        .map(|(_, update, _)| update.clone());
    if let Some(update) = prepared {
        return Ok(update);
    }
    let mut preferences = preferences::load(&task.conn)
        .await
        .map_err(|error| error.to_string())?;
    preferences.silent_auto_update = true;
    let request = DesktopUpdateRequest {
        app: &task.app,
        preferences: &preferences,
        access_token: None,
    };
    let (_, update) = release::check_desktop_update(&request, CheckReason::Automatic)
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Prepared release is no longer available; check for updates again")?;
    let extensions = release::extensions(&update).map_err(|error| error.to_string())?;
    offer::validate_checked_update(&update, &extensions, identity)?;
    *PREPARED_UPDATE
        .lock()
        .unwrap_or_else(|error| error.into_inner()) =
        Some((identity.clone(), update.clone(), true));
    Ok(update)
}

pub fn prefetch(task: Task, checked: (AppUpdateInfo, Update), silent: bool) {
    let (info, mut update) = checked;
    let identity = Identity {
        version: info.version,
        release_id: info.release_id,
        channel: info.channel,
    };
    if !offer::try_prefetch(&task.handle, &task.emitter(), &identity) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tracing::info!(version = %identity.version, "[app-update] preparing update in background");
        match load_or_download(&task, &mut update).await {
            Ok(_) => {
                *PREPARED_UPDATE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some((identity, update, silent));
                offer::set_downloaded(&task.handle, &task.emitter());
                tracing::info!("[app-update] download ready; waiting for user to install");
            }
            Err(error) => task.fail(error),
        }
    });
}

pub fn install(task: Task) -> Result<AppUpdateState, AppCommandError> {
    let (started, snapshot, identity) = offer::try_begin(&task.handle, &task.emitter());
    if !started {
        return Ok(snapshot);
    }
    let Some(identity) = identity else {
        task.fail("Available update metadata is incomplete".to_string());
        return Err(AppCommandError::configuration_invalid(
            "Available update metadata is incomplete",
        ));
    };
    tauri::async_runtime::spawn(async move {
        if let Err(error) = install_impl(&task, &identity).await {
            task.fail(error);
        }
    });
    Ok(snapshot)
}

async fn install_impl(task: &Task, identity: &Identity) -> Result<(), String> {
    let preferences = preferences::load(&task.conn)
        .await
        .map_err(|error| error.to_string())?;
    if preferences.channel.as_str() != identity.channel {
        return Err("The selected update channel changed before installation".to_string());
    }
    let mut update = checked_update(task, identity, &preferences).await?;
    let bytes = load_or_download(task, &mut update).await?;
    let data_dir = crate::system_skills::data_dir_from_env();
    if let Err(error) = super::digest::record_before_update(&data_dir).await {
        tracing::warn!(%error, "[app-update] failed to record pre-update managed-root digest");
    }
    state::set_installing(&task.handle, &task.emitter());
    tracing::info!(version = %update.version, "[app-update] installing cached update after user request");
    update.install(bytes).map_err(|error| error.to_string())?;
    finish_install(task, update.version)
}

async fn checked_update(
    task: &Task,
    identity: &Identity,
    preferences: &preferences::UpdatePreferences,
) -> Result<Update, String> {
    let prepared = PREPARED_UPDATE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .filter(|(expected, _, _)| expected == identity)
        .map(|(_, update, _)| update.clone());
    if let Some(update) = prepared {
        return Ok(update);
    }
    let request = DesktopUpdateRequest {
        app: &task.app,
        preferences,
        access_token: None,
    };
    let (_, update) = release::check_desktop_update(&request, CheckReason::Manual)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No update available".to_string())?;
    let extensions = release::extensions(&update).map_err(|error| error.to_string())?;
    offer::validate_checked_update(&update, &extensions, identity)?;
    Ok(update)
}

async fn load_or_download(task: &Task, update: &mut Update) -> Result<Vec<u8>, String> {
    state::set_verifying(&task.handle, &task.emitter());
    if let Some(bytes) = desktop_cache::load(&task.app, update).await? {
        return Ok(bytes);
    }
    let access_token = crate::commands::iyw_account::iyw_account_access_token_core(&task.conn)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Sign in to download the update".to_string())?;
    update.headers.insert(
        "token",
        access_token
            .expose()
            .parse()
            .map_err(|_| "Invalid update authorization header")?,
    );
    state::set_download_target(&task.handle, &task.emitter(), update.version.clone());
    let progress = state::ProgressEmitter::new(task.handle.clone(), task.emitter());
    let mut downloaded = 0_u64;
    let bytes = update
        .download(
            |chunk_len, content_len| {
                downloaded += chunk_len as u64;
                progress.downloading(downloaded, content_len);
            },
            || progress.verifying(),
        )
        .await
        .map_err(|error| format!("Update download failed: {error}"))?;
    update.headers.remove("token");
    desktop_cache::store(&task.app, update, &bytes).await?;
    Ok(bytes)
}

#[cfg(target_os = "windows")]
fn finish_install(_task: &Task, _version: String) -> Result<(), String> {
    Err("NSIS updater returned without terminating the application".to_string())
}

#[cfg(not(target_os = "windows"))]
fn finish_install(task: &Task, version: String) -> Result<(), String> {
    state::set_ready(
        &task.handle,
        &task.emitter(),
        Some(version),
        None,
        None,
        None,
    );
    Ok(())
}
