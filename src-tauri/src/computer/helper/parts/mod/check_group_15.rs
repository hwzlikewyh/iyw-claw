// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Delivery {
    /// Whether the action may go out now.
    pub fn check(&self) -> Result<(), HelperError> {
        if self.stopped.load(Ordering::Acquire) > self.stop {
            return Err(stopped());
        }
        if crate::computer::procinfo::process_start(self.pid) != Some(self.started_at) {
            return Err(HelperError::new(
                HelperErrorCode::NoSuchWindow,
                "the window's process is gone",
            ));
        }
        if let Some(run) = self.content {
            if self.content_start(run) != Some(run.started_at) {
                return Err(HelperError::new(
                    HelperErrorCode::NoSuchWindow,
                    "the application that was in the window is gone from it",
                ));
            }
        }
        match session::state() {
            session::SessionState::Unlocked => Ok(()),
            session::SessionState::Locked => Err(HelperError::new(
                HelperErrorCode::Paused,
                "The screen is locked, or another user's session is active.",
            )),
            session::SessionState::Unknown => Err(HelperError::new(
                HelperErrorCode::ActionFailed,
                "iyw-claw cannot tell whether this desktop's session is locked, so it does not act \
                 on windows here; retrying will not change that. Reading windows still works.",
            )),
        }
    }

    /// The start stamp of `run`, the process drawing inside the window — on
    /// Windows read through a handle held while the frame is found still
    /// showing that process, where it says (see `hwnd::frame_holds`): a frame
    /// showing another is not the window that was shared. Elsewhere no window
    /// has another process drawing inside it.
    pub(in crate::computer::helper) fn content_start(&self, run: ProcessRun) -> Option<u64> {
        #[cfg(windows)]
        {
            crate::computer::procinfo::process_start_while(run.pid, || {
                hwnd::frame_holds(self.window_id, self.pid, run.pid) != Some(false)
            })
        }
        #[cfg(not(windows))]
        {
            let _ = self.window_id;
            crate::computer::procinfo::process_start(run.pid)
        }
    }
}
