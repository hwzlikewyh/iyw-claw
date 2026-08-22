#[cfg(feature = "tauri-runtime")]
use rand::Rng;
#[cfg(feature = "tauri-runtime")]
use sea_orm::DatabaseConnection;
#[cfg(feature = "tauri-runtime")]
use tauri::Manager;

#[cfg(feature = "tauri-runtime")]
use crate::update::preferences;
#[cfg(feature = "tauri-runtime")]
use crate::update::release::CheckReason;

#[cfg(feature = "tauri-runtime")]
const CHECK_INTERVAL_SECS: i64 = 15 * 60;
#[cfg(feature = "tauri-runtime")]
static SCHEDULER_WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();

#[cfg(feature = "tauri-runtime")]
pub fn wake() {
    SCHEDULER_WAKE.notify_one();
}

#[cfg(feature = "tauri-runtime")]
pub fn wake_for_focus(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let db = app.state::<crate::db::AppDatabase>();
        match preferences::load(&db.conn).await {
            Ok(value) if value.auto_check && !value.attempted_recently(CHECK_INTERVAL_SECS) => {
                wake();
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!("[app-update] failed to evaluate focus check: {error}");
            }
        }
    });
}

#[cfg(feature = "tauri-runtime")]
pub fn spawn(
    app: tauri::AppHandle,
    conn: DatabaseConnection,
    state: crate::update::AppUpdateStateHandle,
) {
    tauri::async_runtime::spawn(async move {
        let startup_delay = rand::thread_rng().gen_range(30..=90);
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(startup_delay)) => {}
            _ = SCHEDULER_WAKE.notified() => {}
        }
        loop {
            let delay = run_once(&app, &conn, &state).await;
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = SCHEDULER_WAKE.notified() => {}
            }
        }
    });
}

#[cfg(feature = "tauri-runtime")]
async fn run_once(
    app: &tauri::AppHandle,
    conn: &DatabaseConnection,
    state: &crate::update::AppUpdateStateHandle,
) -> std::time::Duration {
    let (preferences, reminder_expired) = match preferences::load_for_scheduler(conn).await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!("[app-update] failed to load scheduler preferences: {error}");
            return std::time::Duration::from_secs(15 * 60);
        }
    };
    if !preferences.auto_check {
        return next_success_delay(&preferences);
    }
    if preferences.checked_recently(CHECK_INTERVAL_SECS) && !reminder_expired {
        return next_success_delay(&preferences);
    }
    if let Err(error) = crate::commands::app_update::check_desktop_update_core(
        app,
        conn,
        state,
        CheckReason::Automatic,
    )
    .await
    {
        tracing::warn!("[app-update] automatic check failed: {error}");
    }
    let latest = preferences::load(conn).await.unwrap_or(preferences);
    if latest.failure_count == 0 {
        next_success_delay(&latest)
    } else {
        failure_backoff(latest.failure_count)
    }
}

#[cfg(feature = "tauri-runtime")]
fn jittered_interval() -> std::time::Duration {
    jittered(std::time::Duration::from_secs(CHECK_INTERVAL_SECS as u64))
}

#[cfg(feature = "tauri-runtime")]
fn next_success_delay(preferences: &preferences::UpdatePreferences) -> std::time::Duration {
    let regular = jittered_interval();
    let Some(reminder) = preferences.reminder_delay() else {
        return regular;
    };
    let reminder_jitter = std::time::Duration::from_secs(rand::thread_rng().gen_range(0..=30));
    regular.min(reminder.saturating_add(reminder_jitter))
}

#[cfg(feature = "tauri-runtime")]
fn failure_backoff(failure_count: u32) -> std::time::Duration {
    let seconds = match failure_count {
        0 | 1 => 5 * 60,
        2 => 15 * 60,
        3 => 60 * 60,
        _ => 6 * 60 * 60,
    };
    jittered(std::time::Duration::from_secs(seconds))
}

#[cfg(feature = "tauri-runtime")]
fn jittered(base: std::time::Duration) -> std::time::Duration {
    let seconds = base.as_secs().max(1);
    let spread = (seconds / 5).max(1);
    let offset = rand::thread_rng().gen_range(-(spread as i64)..=(spread as i64));
    std::time::Duration::from_secs((seconds as i64 + offset).max(1) as u64)
}
