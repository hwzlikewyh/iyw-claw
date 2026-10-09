// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The running driver, and the Stop count of the request that started it —
/// so a Stop from after that request ends it and one from before does not.
pub(super) struct RunningDriver {
    pub(in crate::computer::helper) proc: Arc<DriverProc>,
    pub(in crate::computer::helper) stop: u64,
}

/// What the running helper holds between requests.
pub(super) struct HelperState {
    /// Set by `Configure`.
    pub(in crate::computer::helper) driver_path: Mutex<Option<PathBuf>>,
    /// The running driver, started on first use and again after it exits.
    pub(in crate::computer::helper) driver: Mutex<Option<RunningDriver>>,
    pub(in crate::computer::helper) apps: Mutex<AppCache>,
    /// The latest snapshot of each window, as the running driver keeps them.
    pub(in crate::computer::helper) snapshots: std::sync::Mutex<SnapshotBook>,
    /// The latest of the person's Stops iyw-claw has told of (`Halt::stop`),
    /// moved the moment its frame is read — and to [`STOP_ALL`] when iyw-claw
    /// goes. Nothing of a request from before it (`HelperRequest::stop`)
    /// reaches a driver after that, whichever frame arrived first; what is
    /// from after it runs as usual. Shared with each action's [`Delivery`].
    pub(in crate::computer::helper) stopped: Arc<AtomicU64>,
    /// The helper's permissions as the system last answered, and when. Never
    /// asked in this process on macOS: a process keeps the first "not
    /// granted" it hears for the rest of its life (see [`permissions`]).
    ///
    /// [`permissions`]: Self::permissions
    pub(in crate::computer::helper) permissions: Mutex<Option<(PermissionReport, Instant)>>,
    /// The permissions in force when the running driver started, while one
    /// runs.
    pub(in crate::computer::helper) driver_saw: std::sync::Mutex<Option<PermissionReport>>,
    /// A permission is in force that the running driver started without. The
    /// driver may still hold the system's earlier "no" — for Screen Recording
    /// macOS keeps it until the process ends — so the next call that needs
    /// the driver starts a fresh one.
    pub(in crate::computer::helper) driver_stale: AtomicBool,
}
