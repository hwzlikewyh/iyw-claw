use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use tauri::Manager;

use crate::acp::manager::ConnectionManager;
use crate::acp::types::ConnectionStatus;
use crate::browser::BrowserSessionManager;
use crate::terminal::manager::TerminalManager;

static QUITTING: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_COMPLETE: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy)]
pub enum ShutdownReason {
    NormalExit,
    WindowsUpdate,
}

impl ShutdownReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::NormalExit => "normal_exit",
            Self::WindowsUpdate => "windows_update",
        }
    }
}

pub fn is_quitting() -> bool {
    QUITTING.load(Ordering::Acquire)
}

pub fn shutdown_blocking(app: &tauri::AppHandle, reason: ShutdownReason) {
    QUITTING.store(true, Ordering::Release);
    let _guard = SHUTDOWN_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if SHUTDOWN_COMPLETE.load(Ordering::Acquire) {
        tracing::info!(
            shutdown_reason = reason.as_str(),
            "[shutdown] already complete"
        );
        return;
    }

    let started = Instant::now();
    tracing::info!(shutdown_reason = reason.as_str(), "[shutdown] started");
    if run_on_shutdown_thread(app.clone(), reason) {
        SHUTDOWN_COMPLETE.store(true, Ordering::Release);
    }
    tracing::info!(
        shutdown_reason = reason.as_str(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        complete = SHUTDOWN_COMPLETE.load(Ordering::Acquire),
        "[shutdown] finished"
    );
}

fn run_on_shutdown_thread(app: tauri::AppHandle, reason: ShutdownReason) -> bool {
    let worker = std::thread::Builder::new()
        .name("iyw-claw-shutdown".to_string())
        .spawn(move || tauri::async_runtime::block_on(shutdown_resources(&app, reason)));
    match worker {
        Ok(worker) => match worker.join() {
            Ok(()) => true,
            Err(_) => {
                tracing::error!(
                    shutdown_reason = reason.as_str(),
                    "[shutdown] worker panicked"
                );
                false
            }
        },
        Err(error) => {
            tracing::error!(
                shutdown_reason = reason.as_str(),
                error = %error,
                "[shutdown] worker could not start"
            );
            false
        }
    }
}

async fn shutdown_resources(app: &tauri::AppHandle, reason: ShutdownReason) {
    let live_agent_connections = snapshot_live_agent_connections(app).await;
    stop_entrypoints(app, reason).await;
    maintain_agent_logs(app, reason, live_agent_connections).await;
    stop_terminals(app, reason);
    stop_office_watchers(reason);
    stop_browser(app, reason).await;
}

async fn snapshot_live_agent_connections(app: &tauri::AppHandle) -> Option<usize> {
    let manager = app.try_state::<ConnectionManager>()?;
    Some(
        manager
            .list_connections()
            .await
            .into_iter()
            .filter(|connection| {
                !matches!(
                    connection.status,
                    ConnectionStatus::Disconnected | ConnectionStatus::Error
                )
            })
            .count(),
    )
}

async fn maintain_agent_logs(
    app: &tauri::AppHandle,
    reason: ShutdownReason,
    live_connections: Option<usize>,
) {
    let skip_reason = match (reason, live_connections) {
        (ShutdownReason::WindowsUpdate, _) => Some("update_exit"),
        (_, None) => Some("manager_unavailable"),
        (_, Some(count)) if count > 0 => Some("live_agents"),
        _ => None,
    };
    if let Some(skip_reason) = skip_reason {
        log_agent_cleanup_skip(reason, live_connections, skip_reason);
        return;
    }
    let Some(database) = app.try_state::<crate::db::AppDatabase>() else {
        log_agent_cleanup_skip(reason, live_connections, "database_unavailable");
        return;
    };
    let report = crate::logging::agent_retention::cleanup_managed_agent_logs(&database.conn).await;
    log_agent_cleanup_report(reason, live_connections, &report);
}

fn log_agent_cleanup_skip(
    reason: ShutdownReason,
    live_connections: Option<usize>,
    decision: &'static str,
) {
    tracing::info!(
        target: "iyw_claw::diagnostics::agent_retention",
        shutdown_reason = reason.as_str(),
        live_agent_connections = live_connections.unwrap_or_default(),
        activity_known = live_connections.is_some(),
        decision,
        "[shutdown] Agent log cleanup skipped"
    );
}

fn log_agent_cleanup_report(
    reason: ShutdownReason,
    live_connections: Option<usize>,
    report: &crate::logging::agent_retention::AgentLogCleanupReport,
) {
    if report.failed_files > 0 || report.decision == "cleanup_timed_out" {
        log_agent_cleanup_error(reason, live_connections, report);
    } else {
        log_agent_cleanup_success(reason, live_connections, report);
    }
}

fn log_agent_cleanup_error(
    reason: ShutdownReason,
    live_connections: Option<usize>,
    report: &crate::logging::agent_retention::AgentLogCleanupReport,
) {
    tracing::warn!(
        target: "iyw_claw::diagnostics::agent_retention",
        shutdown_reason = reason.as_str(),
        live_agent_connections = live_connections.unwrap_or_default(),
        scanned_agents = report.scanned_agents,
        scanned_files = report.scanned_files,
        total_bytes = report.total_bytes,
        deleted_files = report.deleted_files,
        deleted_bytes = report.deleted_bytes,
        failed_files = report.failed_files,
        elapsed_ms = report.elapsed.as_millis() as u64,
        decision = report.decision,
        error = report.first_error.as_deref().unwrap_or(""),
        "[shutdown] Agent log cleanup incomplete"
    );
}

fn log_agent_cleanup_success(
    reason: ShutdownReason,
    live_connections: Option<usize>,
    report: &crate::logging::agent_retention::AgentLogCleanupReport,
) {
    tracing::info!(
        target: "iyw_claw::diagnostics::agent_retention",
        shutdown_reason = reason.as_str(),
        live_agent_connections = live_connections.unwrap_or_default(),
        scanned_agents = report.scanned_agents,
        scanned_files = report.scanned_files,
        total_bytes = report.total_bytes,
        threshold_bytes = crate::logging::agent_retention::TOTAL_BYTES_THRESHOLD,
        retention_days = crate::logging::agent_retention::RETENTION_DAYS,
        deleted_files = report.deleted_files,
        deleted_bytes = report.deleted_bytes,
        elapsed_ms = report.elapsed.as_millis() as u64,
        decision = report.decision,
        "[shutdown] Agent log cleanup evaluated"
    );
}

async fn stop_entrypoints(app: &tauri::AppHandle, reason: ShutdownReason) {
    let started = Instant::now();
    let web_server_found = if let Some(state) = app.try_state::<crate::web::WebServerState>() {
        crate::web::do_stop_web_server(&state).await;
        true
    } else {
        false
    };
    let disconnected = if let Some(manager) = app.try_state::<ConnectionManager>() {
        manager.disconnect_all().await
    } else {
        0
    };
    tracing::info!(
        shutdown_reason = reason.as_str(),
        shutdown_stage = "entrypoints",
        elapsed_ms = started.elapsed().as_millis() as u64,
        web_server_found,
        disconnected,
        "[shutdown] entrypoints stopped"
    );
}

fn stop_terminals(app: &tauri::AppHandle, reason: ShutdownReason) {
    let started = Instant::now();
    let killed = app
        .try_state::<TerminalManager>()
        .map(|manager| manager.kill_all())
        .unwrap_or_default();
    tracing::info!(
        shutdown_reason = reason.as_str(),
        shutdown_stage = "terminals",
        elapsed_ms = started.elapsed().as_millis() as u64,
        killed,
        "[shutdown] terminals stopped"
    );
}

fn stop_office_watchers(reason: ShutdownReason) {
    let started = Instant::now();
    let stopped = crate::office_watch::stop_all_office_watches();
    tracing::info!(
        shutdown_reason = reason.as_str(),
        shutdown_stage = "office_watchers",
        elapsed_ms = started.elapsed().as_millis() as u64,
        stopped,
        "[shutdown] office watchers stopped"
    );
}

async fn stop_browser(app: &tauri::AppHandle, reason: ShutdownReason) {
    let started = Instant::now();
    let result = if let Some(browser) = app.try_state::<BrowserSessionManager>() {
        browser.shutdown().await
    } else {
        Ok(())
    };
    match result {
        Ok(()) => tracing::info!(
            shutdown_reason = reason.as_str(),
            shutdown_stage = "browser",
            elapsed_ms = started.elapsed().as_millis() as u64,
            "[shutdown] browser stopped"
        ),
        Err(error) => tracing::error!(
            shutdown_reason = reason.as_str(),
            shutdown_stage = "browser",
            elapsed_ms = started.elapsed().as_millis() as u64,
            error_code = ?error.code,
            error = %error,
            "[shutdown] browser stop failed"
        ),
    }
}
