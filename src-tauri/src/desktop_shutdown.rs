use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use tauri::Manager;

use crate::acp::manager::ConnectionManager;
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
        Ok(worker) if worker.join().is_ok() => true,
        Ok(_) => {
            tracing::error!(
                shutdown_reason = reason.as_str(),
                "[shutdown] worker panicked"
            );
            false
        }
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
    stop_entrypoints(app, reason).await;
    stop_terminals(app, reason);
    stop_office_watchers(reason);
    stop_browser(app, reason).await;
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
