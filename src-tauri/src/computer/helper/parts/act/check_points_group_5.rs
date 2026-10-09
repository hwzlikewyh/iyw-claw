// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Check that the points of an action are still where they were read: the
/// window is the size it was when the capture they came from was taken —
/// measured once for all of them. Returns where the window is now, when the
/// driver said — for the marker, not for aiming. Nothing to check, nothing
/// measured.
pub async fn check_points(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
    points: &[&WindowPoint],
) -> Result<Option<Rect>, HelperError> {
    if points.is_empty() {
        return Ok(None);
    }
    if !driver.full_size_captures() {
        return Err(HelperError::new(
            HelperErrorCode::ActionFailed,
            "Pointing by coordinates is not available right now. Use a ref from \
             computer_snapshot instead.",
        ));
    }
    let window = listed(driver, pid, window_id).await?;
    let bounds = window
        .get("bounds")
        .ok_or_else(|| HelperError::new(HelperErrorCode::NoSuchWindow, "the window is gone"))?;
    let number = |key: &str| bounds.get(key).and_then(Value::as_f64);
    let (width, height) = (
        number("width").unwrap_or(0.0),
        number("height").unwrap_or(0.0),
    );
    let resized = |point: &&WindowPoint| {
        (width - point.window_width).abs() > 1.0 || (height - point.window_height).abs() > 1.0
    };
    if points.iter().any(resized) {
        return Err(HelperError::new(
            HelperErrorCode::StaleRef,
            "The window has changed size since that screenshot, so its contents are not where \
             they were. Take a new computer_screenshot and use a point from it.",
        ));
    }
    // Where the window is, for the marker: only when the driver said.
    Ok(match (number("x"), number("y")) {
        (Some(x), Some(y)) => Some(Rect {
            x,
            y,
            width,
            height,
        }),
        _ => None,
    })
}
