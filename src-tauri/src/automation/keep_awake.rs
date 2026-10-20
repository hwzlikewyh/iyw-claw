use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::db::service::automation_service;

#[cfg(not(target_os = "windows"))]
#[path = "keep_awake_process.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "keep_awake_windows.rs"]
mod platform;

const SYNC_INTERVAL: Duration = Duration::from_secs(5);
static CHANGED: Notify = Notify::const_new();

pub(crate) struct KeepAwakeTask(JoinHandle<()>);

impl Drop for KeepAwakeTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) fn notify_changed() {
    CHANGED.notify_one();
}

pub(crate) fn spawn(conn: DatabaseConnection) -> KeepAwakeTask {
    KeepAwakeTask(tokio::spawn(async move {
        let mut controller = Controller::default();
        let mut interval = tokio::time::interval(SYNC_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = interval.tick() => {},
                _ = CHANGED.notified() => {},
            }
            match automation_service::has_enabled_schedule(&conn).await {
                Ok(enabled) => controller.sync(enabled),
                // 查询失败时保留已有锁，避免一次数据库故障导致休眠。
                Err(error) => controller.report_error(error.to_string()),
            }
        }
    }))
}

#[derive(Default)]
struct Controller {
    guard: Option<platform::Inhibitor>,
    last_error: Option<String>,
}

impl Controller {
    fn sync(&mut self, enabled: bool) {
        if !enabled {
            if self.guard.take().is_some() {
                tracing::info!("[automation] released scheduled-task sleep inhibitor");
            }
            self.last_error = None;
            return;
        }
        if let Some(guard) = self.guard.as_mut() {
            match guard.refresh() {
                Ok(()) => {
                    if self.last_error.take().is_some() {
                        tracing::info!("[automation] scheduled-task sleep inhibitor recovered");
                    }
                    return;
                }
                Err(error) => self.report_error(error.to_string()),
            }
            self.guard.take();
        }
        match platform::Inhibitor::start() {
            Ok(guard) => {
                self.guard = Some(guard);
                if self.last_error.is_none() {
                    tracing::info!("[automation] requested scheduled-task sleep inhibitor");
                }
            }
            Err(error) => self.report_error(error.to_string()),
        }
    }

    fn report_error(&mut self, error: String) {
        if self.last_error.as_deref() != Some(error.as_str()) {
            tracing::warn!(error = %error, "[automation] scheduled-task sleep inhibitor unavailable");
        }
        self.last_error = Some(error);
    }
}
