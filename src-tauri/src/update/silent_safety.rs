//! 空闲安装的窗口与文件门禁；隐藏 WebView 暂停时保留最后的阻塞状态。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

pub const IDLE_DELAY: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct WindowSafety {
    generation: u64,
    blocked: Option<bool>,
}

#[derive(Default)]
struct Safety {
    windows: HashMap<String, WindowSafety>,
    background_since: Option<Instant>,
}

static SAFETY: LazyLock<Mutex<Safety>> = LazyLock::new(|| Mutex::new(Safety::default()));

pub fn reset_window(label: &str) {
    let mut safety = SAFETY.lock().unwrap_or_else(|error| error.into_inner());
    let entry = safety.windows.entry(label.to_string()).or_default();
    entry.generation += 1;
    entry.blocked = None;
    safety.background_since = None;
}

pub fn note_window_event(label: &str, event: &tauri::WindowEvent) {
    let mut safety = SAFETY.lock().unwrap_or_else(|error| error.into_inner());
    match event {
        tauri::WindowEvent::Focused(_) => safety.background_since = None,
        tauri::WindowEvent::Destroyed => {
            safety.windows.remove(label);
            safety.background_since = None;
        }
        _ => {}
    }
}

pub fn report(label: &str, generation: Option<u64>, blocked: bool) -> u64 {
    let mut safety = SAFETY.lock().unwrap_or_else(|error| error.into_inner());
    let entry = safety.windows.entry(label.to_string()).or_default();
    if generation.is_none() {
        entry.generation += 1;
        entry.blocked = Some(true);
    } else if generation == Some(entry.generation) {
        entry.blocked = Some(blocked);
    }
    entry.generation
}

pub fn blocker(app: &AppHandle) -> Option<&'static str> {
    let windows = app.webview_windows();
    // 原生窗口查询可能切到 UI 线程；此时不能持有事件回调也要获取的互斥锁。
    let focused = windows
        .values()
        .any(|window| window.is_focused().unwrap_or(true));
    let mut safety = SAFETY.lock().unwrap_or_else(|error| error.into_inner());
    if focused {
        safety.background_since = None;
        return Some("window_focused");
    }
    for label in windows.keys() {
        // 宠物常驻窗口不阻止更新，其他辅助编辑/远程窗口保守等待关闭。
        if label.starts_with("pet") {
            continue;
        }
        if label != "main" && label != "settings" {
            return Some("auxiliary_window_open");
        }
        if !safety
            .windows
            .get(label)
            .is_some_and(|entry| entry.blocked == Some(false))
        {
            return Some("unsaved_or_unknown_window_state");
        }
    }
    if !windows.contains_key("main") {
        return Some("main_window_unavailable");
    }
    let background_since = safety.background_since.get_or_insert_with(Instant::now);
    (background_since.elapsed() < IDLE_DELAY).then_some("background_grace_period")
}

pub async fn task_blocker(app: &AppHandle) -> Result<Option<&'static str>, String> {
    let manager = app.state::<crate::acp::manager::ConnectionManager>();
    if !manager.is_globally_idle(IDLE_DELAY).await {
        return Ok(Some("agent_operation_active"));
    }
    let db = app.state::<crate::db::AppDatabase>();
    if !crate::db::service::automation_service::list_active_runs(&db.conn)
        .await
        .map_err(|error| error.to_string())?
        .is_empty()
    {
        return Ok(Some("automation_active"));
    }
    let voice = app.state::<crate::commands::realtime_voice::RealtimeVoiceState>();
    for label in app.webview_windows().keys() {
        if voice.has_session(label).await {
            return Ok(Some("voice_active"));
        }
    }
    let terminals = app.state::<crate::terminal::manager::TerminalManager>();
    let emitter = crate::web::event_bridge::EventEmitter::Tauri(app.clone());
    // 无可靠的前台命令探测时，不退出任何仍存活的交互终端。
    Ok((!terminals.list_with_exit_check(Some(&emitter)).is_empty()).then_some("terminal_open"))
}
