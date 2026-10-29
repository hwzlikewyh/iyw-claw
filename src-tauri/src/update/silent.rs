//! 仅本地桌面启用；空闲安装复用下载缓存、签名校验与重启互斥。

use std::time::Duration;

use tauri::Manager;

use crate::acp::agent_storage_work::{
    has_active_agent_storage_work, try_begin_agent_storage_migration,
};
use crate::acp::manager::ConnectionManager;
use crate::acp::version_center::acquire_writer_lock;

use super::desktop::{self, Task};
use super::offer::Identity;
use super::preferences;
use super::silent_safety;
use super::state::{self, AppUpdateLifecycle};

const POLL_INTERVAL: Duration = Duration::from_secs(15);

pub fn spawn(task: Task) {
    tauri::async_runtime::spawn(async move {
        let mut last_blocker = None;
        let mut failed_restart_seq = None;
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            let before = state::snapshot(&task.handle);
            if failed_restart_seq == Some(before.seq) {
                continue;
            }
            match run_once(&task).await {
                Ok(reason) => {
                    if reason != last_blocker {
                        tracing::info!(reason, "[app-update] silent update eligibility changed");
                        last_blocker = reason;
                    }
                }
                Err(error) => {
                    if before.status == AppUpdateLifecycle::ReadyToRestart {
                        tracing::warn!(%error, "[app-update] silent restart deferred; manual restart remains available");
                        failed_restart_seq = Some(before.seq);
                    } else {
                        super::offer::fail_prepared(&task, before.seq, error);
                    }
                    last_blocker = Some("update_failed");
                }
            }
        }
    });
}

async fn run_once(task: &Task) -> Result<Option<&'static str>, String> {
    // 即使尚无更新也维护后台计时，焦点变化通过原生事件立即归零。
    let blocker = silent_safety::blocker(&task.app);
    let snapshot = state::snapshot(&task.handle);
    if !matches!(
        snapshot.status,
        AppUpdateLifecycle::ReadyToInstall | AppUpdateLifecycle::ReadyToRestart
    ) {
        return Ok(Some("update_not_prepared"));
    }
    if blocker.is_some() {
        return Ok(blocker);
    }
    let Some(version) = snapshot.version else {
        return Ok(Some("missing_version"));
    };
    let identity = Identity {
        version,
        release_id: snapshot.release_id,
        channel: snapshot.channel.unwrap_or_default(),
    };
    if snapshot.status == AppUpdateLifecycle::ReadyToRestart {
        if let Some(_guards) = claim(task, &identity).await? {
            if state::try_claim_restart(&task.handle, &task.emitter()) {
                tracing::info!(version = %identity.version, "[app-update] silent restart started");
                task.app.restart();
            }
        }
        return Ok(Some("restart_deferred"));
    }
    if let Some(reason) = eligibility(task, &identity).await? {
        return Ok(Some(reason));
    }
    desktop::install_silently(task, &identity).await?;
    Ok(Some("installation_finished_or_deferred"))
}

async fn eligibility(task: &Task, identity: &Identity) -> Result<Option<&'static str>, String> {
    let preferences = preferences::load(&task.conn)
        .await
        .map_err(|error| error.to_string())?;
    if !preferences.silent_auto_update {
        return Ok(Some("disabled"));
    }
    if !preferences.auto_check {
        return Ok(Some("automatic_checks_disabled"));
    }
    if preferences.channel.as_str() != identity.channel {
        return Ok(Some("channel_changed"));
    }
    let snapshot = state::snapshot(&task.handle);
    if preferences.suppresses(
        &identity.version,
        snapshot.update_policy.as_deref() == Some("required"),
    ) {
        return Ok(Some("update_postponed"));
    }
    if crate::desktop_shutdown::is_quitting() {
        return Ok(Some("app_quitting"));
    }
    if has_active_agent_storage_work() {
        return Ok(Some("agent_storage_busy"));
    }
    if let Some(reason) = silent_safety::blocker(&task.app) {
        return Ok(Some(reason));
    }
    silent_safety::task_blocker(&task.app).await
}

pub(super) async fn claim(task: &Task, identity: &Identity) -> Result<Option<impl Send>, String> {
    if eligibility(task, identity).await?.is_some() {
        return Ok(None);
    }
    let Some(storage) = try_begin_agent_storage_migration() else {
        return Ok(None);
    };
    let data_dir = crate::system_skills::data_dir_from_env();
    let Some(writer) = acquire_writer_lock(&data_dir)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let manager = task.app.state::<ConnectionManager>();
    let Ok(relaunch) = manager.claim_relaunch().await else {
        return Ok(None);
    };
    // 拿到互斥后重新校验：准备缓存、计算摘要期间可能恢复使用或关闭开关。
    if eligibility(task, identity).await?.is_some() {
        return Ok(None);
    }
    if silent_safety::blocker(&task.app).is_some() {
        return Ok(None);
    }
    Ok(Some((storage, writer, relaunch)))
}
