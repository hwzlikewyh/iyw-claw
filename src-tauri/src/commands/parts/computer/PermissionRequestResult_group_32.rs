// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What asking for a permission did.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestResult {
    /// The helper's permissions once the system had been asked.
    pub report: PermissionReport,
    /// The system put up its own dialog for it, which has a button to the
    /// right pane of System Settings.
    pub prompted: bool,
}

/// Raise the system's request for one permission — that one alone — charged
/// to the helper, and say where that leaves things.
pub async fn computer_request_permission_core(
    service: &ComputerService,
    permission: OsPermission,
) -> Result<PermissionRequestResult, AppCommandError> {
    let PermissionAsked { prompted } = service
        .backend
        .request_permission(permission)
        .await
        .map_err(backend_error)?;
    let report = service.backend.permissions().await.map_err(backend_error)?;
    Ok(PermissionRequestResult { report, prompted })
}
