// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Running applications, each stamped with its start time: the driver's own
/// list. On macOS and Windows that list is not read — on macOS it is frozen
/// at the driver's first call, and on Windows it knows most processes by
/// their executable's file name alone (see `appident`) — and the
/// applications are the owners of the windows instead ([`apps_of`]).
#[cfg(not(any(target_os = "macos", windows)))]
pub async fn list_apps(
    driver: &DriverProc,
    _cache: &tokio::sync::Mutex<AppCache>,
) -> Result<Vec<RawApp>, HelperError> {
    driver_apps(driver).await
}
