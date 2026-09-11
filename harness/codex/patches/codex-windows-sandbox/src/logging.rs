use anyhow::Context;
use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::os::windows::io::OwnedHandle;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

use codex_utils_string::take_bytes_at_char_boundary;
use tracing_appender::rolling::RollingFileAppender;
use tracing_appender::rolling::Rotation;

const LOG_COMMAND_PREVIEW_LIMIT: usize = 200;
pub const LOG_FILE_PREFIX: &str = "sandbox";
pub const LOG_FILE_SUFFIX: &str = "log";
pub const MAX_LOG_FILES: usize = 90;

// Service provisioning never appends to a caller-controlled existing log entry.
// An initialized but empty slot also prevents falling back after an open failure.
static SETUP_LOG: OnceLock<Mutex<Option<(File, OwnedHandle)>>> = OnceLock::new();

fn exe_label() -> &'static str {
    static LABEL: OnceLock<String> = OnceLock::new();
    LABEL.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "proc".to_string())
    })
}

fn preview(command: &[String]) -> String {
    let joined = command.join(" ");
    if joined.len() <= LOG_COMMAND_PREVIEW_LIMIT {
        joined
    } else {
        take_bytes_at_char_boundary(&joined, LOG_COMMAND_PREVIEW_LIMIT).to_string()
    }
}

pub fn log_file_path_for_utc_date(base_dir: &Path, date: chrono::NaiveDate) -> PathBuf {
    base_dir.join(format!(
        "{LOG_FILE_PREFIX}.{}.{}",
        date.format("%Y-%m-%d"),
        LOG_FILE_SUFFIX
    ))
}

pub fn current_log_file_path(base_dir: &Path) -> PathBuf {
    log_file_path_for_utc_date(base_dir, chrono::Utc::now().date_naive())
}

pub fn current_log_file_path_for_codex_home(codex_home: &Path) -> PathBuf {
    current_log_file_path(&crate::sandbox_dir(codex_home))
}

pub fn log_writer(base_dir: &Path) -> Option<RollingFileAppender> {
    if !base_dir.is_dir() {
        return None;
    }

    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(base_dir)
        .ok()
}

/// Opens one fresh log for service provisioning and retains its parent directory.
/// Subsequent setup diagnostics share this handle instead of reopening a pathname.
pub fn setup_log_writer(base_dir: &Path) -> Result<File> {
    let mut log = SETUP_LOG
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow::anyhow!("setup log lock poisoned"))?;
    match log.as_ref() {
        Some((file, _parent)) => file.try_clone().context("clone setup log"),
        None => {
            let (file, parent) = crate::file_write::create_temporary_file(base_dir, ".log")?;
            let writer = file.try_clone().context("clone setup log")?;
            *log = Some((file, parent));
            Ok(writer)
        }
    }
}

fn append_line(line: &str, base_dir: Option<&Path>) {
    if let Some(log) = SETUP_LOG.get() {
        if let Ok(mut log) = log.lock()
            && let Some((file, _parent)) = log.as_mut()
        {
            let _ = writeln!(file, "{line}");
        }
        return;
    }
    if let Some(dir) = base_dir
        && let Some(mut f) = log_writer(dir)
    {
        let _ = writeln!(f, "{line}");
    }
}

pub fn log_start(command: &[String], base_dir: Option<&Path>) {
    let p = preview(command);
    log_note(&format!("START: {p}"), base_dir);
}

pub fn log_success(command: &[String], base_dir: Option<&Path>) {
    let p = preview(command);
    log_note(&format!("SUCCESS: {p}"), base_dir);
}

pub fn log_failure(command: &[String], detail: &str, base_dir: Option<&Path>) {
    let p = preview(command);
    log_note(&format!("FAILURE: {p} ({detail})"), base_dir);
}

// Debug logging helper. Emits only when SBX_DEBUG=1 to avoid noisy logs.
pub fn debug_log(msg: &str, base_dir: Option<&Path>) {
    if std::env::var("SBX_DEBUG").ok().as_deref() == Some("1") {
        append_line(&format!("DEBUG: {msg}"), base_dir);
        eprintln!("{msg}");
    }
}

// Unconditional note logging to the daily sandbox log.
pub fn log_note(msg: &str, base_dir: Option<&Path>) {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    append_line(&format!("[{ts} {}] {}", exe_label(), msg), base_dir);
}
