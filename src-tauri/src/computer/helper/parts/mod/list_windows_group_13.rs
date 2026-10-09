// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl HelperState {
    /// The normal windows, each joined with its application — with the
    /// minimized ones, and on macOS those of hidden applications, marked
    /// ([`mark_out_of_sight`](Self::mark_out_of_sight)).
    pub(in crate::computer::helper) async fn list_windows(
        &self,
        driver: &DriverProc,
        pid: Option<u32>,
    ) -> Result<Vec<RawWindow>, HelperError> {
        let windows = ops::list_windows(driver, &self.apps, pid).await?;
        Ok(self.mark_out_of_sight(windows).await)
    }

    /// The running applications: on macOS and Windows the owners of the
    /// windows a person could mean ([`ops::apps_of`]), from the listing
    /// [`list_windows`](Self::list_windows) gives; elsewhere the driver's
    /// own list.
    pub(in crate::computer::helper) async fn list_apps(
        &self,
        driver: &DriverProc,
    ) -> Result<Vec<RawApp>, HelperError> {
        #[cfg(any(target_os = "macos", windows))]
        {
            Ok(ops::apps_of(self.list_windows(driver, None).await?))
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            ops::list_apps(driver, &self.apps).await
        }
    }

    /// `windows`, with the minimized ones and those of hidden applications
    /// marked — on macOS when this helper may ask Accessibility, which only a
    /// process started for the purpose can establish (see
    /// [`permissions`](Self::permissions)); without it they stay unmarked,
    /// and unlisted. On X11 the window manager is asked. Elsewhere the
    /// driver's listing says all it can.
    pub(in crate::computer::helper) async fn mark_out_of_sight(
        &self,
        mut windows: Vec<RawWindow>,
    ) -> Vec<RawWindow> {
        #[cfg(target_os = "macos")]
        if self.permissions(false).await.accessibility {
            ops::mark_out_of_sight(&mut windows).await;
        }
        #[cfg(all(target_os = "linux", feature = "computer-executor"))]
        ops::mark_out_of_sight(&mut windows).await;
        #[cfg(not(any(
            target_os = "macos",
            all(target_os = "linux", feature = "computer-executor")
        )))]
        let _ = &mut windows;
        windows
    }

    /// Whether `pid`'s window `window_id` is minimized or its application
    /// hidden, when this helper may ask Accessibility and the application
    /// says.
    #[cfg(target_os = "macos")]
    pub(in crate::computer::helper) async fn out_of_sight(
        &self,
        pid: u32,
        window_id: u64,
    ) -> Option<axwin::OutOfSight> {
        if !self.permissions(false).await.accessibility {
            return None;
        }
        axwin::out_of_sight(pid, window_id).await
    }

    /// The person's `stop`-th Stop: kill the driver started for a request
    /// from before it now, whatever that driver is doing — unlike
    /// [`shutdown`], it is given no time to finish what it is in the middle
    /// of, and every call waiting on it fails at once. A driver started for
    /// a request from after this Stop is left be. A driver still starting
    /// cannot be in the middle of anything; one started for a request from
    /// before the Stop stops itself when it has (see [`driver`](Self::driver)).
    ///
    /// [`shutdown`]: Self::shutdown
    pub(in crate::computer::helper) async fn halt(&self, stop: u64) {
        let mut slot = self.driver.lock().await;
        if !slot.as_ref().is_some_and(|d| d.stop < stop) {
            return;
        }
        let driver = slot.take();
        drop(slot);
        self.snapshots().clear();
        self.forget_driver_saw();
        if let Some(driver) = driver {
            driver.proc.kill().await;
        }
    }
}
