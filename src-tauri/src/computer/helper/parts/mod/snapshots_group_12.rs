// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl HelperState {
    pub(in crate::computer::helper) fn snapshots(&self) -> std::sync::MutexGuard<'_, SnapshotBook> {
        // Every change to the book is a single insert or removal, so a
        // poisoned lock still guards a consistent book.
        self.snapshots.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Whether a request iyw-claw let through at Stop count `stop` may still
    /// reach a driver: no later Stop has been heard of.
    pub(in crate::computer::helper) fn check_not_stopped(
        &self,
        stop: u64,
    ) -> Result<(), HelperError> {
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        Ok(())
    }

    /// What an action on `pid`'s window `window_id`, let through at Stop
    /// count `stop`, must still find when it goes out (see [`Delivery`]).
    pub(in crate::computer::helper) fn delivery(
        &self,
        pid: u32,
        window_id: u64,
        started_at: u64,
        content: Option<ProcessRun>,
        stop: u64,
    ) -> Delivery {
        Delivery {
            stopped: self.stopped.clone(),
            stop,
            pid,
            window_id,
            started_at,
            content,
        }
    }

    /// The driver running now, for a request iyw-claw let through at Stop count
    /// `stop` — never one started for it: none running, or one a Stop since
    /// has ended, refuses the request, which iyw-claw asks again about after
    /// [`HelperOp::DriverReady`].
    pub(in crate::computer::helper) async fn running_driver(
        &self,
        stop: u64,
    ) -> Result<Arc<DriverProc>, HelperError> {
        self.check_not_stopped(stop)?;
        let slot = self.driver.lock().await;
        self.check_not_stopped(stop)?;
        let stopped = self.stopped.load(Ordering::Acquire);
        slot.as_ref()
            .filter(|d| d.stop >= stopped && d.proc.alive())
            .map(|d| d.proc.clone())
            .ok_or_else(|| {
                HelperError::new(
                    HelperErrorCode::ActionFailed,
                    "The driver restarted just before the action, so nothing was sent; try again.",
                )
            })
    }

    /// The running driver, for a request iyw-claw let through at Stop count
    /// `stop` — starting one if there is none, if the one running started
    /// before a permission it now has (`driver_stale`), or if it started for
    /// a request from before a Stop since heard of (the `Halt` that ends it
    /// may still be on its way to the slot).
    pub(in crate::computer::helper) async fn driver(
        &self,
        stop: u64,
    ) -> Result<Arc<DriverProc>, HelperError> {
        self.check_not_stopped(stop)?;
        let mut slot = self.driver.lock().await;
        // A Stop heard of while this one waited for the slot.
        self.check_not_stopped(stop)?;
        let stopped = self.stopped.load(Ordering::Acquire);
        let stale = self.driver_stale.swap(false, Ordering::AcqRel);
        if !stale {
            if let Some(running) = slot
                .as_ref()
                .filter(|d| d.stop >= stopped && d.proc.alive())
            {
                return Ok(running.proc.clone());
            }
        }
        if let Some(old) = slot.take() {
            self.forget_driver_saw();
            if old.stop >= stopped {
                old.proc.shutdown().await;
            } else {
                self.snapshots().clear();
                old.proc.kill().await;
            }
        }
        let path = self.driver_path.lock().await.clone().ok_or_else(|| {
            HelperError::new(
                HelperErrorCode::NotConfigured,
                "the helper has not been told where the driver is",
            )
        })?;
        let artifact = driver::artifact_for_current_platform().ok_or_else(|| {
            HelperError::new(
                HelperErrorCode::DriverUnavailable,
                "no driver release for this platform",
            )
        })?;
        // What the new driver starts with, so a permission granted later is
        // told apart from one it had all along.
        let had = self.permissions(false).await;
        let launched =
            Arc::new(DriverProc::launch(&path, artifact, || self.check_not_stopped(stop)).await?);
        // A Stop that arrived while this one was starting stops it too.
        if let Err(halted) = self.check_not_stopped(stop) {
            launched.shutdown().await;
            return Err(halted);
        }
        // A fresh driver has taken no snapshots.
        self.snapshots().clear();
        *self.driver_saw.lock().unwrap_or_else(|p| p.into_inner()) = Some(had);
        // A check that ran while this one was starting, and found more than
        // it started with, compared itself with no driver: compare now.
        if self
            .permissions
            .lock()
            .await
            .as_ref()
            .is_some_and(|(last, _)| gained(&had, last))
        {
            self.driver_stale.store(true, Ordering::Release);
        }
        *slot = Some(RunningDriver {
            proc: launched.clone(),
            stop,
        });
        Ok(launched)
    }

    pub(in crate::computer::helper) fn forget_driver_saw(&self) {
        *self.driver_saw.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    pub(in crate::computer::helper) async fn shutdown(&self) {
        let driver = self.driver.lock().await.take();
        self.snapshots().clear();
        self.forget_driver_saw();
        if let Some(driver) = driver {
            driver.proc.shutdown().await;
        }
    }

    /// The helper's own OS permissions. `fresh` asks the system now;
    /// otherwise a recent answer stands — one that both are granted until a
    /// driver call says otherwise (see [`handle`]), one that something is
    /// missing for [`RECHECK_MISSING`].
    ///
    /// On macOS the system is asked in a process started for the purpose
    /// (`driver_proc::probe_permissions`), never in this one: macOS keeps a
    /// process's first "not granted" for its whole life, and a helper that
    /// asked itself would go on reporting a permission missing after the
    /// person had granted it — and could not use it itself (see `axwin`).
    /// When no such process can be started, the last answer stands for now
    /// and the next call asks again; with none, nothing is granted. A
    /// permission that has appeared since the running driver started marks
    /// that driver stale.
    pub(in crate::computer::helper) async fn permissions(&self, fresh: bool) -> PermissionReport {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = fresh;
            PermissionReport {
                required: false,
                accessibility: true,
                screen_recording: true,
            }
        }
        #[cfg(target_os = "macos")]
        {
            let mut last = self.permissions.lock().await;
            if !fresh {
                if let Some((report, at)) = last.as_ref() {
                    if answer_stands(report, at.elapsed()) {
                        return *report;
                    }
                }
            }
            let Some(report) = self.ask_system().await else {
                return last.as_ref().map_or(
                    PermissionReport {
                        required: true,
                        accessibility: false,
                        screen_recording: false,
                    },
                    |(report, _)| *report,
                );
            };
            if self
                .driver_saw
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .as_ref()
                .is_some_and(|saw| gained(saw, &report))
            {
                self.driver_stale.store(true, Ordering::Release);
            }
            *last = Some((report, Instant::now()));
            report
        }
    }

    /// Ask macOS, in a fresh process; `None` where that cannot be done — no
    /// driver configured yet, or one that would not start. Never asked here
    /// instead: this process would keep a "not granted" for the rest of its
    /// life, and it makes Accessibility calls of its own.
    #[cfg(target_os = "macos")]
    pub(in crate::computer::helper) async fn ask_system(&self) -> Option<PermissionReport> {
        let path = self.driver_path.lock().await.clone()?;
        match driver_proc::probe_permissions(&path).await {
            Ok(report) => Some(report),
            Err(e) => {
                tracing::warn!(
                    "could not check permissions in a fresh process: {}",
                    e.message
                );
                None
            }
        }
    }

    /// Refuse an op up front when the helper lacks the permission it needs,
    /// so the answer names the permission instead of being whatever the
    /// driver makes of a failed system call.
    pub(in crate::computer::helper) async fn require(
        &self,
        permission: OsPermission,
    ) -> Result<(), HelperError> {
        if self.permissions(false).await.has(permission) {
            Ok(())
        } else {
            Err(HelperError::permission_missing(permission))
        }
    }
}
