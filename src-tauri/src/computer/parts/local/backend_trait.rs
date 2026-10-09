use super::*;

#[async_trait::async_trait]
impl ComputerBackend for LocalBackend {
    async fn act_screen(
        &self,
        rules: ScreenRules,
        action: WindowAction,
        geometry: ScreenGeometry,
        stop: u64,
        still: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<RawAct, BackendError> {
        self.backend_act_screen(rules, action, geometry, stop, still)
            .await
    }
    async fn clipboard_read(&self, expect: u64) -> Result<RawClipboard, BackendError> {
        self.backend_clipboard_read(expect).await
    }
    async fn clipboard_write(&self, text: String, stop: u64) -> Result<u64, BackendError> {
        self.backend_clipboard_write(text, stop).await
    }
    async fn halt(&self, stop: u64) -> Result<(), BackendError> {
        self.backend_halt(stop).await
    }
    async fn status(&self) -> BackendStatus {
        self.backend_status().await
    }
    async fn permissions(&self) -> Result<PermissionReport, BackendError> {
        self.backend_permissions().await
    }
    async fn request_permission(
        &self,
        permission: OsPermission,
    ) -> Result<PermissionAsked, BackendError> {
        self.backend_request_permission(permission).await
    }
    async fn list_apps(&self) -> Result<Vec<RawApp>, BackendError> {
        self.backend_list_apps().await
    }
    async fn find_app(
        &self,
        name: Option<String>,
        key: Option<String>,
    ) -> Result<InstalledApp, BackendError> {
        self.backend_find_app(name, key).await
    }
    async fn launch_app(&self, app: InstalledApp, stop: u64) -> Result<RawLaunch, BackendError> {
        self.backend_launch_app(app, stop).await
    }
    async fn list_windows(&self, pid: Option<u32>) -> Result<Vec<RawWindow>, BackendError> {
        self.backend_list_windows(pid).await
    }
    async fn process_start(&self, pid: u32) -> Result<Option<u64>, BackendError> {
        self.backend_process_start(pid).await
    }
    async fn capture(
        &self,
        pid: u32,
        window_id: u64,
        max_dimension: Option<u32>,
    ) -> Result<RawCapture, BackendError> {
        self.backend_capture(pid, window_id, max_dimension).await
    }
    async fn snapshot(
        &self,
        pid: u32,
        window_id: u64,
        options: SnapshotOptions,
    ) -> Result<RawSnapshot, BackendError> {
        self.backend_snapshot(pid, window_id, options).await
    }
    async fn verify(
        &self,
        pid: u32,
        window_id: u64,
        request: VerifyRequest,
    ) -> Result<RawVerify, BackendError> {
        self.backend_verify(pid, window_id, request).await
    }
    async fn act(
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
        self.backend_act(
            pid, window_id, started_at, content, app_key, action, delivery, clipboard, stop,
        )
        .await
    }
    async fn capture_screen(
        &self,
        rules: ScreenRules,
        max_dimension: Option<u32>,
    ) -> Result<RawCapture, BackendError> {
        self.backend_capture_screen(rules, max_dimension).await
    }
}
