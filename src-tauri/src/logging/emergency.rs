//! Synchronous startup and panic diagnostics.
//!
//! The normal tracing subscriber is asynchronous and may not exist during
//! the first lines of process startup. This writer bypasses the subscriber but
//! appends to the same daily JSONL file, so diagnostics stay in one place.

use std::cell::Cell;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub use super::startup_stage::{run_stage, StartupStage};

use super::emergency_redact::redact;

const FILE_PREFIX: &str = "iyw-claw";

static SINK_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
static RUN_ID: OnceLock<String> = OnceLock::new();
static PANIC_HOOK: OnceLock<()> = OnceLock::new();
static CURRENT_STAGE: Mutex<&'static str> = Mutex::new("process-entry");

thread_local! {
    static PANIC_HOOK_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

struct PanicHookGuard;

impl PanicHookGuard {
    fn enter() -> Option<Self> {
        if PANIC_HOOK_ACTIVE.with(|active| active.replace(true)) {
            None
        } else {
            Some(Self)
        }
    }
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        PANIC_HOOK_ACTIVE.with(|active| active.set(false));
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default()
}

fn run_id() -> &'static str {
    RUN_ID.get_or_init(|| format!("{}-{}", now_ms(), std::process::id()))
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = std::env::var_os("IYW_CLAW_LOG_DIR").filter(|v| !v.is_empty()) {
        candidates.push(PathBuf::from(value));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(root) = crate::desktop_bootstrap::resolve_install_root(&executable) {
            candidates.push(root.join("logs"));
        }
    }
    if let Some(value) = std::env::var_os("IYW_CLAW_HOME").filter(|v| !v.is_empty()) {
        candidates.push(PathBuf::from(value).join("logs"));
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".iyw-claw").join("logs"));
    }
    if let Some(local) = dirs::data_local_dir() {
        candidates.push(local.join("app.iywclaw").join("logs"));
    }
    candidates.push(std::env::temp_dir().join("iyw-claw").join("logs"));
    candidates
}

fn daily_file_name() -> String {
    format!(
        "{FILE_PREFIX}.{}.log",
        chrono::Utc::now().format("%Y-%m-%d")
    )
}

fn resolve_sink_dir() -> Option<PathBuf> {
    for directory in candidate_dirs() {
        if fs::create_dir_all(&directory).is_err() {
            continue;
        }
        let path = directory.join(daily_file_name());
        if OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .is_ok()
        {
            std::env::set_var("IYW_CLAW_LOG_DIR", &directory);
            return Some(directory);
        }
    }
    None
}

pub fn emergency_path() -> Option<PathBuf> {
    SINK_DIR
        .get_or_init(resolve_sink_dir)
        .as_ref()
        .map(|directory| directory.join(daily_file_name()))
}

fn current_stage() -> &'static str {
    match CURRENT_STAGE.lock() {
        Ok(value) => *value,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

pub(super) fn replace_stage(stage: &'static str) -> &'static str {
    match CURRENT_STAGE.lock() {
        Ok(mut value) => std::mem::replace(&mut *value, stage),
        Err(poisoned) => {
            let mut value = poisoned.into_inner();
            std::mem::replace(&mut *value, stage)
        }
    }
}

pub(super) fn restore_stage(previous: &'static str) {
    match CURRENT_STAGE.lock() {
        Ok(mut value) => *value = previous,
        Err(poisoned) => *poisoned.into_inner() = previous,
    }
}

pub fn set_process_stage(stage: &'static str) {
    let _ = replace_stage(stage);
}

pub fn write_event(event: &str, status: &str, stage: &str, detail: Option<String>) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        write_event_inner(event, status, stage, detail);
    }));
}

fn write_event_inner(event: &str, status: &str, stage: &str, detail: Option<String>) {
    let Some(path) = emergency_path() else {
        let _ = writeln!(
            std::io::stderr(),
            "[iyw-claw][emergency] event={event} status={status} stage={stage}"
        );
        return;
    };
    let record = serde_json::json!({
        "schema_version": 1,
        "timestamp_ms": now_ms(),
        "app_version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "run_id": run_id(),
        "event": event,
        "status": status,
        "stage": stage,
        "thread": format!("{:?}", std::thread::current().id()),
        "detail": detail.map(redact),
    });
    let Ok(mut line) = serde_json::to_string(&record) else {
        return;
    };
    line.push('\n');
    super::budget::record_essential_write(line.len());
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    if file.write_all(line.as_bytes()).is_ok() {
        let _ = file.sync_data();
    }
}

pub fn install_panic_hook() {
    PANIC_HOOK.get_or_init(|| {
        let _ = catch_unwind(AssertUnwindSafe(emergency_path));
        write_event("process_enter", "begin", "process-entry", None);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let Some(_guard) = PanicHookGuard::enter() else {
                previous(info);
                return;
            };
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("panic payload was not a string");
            let location = info
                .location()
                .map(|value| format!("{}:{}:{}", value.file(), value.line(), value.column()))
                .unwrap_or_else(|| "unknown".to_string());
            write_event(
                "panic",
                "error",
                current_stage(),
                Some(format!("location={location}; {payload}")),
            );
            previous(info);
        }));
    });
}
