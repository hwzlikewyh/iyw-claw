use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};
use sea_orm::DatabaseConnection;
use tokio::runtime::Handle;

const RECONCILE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Watch the central Skill store and publish changes to installed, enabled
/// Agents through the existing Rust reconciliation path.
pub fn spawn_central_skill_watcher(conn: DatabaseConnection) {
    let root = crate::commands::experts::central_experts_dir();
    if let Err(error) = std::fs::create_dir_all(&root) {
        tracing::warn!(
            path = %root.display(),
            error = %error,
            "[skills] central watcher could not create Skill store"
        );
        return;
    }

    let runtime = Handle::current();
    thread::spawn(move || run_watcher(root, conn, runtime));
}

fn run_watcher(root: PathBuf, conn: DatabaseConnection, runtime: Handle) {
    let (events_tx, events_rx) = mpsc::channel();
    let callback = move |result: notify::Result<Event>| {
        let _ = events_tx.send(result);
    };
    let mut watcher = match notify::recommended_watcher(callback) {
        Ok(watcher) => watcher,
        Err(error) => {
            tracing::warn!(
                path = %root.display(),
                error = %error,
                "[skills] central watcher could not start"
            );
            return;
        }
    };
    if let Err(error) = watcher.watch(&root, RecursiveMode::Recursive) {
        tracing::warn!(
            path = %root.display(),
            error = %error,
            "[skills] central watcher could not watch Skill store"
        );
        return;
    }

    while let Ok(first) = events_rx.recv() {
        if !event_relevant(&root, &first) {
            continue;
        }
        drain_debounced_events(&root, &events_rx);
        match runtime.block_on(crate::commands::acp::reconcile_shared_market_skills(&conn)) {
            Ok(()) => {
                tracing::info!("[skills] central Skill change published to enabled Agents");
                if let Err(error) =
                    runtime.block_on(crate::plugin_runtime::registry::reconcile_global(&conn))
                {
                    tracing::warn!(error = %error, "[plugin-registry] watcher reconcile failed");
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "[skills] central Skill publication failed")
            }
        }
    }
}

fn drain_debounced_events(root: &Path, events_rx: &mpsc::Receiver<notify::Result<Event>>) {
    let deadline = Instant::now() + RECONCILE_DEBOUNCE;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        match events_rx.recv_timeout(remaining) {
            Ok(event) if event_relevant(root, &event) => {}
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn event_relevant(root: &Path, result: &notify::Result<Event>) -> bool {
    let Ok(event) = result else {
        return true;
    };
    event
        .paths
        .iter()
        .any(|path| relevant_skill_path(root, path))
}

fn relevant_skill_path(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut components = relative.components();
    let Some(first) = components.next() else {
        return false;
    };
    let first = first.as_os_str().to_string_lossy();
    if first.starts_with('.') {
        return false;
    }
    !relative.components().any(|component| {
        matches!(
            component.as_os_str().to_string_lossy().as_ref(),
            ".venv" | "node_modules"
        )
    })
}
