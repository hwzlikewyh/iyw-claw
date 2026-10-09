// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A screenshot of one window, and nothing around it: the driver captures the
/// window's own pixels, so nothing of the windows beside or under it can end
/// up in the image.
///
/// Always captured at the window's own size — the driver is configured with
/// no ceiling, and asked for none (see `driver_proc`'s module note: a capture
/// at any other size would change how the driver maps every later click's
/// coordinates) — and shrunk here to `max_dimension`.
///
/// Taken without making it the window's snapshot wherever that can be done.
/// The driver keeps one snapshot of each window, and a capture taken as one
/// replaces the snapshot the agent's refs name, leaving nothing for them to
/// name. On macOS and Windows `verify_state` reads the window that way; the
/// window's bounds and title then come from its listing — read before the
/// capture and after it, and a point is not aimed by a capture the window
/// changed size around — and the capture's scale from its size against them,
/// as the driver reckons it. On Linux that
/// read is cut down to a model-sized image, so the capture is a snapshot of
/// its own there, at the window's size, and replaces the window's snapshot
/// ([`capture_replaces_snapshot`]); one the driver took from the screen
/// rather than from the window — for a pop-up of the application over it —
/// could hold other windows, and is not handed on.
pub async fn capture(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
    max_dimension: Option<u32>,
) -> Result<RawCapture, HelperError> {
    let taken = take_capture(driver, pid, window_id).await?;
    let data = taken.png_base64;
    let shrunk = tokio::task::spawn_blocking(move || shrink_png(&data, max_dimension))
        .await
        .map_err(|e| HelperError::failed(format!("the capture could not be scaled: {e}")))?
        .map_err(|e| HelperError::failed(format!("the capture could not be scaled: {e}")))?;
    let scale = taken.scale.unwrap_or_else(|| {
        reckoned_scale(
            shrunk.native_width,
            &taken.bounds,
            cfg!(target_os = "macos"),
        )
    });
    let full_size = driver.full_size_captures()
        && taken.steady
        && is_whole_window(
            shrunk.native_width,
            shrunk.native_height,
            &taken.bounds,
            scale,
        );
    Ok(RawCapture {
        png_base64: shrunk.png_base64,
        width: shrunk.width,
        height: shrunk.height,
        native_width: shrunk.native_width,
        native_height: shrunk.native_height,
        full_size,
        window_bounds: taken.bounds,
        title: taken.title,
    })
}

/// Give the driver a capture of the window to aim points by, without a walk
/// of its tree: a snapshot holding the capture alone, which takes the place
/// of the window's snapshot. `Ok(true)`: it did, so the refs from the one
/// before name nothing any more. `Ok(false)`: the window could not be
/// captured, and its snapshot is as it was.
pub async fn publish_capture(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
) -> Result<bool, HelperError> {
    let args = json!({
        "pid": pid,
        "window_id": window_id,
        "include_screenshot": true,
        "include_accessibility_tree": false,
        "max_image_dimension": 0,
    });
    let result = call(driver, "get_window_state", args, WINDOW_STATE_TIMEOUT).await?;
    Ok(result.image().is_some())
}

/// Whether capturing a window replaces the driver's snapshot of it — the one
/// its refs name: only on Linux (see [`capture`]).
pub fn capture_replaces_snapshot() -> bool {
    cfg!(target_os = "linux")
}
