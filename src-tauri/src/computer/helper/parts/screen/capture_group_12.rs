// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The entire screen, every window `rules` do not allow painted over, shrunk
/// to `max_dimension` — taken again while the windows never shared will not
/// stand still across it (see the module note).
pub async fn capture(
    driver: &DriverProc,
    rules: &ScreenRules,
    max_dimension: Option<u32>,
) -> Result<RawCapture, HelperError> {
    for _ in 0..CAPTURE_ATTEMPTS {
        let before = windows(rules).await?;
        let picture = take_picture(driver).await?;
        let after = windows(rules).await?;
        if refused(&before) != refused(&after) {
            continue;
        }
        let windows: Vec<ScreenWindow> = before.into_iter().chain(after).collect();
        return paint_picture(picture, windows, max_dimension).await;
    }
    Err(HelperError::failed(
        "windows that are never shared kept moving, coming or going while the screen was \
         being captured, so no picture was handed over — try again in a moment",
    ))
}
