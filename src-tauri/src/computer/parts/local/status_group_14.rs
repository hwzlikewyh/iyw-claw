#[allow(unused_imports)]
use super::*;

impl LocalBackend {
    pub(in crate::computer::local) async fn backend_status(&self) -> BackendStatus {
        self.status
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    pub(in crate::computer::local) async fn backend_permissions(
        &self,
    ) -> Result<PermissionReport, BackendError> {
        self.call(HelperOp::Permissions).await
    }
    pub(in crate::computer::local) async fn backend_request_permission(
        &self,
        permission: OsPermission,
    ) -> Result<PermissionAsked, BackendError> {
        #[cfg(target_os = "macos")]
        {
            // Only while computer use is on, as for anything else the helper
            // does: the person pressed the button in a panel that says so.
            if !self.switched_on().await {
                return Err(BackendError::Unavailable(
                    "computer use is switched off".into(),
                ));
            }
            // The copy the running helper is started from, so the request
            // names the principal that will use the grant.
            let helper = helper_to_run().await?;
            ask_for_permission(&helper, permission).await
        }
        #[cfg(not(target_os = "macos"))]
        {
            // Nothing here is granted per application.
            let _ = permission;
            Ok(PermissionAsked { prompted: false })
        }
    }
    pub(in crate::computer::local) async fn backend_list_apps(
        &self,
    ) -> Result<Vec<RawApp>, BackendError> {
        self.call(HelperOp::ListApps).await
    }
    pub(in crate::computer::local) async fn backend_find_app(
        &self,
        name: Option<String>,
        key: Option<String>,
    ) -> Result<InstalledApp, BackendError> {
        self.call(HelperOp::FindApp { name, key }).await
    }
    pub(in crate::computer::local) async fn backend_launch_app(
        &self,
        app: InstalledApp,
        stop: u64,
    ) -> Result<RawLaunch, BackendError> {
        let stopped = || {
            BackendError::Refused(
                ActRefusal::Stopped,
                "The user pressed Stop in iyw-claw's Computer use panel.".into(),
            )
        };
        // As an action: checked before a helper is started for it and again
        // with the helper in hand, and sent once — a start that may have
        // happened is not started again.
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        let connection = self.connection().await?;
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        let reply = connection
            .request(HelperOp::LaunchApp { app }, stop)
            .await?;
        self.decode(reply).await
    }
    pub(in crate::computer::local) async fn backend_list_windows(
        &self,
        pid: Option<u32>,
    ) -> Result<Vec<RawWindow>, BackendError> {
        self.call(HelperOp::ListWindows { pid }).await
    }
    pub(in crate::computer::local) async fn backend_process_start(
        &self,
        pid: u32,
    ) -> Result<Option<u64>, BackendError> {
        self.call(HelperOp::ProcessStart { pid }).await
    }
    pub(in crate::computer::local) async fn backend_capture(
        &self,
        pid: u32,
        window_id: u64,
        max_dimension: Option<u32>,
    ) -> Result<RawCapture, BackendError> {
        self.call(HelperOp::Capture {
            pid,
            window_id,
            max_dimension,
        })
        .await
    }
    pub(in crate::computer::local) async fn backend_snapshot(
        &self,
        pid: u32,
        window_id: u64,
        options: SnapshotOptions,
    ) -> Result<RawSnapshot, BackendError> {
        self.call(HelperOp::Snapshot {
            pid,
            window_id,
            max_depth: options.max_depth,
            max_elements: options.max_elements,
            query: options.query,
            app_menus: options.app_menus,
        })
        .await
    }
    pub(in crate::computer::local) async fn backend_verify(
        &self,
        pid: u32,
        window_id: u64,
        request: VerifyRequest,
    ) -> Result<RawVerify, BackendError> {
        self.call(HelperOp::Verify {
            pid,
            window_id,
            request,
        })
        .await
    }
    pub(in crate::computer::local) async fn backend_act(
        &self,
        pid: u32,
        window_id: u64,
        started_at: u64,
        content: Option<ProcessRun>,
        app_key: Option<String>,
        action: WindowAction,
        delivery: ActDelivery,
        clipboard: ClipboardUse,
        stop: u64,
    ) -> Result<RawAct, BackendError> {
        let stopped = || {
            BackendError::Refused(
                ActRefusal::Stopped,
                "The user pressed Stop in iyw-claw's Computer use panel.".into(),
            )
        };
        // Before a helper is started for it, and again with the helper in
        // hand, as late as iyw-claw can: a Stop that came while one was being
        // started stops this action too. The helper holds it to the same
        // count, for a Stop that overtakes it on the way there.
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        let connection = self.connection().await?;
        if self.stopped.load(Ordering::Acquire) > stop {
            return Err(stopped());
        }
        let reply = connection
            .request(
                HelperOp::Act {
                    pid,
                    window_id,
                    started_at,
                    content,
                    app_key,
                    action,
                    delivery,
                    clipboard,
                },
                stop,
            )
            .await?;
        self.decode(reply).await
    }
    pub(in crate::computer::local) async fn backend_capture_screen(
        &self,
        rules: ScreenRules,
        max_dimension: Option<u32>,
    ) -> Result<RawCapture, BackendError> {
        self.call(HelperOp::CaptureScreen {
            rules,
            max_dimension,
        })
        .await
    }
}
