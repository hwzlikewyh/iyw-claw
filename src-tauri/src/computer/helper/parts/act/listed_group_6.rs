// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The driver's listing of one window now, as it gave it.
pub(in crate::computer::helper) async fn listed(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
) -> Result<Value, HelperError> {
    let result = driver
        .call(
            "list_windows",
            json!({ "pid": pid, "on_screen_only": false }),
            MEASURE_TIMEOUT,
        )
        .await?;
    if result.is_error {
        return Err(crate::computer::helper::ops::tool_error(
            "list_windows",
            &result,
        ));
    }
    result
        .structured
        .as_ref()
        .and_then(|s| s.get("windows"))
        .and_then(Value::as_array)
        .and_then(|windows| {
            windows
                .iter()
                .find(|w| w.get("window_id").and_then(Value::as_u64) == Some(window_id))
        })
        .cloned()
        .ok_or_else(|| HelperError::new(HelperErrorCode::NoSuchWindow, "the window is gone"))
}
