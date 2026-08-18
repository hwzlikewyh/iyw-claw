//! Seven-day retention for iyw-claw-owned diagnostic logs.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

use sea_orm::DatabaseConnection;

const RETENTION_DAYS: i64 = 7;
const HOURS_PER_DAY: u64 = 24;
const SECONDS_PER_HOUR: u64 = 60 * 60;
const RETENTION_AGE: Duration =
    Duration::from_secs(RETENTION_DAYS as u64 * HOURS_PER_DAY * SECONDS_PER_HOUR);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(HOURS_PER_DAY * SECONDS_PER_HOUR);

pub const MAX_APP_LOG_FILES: usize = RETENTION_DAYS as usize;

static RETENTION_STARTED: OnceLock<()> = OnceLock::new();

#[derive(Default)]
struct FileCleanupSummary {
    scanned: usize,
    deleted: usize,
    failed: usize,
    deleted_bytes: u64,
    first_error: Option<String>,
}

struct FileCleanupReport {
    app: FileCleanupSummary,
    codex_acp: FileCleanupSummary,
    elapsed: Duration,
}

pub async fn start(conn: DatabaseConnection) {
    if RETENTION_STARTED.set(()).is_err() {
        return;
    }
    tracing::info!(
        target: "iyw_claw::diagnostics::retention",
        retention_hours = RETENTION_AGE.as_secs() / SECONDS_PER_HOUR,
        cleanup_interval_hours = CLEANUP_INTERVAL.as_secs() / SECONDS_PER_HOUR,
        "[logs] retention task started"
    );
    run_once(&conn).await;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            run_once(&conn).await;
        }
    });
}

async fn run_once(conn: &DatabaseConnection) {
    match tokio::task::spawn_blocking(cleanup_owned_files).await {
        Ok(report) => {
            log_file_summary("application", &report.app, report.elapsed);
            log_file_summary("codex_acp", &report.codex_acp, report.elapsed);
        }
        Err(error) => tracing::warn!(
            target: "iyw_claw::diagnostics::retention",
            error = %error,
            "[logs] retention file cleanup task failed"
        ),
    }
    cleanup_channel_logs(conn).await;
}

fn cleanup_owned_files() -> FileCleanupReport {
    let started = Instant::now();
    let cutoff = SystemTime::now()
        .checked_sub(RETENTION_AGE)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let app = cleanup_tree(
        crate::paths::iyw_claw_logs_root(),
        cutoff,
        false,
        is_application_log,
    );
    let codex_acp = crate::paths::codex_acp_logs_root()
        .map(|root| cleanup_tree(root, cutoff, true, |_| true))
        .unwrap_or_default();
    FileCleanupReport {
        app,
        codex_acp,
        elapsed: started.elapsed(),
    }
}

fn cleanup_tree(
    root: PathBuf,
    cutoff: SystemTime,
    recursive: bool,
    matches: fn(&Path) -> bool,
) -> FileCleanupSummary {
    let mut summary = FileCleanupSummary::default();
    if !root.is_dir() {
        return summary;
    }
    let mut pending = vec![root];
    while let Some(directory) = pending.pop() {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) => {
                record_failure(&mut summary, error);
                continue;
            }
        };
        cleanup_entries(
            entries,
            cutoff,
            recursive,
            matches,
            &mut pending,
            &mut summary,
        );
    }
    summary
}

fn cleanup_entries(
    entries: std::fs::ReadDir,
    cutoff: SystemTime,
    recursive: bool,
    matches: fn(&Path) -> bool,
    pending: &mut Vec<PathBuf>,
    summary: &mut FileCleanupSummary,
) {
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                record_failure(summary, error);
                continue;
            }
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                record_failure(summary, error);
                continue;
            }
        };
        if recursive && file_type.is_dir() && !file_type.is_symlink() {
            pending.push(entry.path());
            continue;
        }
        if file_type.is_file() && !file_type.is_symlink() && matches(&entry.path()) {
            cleanup_file(&entry.path(), cutoff, summary);
        }
    }
}

fn cleanup_file(path: &Path, cutoff: SystemTime, summary: &mut FileCleanupSummary) {
    summary.scanned += 1;
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            record_failure(summary, error);
            return;
        }
    };
    let modified = match metadata.modified() {
        Ok(modified) => modified,
        Err(error) => {
            record_failure(summary, error);
            return;
        }
    };
    if modified >= cutoff {
        return;
    }
    match std::fs::remove_file(path) {
        Ok(()) => {
            summary.deleted += 1;
            summary.deleted_bytes = summary.deleted_bytes.saturating_add(metadata.len());
        }
        Err(error) => record_failure(summary, error),
    }
}

fn is_application_log(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    name.ends_with(".log")
        && (name.starts_with("iyw-claw.") || name.starts_with("iyw-claw-server."))
}

fn record_failure(summary: &mut FileCleanupSummary, error: std::io::Error) {
    summary.failed += 1;
    if summary.first_error.is_none() {
        summary.first_error = Some(error.to_string());
    }
}

fn log_file_summary(category: &str, summary: &FileCleanupSummary, elapsed: Duration) {
    if summary.failed > 0 {
        tracing::warn!(
            target: "iyw_claw::diagnostics::retention",
            category,
            scanned = summary.scanned,
            deleted = summary.deleted,
            failed = summary.failed,
            deleted_bytes = summary.deleted_bytes,
            elapsed_ms = elapsed.as_millis(),
            error = summary
                .first_error
                .as_deref()
                .unwrap_or("unknown I/O error"),
            "[logs] retention cleanup completed with errors"
        );
    } else if summary.deleted > 0 {
        tracing::info!(
            target: "iyw_claw::diagnostics::retention",
            category,
            scanned = summary.scanned,
            deleted = summary.deleted,
            deleted_bytes = summary.deleted_bytes,
            elapsed_ms = elapsed.as_millis(),
            "[logs] retention cleanup completed"
        );
    } else {
        tracing::debug!(
            target: "iyw_claw::diagnostics::retention",
            category,
            scanned = summary.scanned,
            "[logs] retention cleanup checked"
        );
    }
}

async fn cleanup_channel_logs(conn: &DatabaseConnection) {
    let started = Instant::now();
    let cutoff = chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS);
    match crate::db::service::chat_channel_message_log_service::cleanup_old_logs(conn, cutoff).await
    {
        Ok(deleted) if deleted > 0 => tracing::info!(
            target: "iyw_claw::diagnostics::retention",
            category = "chat_channel",
            deleted,
            elapsed_ms = started.elapsed().as_millis(),
            "[logs] retention cleanup completed"
        ),
        Ok(_) => tracing::debug!(
            target: "iyw_claw::diagnostics::retention",
            category = "chat_channel",
            "[logs] retention cleanup checked"
        ),
        Err(error) => tracing::warn!(
            target: "iyw_claw::diagnostics::retention",
            category = "chat_channel",
            error = %error,
            elapsed_ms = started.elapsed().as_millis(),
            "[logs] retention cleanup failed"
        ),
    }
}
