use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

const REPORT_INTERVAL: Duration = Duration::from_secs(1);
const MAX_REPORTS_PER_INTERVAL: u32 = 30;
static REPORT_WINDOW: Mutex<Option<(Instant, u32)>> = Mutex::new(None);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendEvent {
    Offline,
    AccountBlocked,
    ComposerTimeout,
    ConnectTimeout,
    ModeTimeout,
    ContextTimeout,
    ConfigWait,
    QueueStalled,
}

fn allow_report() -> bool {
    let mut guard = REPORT_WINDOW.lock().unwrap_or_else(|err| err.into_inner());
    let (start, count) = guard.get_or_insert_with(|| (Instant::now(), 0));
    if start.elapsed() >= REPORT_INTERVAL {
        *start = Instant::now();
        *count = 0;
    }
    if *count >= MAX_REPORTS_PER_INTERVAL {
        return false;
    }
    *count += 1;
    true
}

pub fn report_frontend_event_core(event: SendEvent, conversation_id: Option<i32>) {
    if allow_report() {
        tracing::info!(?event, ?conversation_id, "[composer] send diagnostic");
    }
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub fn report_frontend_event(event: SendEvent, conversation_id: Option<i32>) {
    report_frontend_event_core(event, conversation_id);
}
