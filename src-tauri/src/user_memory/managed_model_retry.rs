use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio::sync::Notify;

use super::managed_model_state::load_retry;
use super::UserMemoryService;

const IDLE_RECHECK: Duration = Duration::from_secs(6 * 60 * 60);

static RETRY_NOTIFY: LazyLock<Notify> = LazyLock::new(Notify::new);
static RETRY_WORKER_STARTED: AtomicBool = AtomicBool::new(false);

pub(super) fn notify_retry() {
    RETRY_NOTIFY.notify_one();
}

pub(super) fn start_retry_worker(service: UserMemoryService, data_dir: PathBuf) {
    if RETRY_WORKER_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let worker = retry_loop(service, data_dir);
    #[cfg(feature = "tauri-runtime")]
    tauri::async_runtime::spawn(worker);
    #[cfg(not(feature = "tauri-runtime"))]
    tokio::spawn(worker);
}

async fn retry_loop(service: UserMemoryService, data_dir: PathBuf) {
    loop {
        let wait = retry_wait(&service.db).await;
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = RETRY_NOTIFY.notified() => continue,
        }
        let Ok(Some(state)) = load_retry(&service.db).await else {
            continue;
        };
        if state.next_attempt_at > chrono::Utc::now() {
            continue;
        }
        let channel = current_channel(&service.db).await;
        let _ = service.prepare_managed_model(&data_dir, &channel).await;
    }
}

async fn retry_wait(conn: &DatabaseConnection) -> Duration {
    let Ok(Some(state)) = load_retry(conn).await else {
        return IDLE_RECHECK;
    };
    let milliseconds = (state.next_attempt_at - chrono::Utc::now())
        .num_milliseconds()
        .max(0) as u64;
    Duration::from_millis(milliseconds)
}

async fn current_channel(conn: &DatabaseConnection) -> String {
    crate::update::preferences::load(conn)
        .await
        .map(|value| value.channel.as_str().to_string())
        .unwrap_or_else(|_| "stable".to_string())
}
