// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The driver's list of running applications (Linux, where it is read afresh
/// on every call).
#[cfg(not(any(target_os = "macos", windows)))]
pub(super) async fn driver_apps(driver: &DriverProc) -> Result<Vec<RawApp>, HelperError> {
    let result = call(driver, "list_apps", json!({}), LIST_TIMEOUT).await?;
    parse_apps(structured("list_apps", &result)?)
}
