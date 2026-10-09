// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Linux: a `get_window_state` at the window's own size, which becomes the
/// window's snapshot.
#[cfg(target_os = "linux")]
pub(super) async fn take_capture(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
) -> Result<Taken, HelperError> {
    let args = json!({
        "pid": pid,
        "window_id": window_id,
        "include_screenshot": true,
        "include_accessibility_tree": false,
        "max_image_dimension": 0,
    });
    let result = call(driver, "get_window_state", args, WINDOW_STATE_TIMEOUT).await?;
    let meta = structured("get_window_state", &result)?;
    if meta.get("screenshot_composited").and_then(Value::as_bool) == Some(true) {
        return Err(composited_capture());
    }
    let Some((data, mime)) = result.image() else {
        // The capture half failed and the driver said why beside an
        // otherwise successful answer.
        let why = meta
            .pointer("/screenshot_error/reason")
            .or_else(|| meta.get("screenshot_error"))
            .map(|e| e.to_string())
            .unwrap_or_else(|| "no image came back".to_string());
        return Err(HelperError::failed(format!(
            "the window could not be captured: {why}"
        )));
    };
    if mime != "image/png" {
        return Err(HelperError::failed(format!(
            "the capture came back as {mime}"
        )));
    }
    Ok(Taken {
        png_base64: data.to_string(),
        steady: true,
        bounds: meta.get("window_bounds").and_then(rect).unwrap_or_default(),
        scale: meta
            .get("screenshot_scale")
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite() && *s > 0.0),
        title: string(meta, "window_title"),
    })
}

/// A Linux capture the driver took from the screen, not from the window.
#[cfg(target_os = "linux")]
pub(super) fn composited_capture() -> HelperError {
    HelperError::new(
        HelperErrorCode::Occluded,
        "A pop-up of the window's application is over it, and a picture of it now would be \
         taken from the screen, where other windows could be in it — so none was taken. Close \
         the pop-up, or wait for it to close, then take the screenshot again.",
    )
}
