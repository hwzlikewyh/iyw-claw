// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// macOS and Windows: one read through `verify_state`, which leaves the
/// window's snapshot as it was, between two of the window's listings.
#[cfg(not(target_os = "linux"))]
pub(super) async fn take_capture(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
) -> Result<Taken, HelperError> {
    let before = crate::computer::helper::act::listed(driver, pid, window_id)
        .await?
        .get("bounds")
        .and_then(rect);
    let args = json!({
        "pid": pid,
        "window_id": window_id,
        "expect": [{ "window": { "exists": true } }],
        "timeout_ms": 0,
        "stable_samples": 1,
        "include_screenshot": true,
    });
    let result = call(driver, "verify_state", args, WINDOW_STATE_TIMEOUT).await?;
    let Some((data, mime)) = result.image() else {
        // `verify_state` says nothing of why there is no picture; the
        // listing says whether the window is still there.
        crate::computer::helper::act::listed(driver, pid, window_id).await?;
        return Err(HelperError::failed(
            "the window could not be captured: no image came back",
        ));
    };
    if mime != "image/png" {
        return Err(HelperError::failed(format!(
            "the capture came back as {mime}"
        )));
    }
    let png_base64 = data.to_string();
    let window = crate::computer::helper::act::listed(driver, pid, window_id).await?;
    let bounds = window.get("bounds").and_then(rect);
    Ok(Taken {
        png_base64,
        steady: same_size(before.as_ref(), bounds.as_ref()),
        bounds: bounds.unwrap_or_default(),
        scale: None,
        title: string(&window, "title"),
    })
}
