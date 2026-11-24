use tauri::Manager;
use tokio::sync::Mutex;

use super::{desktop::Task, recovery_resume, recovery_store, state};
use crate::acp::{
    manager::{ConnectionManager, RelaunchClaim},
    update_tasks,
};

static RECOVERY_LOCK: Mutex<()> = Mutex::const_new(());
// Windows 向导只等待父进程 60 秒，为正常退出和资源清理保留时间。
const PAUSE_BATCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub(crate) async fn discard_user_cancelled(
    db: &sea_orm::DatabaseConnection,
    conversation_id: i32,
) -> Result<(), String> {
    let _lock = RECOVERY_LOCK.lock().await;
    let mut recovery = recovery_store::load()?;
    let cancelled = recovery
        .tasks
        .iter()
        .filter(|entry| entry.conversation_id == conversation_id)
        .map(|entry| entry.input_id.clone())
        .collect::<Vec<_>>();
    if cancelled.is_empty() {
        return Ok(());
    }
    for id in cancelled {
        crate::db::service::agent_input_outbox_service::delete_waiting(db, &id)
            .await
            .map_err(|error| error.to_string())?;
    }
    recovery
        .tasks
        .retain(|entry| entry.conversation_id != conversation_id);
    recovery_store::save(&recovery)
}

pub(super) async fn pause(task: &Task) -> Result<RelaunchClaim, String> {
    let _lock = RECOVERY_LOCK.lock().await;
    if !recovery_store::load()?.tasks.is_empty() {
        return Err("update_blocked:recovery_pending".into());
    }
    let manager = task.app.state::<ConnectionManager>();
    let claim = manager
        .claim_update_pause()
        .map_err(|error| error.to_string())?;
    let mut recovery = recovery_store::Recovery {
        tasks: update_tasks::capture(&manager, &task.conn).await?,
    };
    recovery_store::save(&recovery)?;
    state::mutate(&task.handle, &task.emitter(), |state| {
        state.status = state::AppUpdateLifecycle::PreparingRestart;
    });
    tokio::time::timeout(PAUSE_BATCH_TIMEOUT, pause_all(&manager, &mut recovery))
        .await
        .map_err(|_| "update_blocked:pause_timeout".to_string())??;
    tracing::info!(
        count = recovery.tasks.iter().filter(|entry| entry.armed).count(),
        "[app-update] interrupted tasks saved for restart"
    );
    Ok(claim)
}

async fn resume(task: &Task) -> Result<(), String> {
    let mut recovery = recovery_store::load()?;
    let mut failure = None;
    let mut index = 0;
    while index < recovery.tasks.len() {
        match recovery_resume::resume(task, &recovery.tasks[index]).await {
            Ok(()) => {
                recovery.tasks.remove(index);
                recovery_store::save(&recovery)?;
            }
            Err(error) => {
                tracing::error!(conversation_id = recovery.tasks[index].conversation_id,
                    %error, "[app-update] task recovery deferred");
                failure = Some(error);
                index += 1;
            }
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(super) async fn recover_and_report(task: &Task) {
    let Ok(_lock) = RECOVERY_LOCK.try_lock() else {
        return;
    };
    state::mutate(&task.handle, &task.emitter(), |state| {
        state.recovering_tasks = true
    });
    let error = resume(task).await.err();
    state::mutate(&task.handle, &task.emitter(), |state| {
        state.recovery_error = error;
        state.recovering_tasks = false;
    });
}

pub(super) fn spawn(task: Task) {
    tauri::async_runtime::spawn(async move {
        // 等主窗口建立后再恢复，避免在同步 setup 阶段启动 Agent。
        loop {
            if task.app.get_webview_window("main").is_some() {
                break;
            }
            if crate::desktop_shutdown::is_quitting() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        recover_and_report(&task).await;
    });
}

async fn pause_all(
    manager: &ConnectionManager,
    recovery: &mut recovery_store::Recovery,
) -> Result<(), String> {
    for index in 0..recovery.tasks.len() {
        recovery.tasks[index].armed = true;
        recovery_store::save(recovery)?;
        let keep = update_tasks::pause(manager, &recovery.tasks[index]).await?;
        recovery.tasks[index].armed = keep;
        recovery_store::save(recovery)?;
    }
    Ok(())
}
