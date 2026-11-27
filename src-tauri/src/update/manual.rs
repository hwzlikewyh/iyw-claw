//! 用户更新策略独立于全局静默更新设置，窗口关闭后仍由后端执行。
use std::time::Duration;

use serde::Deserialize;
use tauri::Manager;

use super::{
    desktop::{self, Task},
    offer, preferences, silent_safety, state, task_recovery,
};
use crate::acp::manager::ConnectionManager;
use crate::app_error::AppCommandError;
use state::{AppUpdateLifecycle as Lifecycle, AppUpdateState};

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallMode {
    #[default]
    Now,
    AfterTasks,
    ForceResume,
    CancelWaiting,
    RetryRecovery,
}

pub fn install(mut task: Task, mode: InstallMode) -> Result<AppUpdateState, AppCommandError> {
    if mode == InstallMode::CancelWaiting {
        return Ok(super::manual_wait::cancel(&task));
    }
    if mode == InstallMode::RetryRecovery {
        let snapshot = state::snapshot(&task.handle);
        if matches!(
            snapshot.status,
            Lifecycle::Installing | Lifecycle::PreparingRestart | Lifecycle::Restarting
        ) {
            return Err(AppCommandError::already_exists("Update is in progress"));
        }
        tauri::async_runtime::spawn(async move {
            task_recovery::recover_and_report(&task).await;
        });
        return Ok(snapshot);
    }
    if state::snapshot(&task.handle).recovering_tasks {
        return Err(AppCommandError::already_exists(
            "update_blocked:recovery_pending",
        ));
    }
    let (started, snapshot, identity) = offer::try_begin(&task.handle, &task.emitter());
    if !started {
        return Ok(snapshot);
    }
    let identity = identity
        .ok_or_else(|| AppCommandError::configuration_invalid("Missing update identity"))?;
    task.force_resume = mode == InstallMode::ForceResume;
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run(&mut task, &identity, mode).await {
            task.fail(error);
            if task.force_resume {
                task_recovery::recover_and_report(&task).await;
            }
        }
    });
    Ok(snapshot)
}

async fn run(task: &mut Task, identity: &offer::Identity, mode: InstallMode) -> Result<(), String> {
    let settings = preferences::load(&task.conn)
        .await
        .map_err(|error| error.to_string())?;
    if !super::promotion::accepts_channel(&settings, &identity.channel, identity.release_id.as_deref()) {
        return Err("Update channel changed".into());
    }
    let mut update = desktop::checked_update(task, identity, &settings).await?;
    drop(desktop::load_or_download(task, &mut update).await?);
    let _claim = if mode == InstallMode::AfterTasks {
        let Some(claim) = super::manual_wait::wait(task, identity).await? else {
            return Ok(());
        };
        task.relaunch_claimed = true;
        update = refresh_update(task, identity, &settings).await?;
        Some(claim)
    } else {
        None
    };
    state::set_installing(&task.handle, &task.emitter());
    if let Err(error) =
        super::digest::record_before_update(&crate::system_skills::data_dir_from_env()).await
    {
        tracing::warn!(%error, "[app-update] failed to record pre-update digest");
    }
    #[cfg(target_os = "windows")]
    if super::direct_payload::uses_installer(&update) {
        return super::direct_handoff::launch(task, &update, false).await;
    }
    install_package(task, update, mode).await
}

async fn install_package(
    task: &Task,
    mut update: tauri_plugin_updater::Update,
    mode: InstallMode,
) -> Result<(), String> {
    let bytes = desktop::load_or_download(task, &mut update).await?;
    let manager = task.app.state::<ConnectionManager>();
    let _claim = if task.force_resume {
        check_non_agent(task).await?;
        Some(task_recovery::pause(task).await?)
    } else if !task.relaunch_claimed {
        check_tasks(task).await?;
        Some(
            manager
                .claim_relaunch()
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    state::set_installing(&task.handle, &task.emitter());
    desktop::install_verified(&update, bytes)?;
    desktop::finish_install(task, update.version)?;
    if mode != InstallMode::Now && state::try_claim_restart(&task.handle, &task.emitter()) {
        task.app.restart();
    }
    Ok(())
}

pub(super) async fn check_tasks(task: &Task) -> Result<(), String> {
    if let Some(reason) =
        silent_safety::task_blocker_with_idle_delay(&task.app, Duration::ZERO).await?
    {
        return Err(format!("update_blocked:{reason}"));
    }
    Ok(())
}

pub(super) async fn check_non_agent(task: &Task) -> Result<(), String> {
    if let Some(reason) = silent_safety::non_agent_blocker(&task.app).await? {
        return Err(format!("update_blocked:{reason}"));
    }
    Ok(())
}

async fn refresh_update(
    task: &Task,
    identity: &offer::Identity,
    settings: &preferences::UpdatePreferences,
) -> Result<tauri_plugin_updater::Update, String> {
    // 等待期间发布可能被撤回，安装前重新确认签名发布身份。
    let request = super::release::DesktopUpdateRequest {
        app: &task.app,
        preferences: settings,
        access_token: None,
    };
    let (_, update) =
        super::release::check_desktop_update(&request, super::release::CheckReason::Manual)
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Update is no longer available")?;
    let extensions = super::release::extensions(&update).map_err(|error| error.to_string())?;
    offer::validate_checked_update(&update, &extensions, identity)?;
    Ok(update)
}
