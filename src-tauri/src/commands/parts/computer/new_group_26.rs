// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl McpComputerTools {
    pub fn new(service: Arc<ComputerService>) -> Self {
        Self { service }
    }
}

#[async_trait::async_trait]
impl ComputerToolAccess for McpComputerTools {
    async fn list_apps(&self) -> ComputerAppsOutcome {
        self.service.agent_list_apps().await
    }

    async fn list_windows(&self, pid: Option<u32>) -> ComputerWindowsOutcome {
        self.service.agent_list_windows(pid).await
    }

    async fn capture(&self, target_id: &str, max_dimension: Option<u32>) -> ComputerCaptureOutcome {
        self.service.agent_capture(target_id, max_dimension).await
    }

    async fn snapshot(&self, target_id: &str, request: SnapshotRequest) -> ComputerSnapshotOutcome {
        self.service.agent_snapshot(target_id, request).await
    }

    async fn verify(&self, target_id: &str, request: VerifyRequest) -> ComputerVerifyOutcome {
        self.service.agent_verify(target_id, request).await
    }

    async fn launch_app(&self, name: Option<String>, key: Option<String>) -> ComputerLaunchOutcome {
        self.service.agent_launch_app(name, key).await
    }

    async fn clipboard(&self, op: ClipboardOp) -> ComputerClipboardOutcome {
        self.service.agent_clipboard(op).await
    }

    async fn act(
        &self,
        target_id: &str,
        request: ComputerActRequest,
        delivery: Option<ActDelivery>,
    ) -> ComputerActOutcome {
        self.service.agent_act(target_id, request, delivery).await
    }
}
