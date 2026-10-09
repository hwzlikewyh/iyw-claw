#[allow(unused_imports)]
use super::*;

impl LocalBackend {
    pub(in crate::computer::local) async fn backend_act_screen(
        &self,
        rules: ScreenRules,
        action: WindowAction,
        geometry: ScreenGeometry,
        stop: u64,
        still: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<RawAct, BackendError> {
        let stopped = || {
            BackendError::Refused(
                ActRefusal::Stopped,
                "The user pressed Stop in iyw-claw's Computer use panel.".into(),
            )
        };
        // As an action on a window: checked before a helper is started for
        // it and again with the helper in hand, and sent once. Starting the
        // helper, or its driver, can take seconds, in which the sharing may
        // have been taken back: both are had first, and `still` asked after
        // — the action then starts nothing (`HelperOp::ActScreen`).
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        let connection = self.connection().await?;
        let ready = connection.request(HelperOp::DriverReady, stop).await?;
        self.decode::<()>(ready).await?;
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        if !still() {
            return Err(BackendError::Refused(
                ActRefusal::Revoked,
                "The entire screen's sharing changed before the action went out — taken back, \
                 lowered to reading, or the never-share list grew — so nothing was sent."
                    .into(),
            ));
        }
        let reply = connection
            .request(
                HelperOp::ActScreen {
                    rules,
                    action,
                    geometry,
                },
                stop,
            )
            .await?;
        self.decode(reply).await
    }
    pub(in crate::computer::local) async fn backend_clipboard_read(
        &self,
        expect: u64,
    ) -> Result<RawClipboard, BackendError> {
        self.call(HelperOp::ClipboardRead { expect }).await
    }
    pub(in crate::computer::local) async fn backend_clipboard_write(
        &self,
        text: String,
        stop: u64,
    ) -> Result<u64, BackendError> {
        let stopped = || {
            BackendError::Refused(
                ActRefusal::Stopped,
                "The user pressed Stop in iyw-claw's Computer use panel.".into(),
            )
        };
        // As an action: the person's clipboard is changed by it, once.
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        let connection = self.connection().await?;
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        let reply = connection
            .request(HelperOp::ClipboardWrite { text }, stop)
            .await?;
        self.decode(reply).await
    }
    pub(in crate::computer::local) async fn backend_halt(
        &self,
        stop: u64,
    ) -> Result<(), BackendError> {
        self.note_stop(stop);
        self.to_running(HelperOp::Halt { stop }, stop).await
    }
}
