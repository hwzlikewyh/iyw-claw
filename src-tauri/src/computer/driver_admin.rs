//! The driver as a person manages it from Settings: which release this iyw-claw
//! runs, which are in the cache, fetching it, clearing older ones — and,
//! while any of that is under way, how far it has got.
//!
//! Only ever the pinned release (`driver`): there is no choosing another, so
//! "upgrade" is fetching the release this iyw-claw pins where the cache holds
//! only an older one, left by an earlier iyw-claw (and left in place: another
//! iyw-claw on the machine may still run it). A download a starting helper
//! makes for itself (`local::LocalBackend`) is shown here as well, followed
//! through the backend's status.
//!
//! Removing the driver is the service's (`ComputerService::uninstall_driver`),
//! which switches computer use off and stops the helper first; it claims the
//! driver here like an install does, so the two cannot overlap.

use std::sync::Mutex;

use serde::Serialize;

use super::backend::{BackendState, BackendStatus};
use super::driver;
use super::events::ComputerEvents;

/// cua-driver, as the settings page shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverInfo {
    /// The release this iyw-claw runs.
    pub version: String,
    /// Whether that release has a build for this platform.
    pub supported: bool,
    /// The releases in the cache, newest first.
    pub installed: Vec<String>,
    /// Where the pinned release's executable is, once it is in the cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// What is being done to it right now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<DriverTask>,
    /// How the last install or removal failed, until the next one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DriverTask {
    /// Fetching the pinned release: megabytes so far, and in all, once the
    /// download has said.
    #[serde(rename_all = "camelCase")]
    Installing {
        #[serde(skip_serializing_if = "Option::is_none")]
        downloaded_mb: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_mb: Option<f64>,
    },
    Uninstalling,
}

impl DriverTask {
    const FETCHING: DriverTask = DriverTask::Installing {
        downloaded_mb: None,
        total_mb: None,
    };
}

#[derive(Default)]
struct Progress {
    /// What a person asked for, while it runs.
    task: Option<DriverTask>,
    /// A starting helper is fetching the driver for itself.
    helper_fetching: bool,
    error: Option<String>,
}

impl Progress {
    /// What the page shows: what the person asked for, or else a helper's
    /// fetch, which is an install as far as they are concerned.
    fn shown_task(&self) -> Option<DriverTask> {
        self.task
            .or(self.helper_fetching.then_some(DriverTask::FETCHING))
    }
}

pub struct DriverAdmin {
    events: ComputerEvents,
    progress: Mutex<Progress>,
}

impl DriverAdmin {
    pub fn new(events: ComputerEvents) -> Self {
        Self {
            events,
            progress: Mutex::new(Progress::default()),
        }
    }

    fn progress(&self) -> std::sync::MutexGuard<'_, Progress> {
        // Every change is a plain assignment; a poisoned lock still holds a
        // consistent value.
        self.progress.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The driver as it stands, read from the cache now.
    pub fn info(&self) -> DriverInfo {
        let installed = driver::installed_driver_versions().unwrap_or_default();
        let path = driver::cached_driver_path().map(|p| p.to_string_lossy().to_string());
        let progress = self.progress();
        DriverInfo {
            version: driver::DRIVER_VERSION.to_string(),
            supported: driver::artifact_for_current_platform().is_some(),
            installed,
            path,
            task: progress.shown_task(),
            error: progress.error.clone(),
        }
    }

    fn emit(&self) {
        self.events.driver(&self.info());
    }

    /// Claim the driver for `task`, unless an install or a removal already
    /// has it.
    pub fn begin(&self, task: DriverTask) -> Result<(), String> {
        {
            let mut progress = self.progress();
            if progress.task.is_some() {
                return Err("cua-driver is already being installed or removed".into());
            }
            progress.task = Some(task);
            progress.error = None;
        }
        self.emit();
        Ok(())
    }

    /// The claimed task is over, and failed with `error` if there is one.
    pub fn finish(&self, error: Option<String>) {
        {
            let mut progress = self.progress();
            progress.task = None;
            progress.error = error;
        }
        self.emit();
    }

    /// One of a download's progress lines, while an install runs.
    fn progressed(&self, line: &str) {
        let Some((done, total)) = parse_progress(line) else {
            return;
        };
        {
            let mut progress = self.progress();
            if !matches!(progress.task, Some(DriverTask::Installing { .. })) {
                return;
            }
            progress.task = Some(DriverTask::Installing {
                downloaded_mb: Some(done),
                total_mb: total,
            });
        }
        self.emit();
    }

    /// Fetch the pinned release, unless the cache already holds it. Fails if
    /// an install or a removal is under way.
    pub async fn install(&self) -> Result<DriverInfo, String> {
        self.begin(DriverTask::FETCHING)?;
        let result = self.install_runtime().await;
        self.finish(result.as_ref().err().cloned());
        result.map(|()| self.info())
    }

    async fn install_runtime(&self) -> Result<(), String> {
        #[cfg(feature = "tauri-runtime")]
        if let ComputerEvents::Desktop(app) = &self.events {
            crate::managed_environment::change_computer_driver(
                true,
                &crate::web::event_bridge::EventEmitter::Tauri(app.clone()),
                |line| self.progressed(line),
            )
            .await?;
        }
        driver::ensure_driver(|line| self.progressed(line))
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Follow the backend: while a starting helper fetches the driver it is
    /// being installed as far as the page is concerned, and once that is over
    /// the cache may hold something new.
    pub fn backend_moved(&self, status: &BackendStatus) {
        let fetching = status.state == BackendState::Downloading;
        {
            let mut progress = self.progress();
            if progress.helper_fetching == fetching {
                return;
            }
            progress.helper_fetching = fetching;
        }
        self.emit();
    }
}

/// Megabytes so far, and in all when known, from a download's progress line
/// (`binary_cache::download_progress_message`).
fn parse_progress(line: &str) -> Option<(f64, Option<f64>)> {
    let rest = line.strip_prefix("Downloading... ")?;
    let (done, rest) = rest.split_once(" MB")?;
    let done = done.trim().parse::<f64>().ok()?;
    let total = rest
        .strip_prefix(" / ")
        .and_then(|t| t.strip_suffix(" MB"))
        .and_then(|t| t.trim().parse::<f64>().ok());
    Some((done, total))
}
