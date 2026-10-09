// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

#[async_trait]
impl ComputerToolAccess for NoComputerDesktop {
    async fn list_apps(&self) -> ComputerAppsOutcome {
        ComputerAppsOutcome::refused(ERROR_UNAVAILABLE, NO_DESKTOP_NOTE)
    }

    async fn list_windows(&self, _pid: Option<u32>) -> ComputerWindowsOutcome {
        ComputerWindowsOutcome::refused(ERROR_UNAVAILABLE, NO_DESKTOP_NOTE)
    }

    async fn capture(&self, target_id: &str, _max: Option<u32>) -> ComputerCaptureOutcome {
        ComputerCaptureOutcome::refused(target_id, ERROR_UNAVAILABLE, NO_DESKTOP_NOTE)
    }

    async fn snapshot(
        &self,
        target_id: &str,
        _request: SnapshotRequest,
    ) -> ComputerSnapshotOutcome {
        ComputerSnapshotOutcome::refused(target_id, ERROR_UNAVAILABLE, NO_DESKTOP_NOTE)
    }

    async fn verify(&self, target_id: &str, _request: VerifyRequest) -> ComputerVerifyOutcome {
        ComputerVerifyOutcome::refused(target_id, ERROR_UNAVAILABLE, NO_DESKTOP_NOTE)
    }

    async fn act(
        &self,
        target_id: &str,
        _request: ComputerActRequest,
        _delivery: Option<ActDelivery>,
    ) -> ComputerActOutcome {
        ComputerActOutcome::refused(target_id, ERROR_UNAVAILABLE, NO_DESKTOP_NOTE)
    }

    async fn launch_app(
        &self,
        _name: Option<String>,
        _key: Option<String>,
    ) -> ComputerLaunchOutcome {
        ComputerLaunchOutcome::refused(ERROR_UNAVAILABLE, NO_DESKTOP_NOTE)
    }

    async fn clipboard(&self, _op: ClipboardOp) -> ComputerClipboardOutcome {
        ComputerClipboardOutcome::refused(ERROR_UNAVAILABLE, NO_DESKTOP_NOTE)
    }
}
