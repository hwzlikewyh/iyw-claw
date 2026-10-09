// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What the driver's picture of the screen holds: the PNG, and the screen's
/// size in desktop units.
pub(super) struct Picture {
    pub(in crate::computer::helper::screen) png_base64: String,
    pub(in crate::computer::helper::screen) screen_width: f64,
    pub(in crate::computer::helper::screen) screen_height: f64,
}

/// The windows `rules` refuse, by number and frame, in one order: what must
/// stand still across a picture.
pub(super) fn refused(windows: &[ScreenWindow]) -> Vec<(u64, [u64; 4])> {
    let mut out: Vec<(u64, [u64; 4])> = windows
        .iter()
        .filter(|w| !w.allowed)
        .map(|w| {
            let b = w.bounds;
            (
                w.id,
                [
                    b.x.to_bits(),
                    b.y.to_bits(),
                    b.width.to_bits(),
                    b.height.to_bits(),
                ],
            )
        })
        .collect();
    out.sort_unstable();
    out
}

/// The driver's picture of the screen, as it is.
pub(super) async fn take_picture(driver: &DriverProc) -> Result<Picture, HelperError> {
    let result = driver
        .call("get_desktop_state", json!({}), CAPTURE_TIMEOUT)
        .await?;
    if result.is_error {
        return Err(crate::computer::helper::ops::tool_error(
            "get_desktop_state",
            &result,
        ));
    }
    let said = result.structured.clone().unwrap_or(Value::Null);
    let number = |key: &str| {
        said.get(key)
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite() && *n > 0.0)
    };
    let (Some(screen_width), Some(screen_height)) =
        (number("screen_width"), number("screen_height"))
    else {
        return Err(HelperError::failed(
            "the screen capture came back without the screen's size",
        ));
    };
    let Some((data, mime)) = result.image() else {
        return Err(HelperError::failed(
            "the screen capture came back without an image",
        ));
    };
    if mime != "image/png" {
        return Err(HelperError::failed(format!(
            "the screen capture came back as {mime}"
        )));
    }
    Ok(Picture {
        png_base64: data.to_string(),
        screen_width,
        screen_height,
    })
}

/// `picture`, every one of `windows` the rules refuse painted over, shrunk to
/// `max_dimension`.
pub(super) async fn paint_picture(
    picture: Picture,
    windows: Vec<ScreenWindow>,
    max_dimension: Option<u32>,
) -> Result<RawCapture, HelperError> {
    let Picture {
        png_base64,
        screen_width,
        screen_height,
    } = picture;
    let masked = tokio::task::spawn_blocking(move || {
        let painted = paint_over(&png_base64, &windows, screen_width)?;
        shrink_png(&painted, max_dimension)
    })
    .await
    .map_err(|e| HelperError::failed(format!("the screen capture could not be prepared: {e}")))?
    .map_err(|e| HelperError::failed(format!("the screen capture could not be prepared: {e}")))?;
    Ok(RawCapture {
        png_base64: masked.png_base64,
        width: masked.width,
        height: masked.height,
        native_width: masked.native_width,
        native_height: masked.native_height,
        full_size: true,
        window_bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: screen_width,
            height: screen_height,
        },
        title: None,
    })
}

/// Paint every window not allowed over in the PNG, at its frame scaled from
/// desktop units to the picture's pixels (`screen_width` units across).
pub(super) fn paint_over(
    png_base64: &str,
    windows: &[ScreenWindow],
    screen_width: f64,
) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use image::ImageFormat;

    let bytes = STANDARD
        .decode(png_base64)
        .map_err(|e| format!("not base64: {e}"))?;
    let mut image = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
        .map_err(|e| format!("not a PNG: {e}"))?
        .to_rgba8();
    let ratio = f64::from(image.width()) / screen_width;
    let (width, height) = (image.width(), image.height());
    for window in windows.iter().filter(|w| !w.allowed) {
        let reach = window.reach();
        let clamp = |v: f64, max: u32| (v.max(0.0) as u32).min(max);
        let left = clamp((reach.x * ratio).floor(), width);
        let top = clamp((reach.y * ratio).floor(), height);
        let right = clamp(((reach.x + reach.width) * ratio).ceil(), width);
        let bottom = clamp(((reach.y + reach.height) * ratio).ceil(), height);
        for y in top..bottom {
            for x in left..right {
                image.put_pixel(x, y, image::Rgba([24, 24, 27, 255]));
            }
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| format!("png encoding failed: {e}"))?;
    Ok(STANDARD.encode(out.into_inner()))
}

/// Where a point of the picture goes: to the driver — on macOS in the
/// picture's own pixels, which the driver reads against its own picture of
/// the screen; elsewhere in the screen's own pixels — and, in desktop units,
/// where that lands on the screen. Whole pixels, as the driver takes them.
pub(super) fn landing(point: &WindowPoint, scale: f64) -> Landing {
    if cfg!(target_os = "macos") {
        let (x, y) = (point.x.floor(), point.y.floor());
        Landing {
            driver: (x, y),
            screen: (x / scale, y / scale),
        }
    } else {
        let (x, y) = ((point.x / scale).floor(), (point.y / scale).floor());
        Landing {
            driver: (x, y),
            screen: (x, y),
        }
    }
}

/// See [`landing`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Landing {
    pub(in crate::computer::helper::screen) driver: (f64, f64),
    pub(in crate::computer::helper::screen) screen: (f64, f64),
}

/// Whether a point on the screen, in desktop units, is clear of every window
/// the rules do not allow that a click could reach: of what the picture
/// paints over, whatever is in front of it. A click through an overlay that
/// passes every click lands on what is under it, judged on its own.
pub(super) fn lands_allowed(windows: &[ScreenWindow], (x, y): (f64, f64)) -> bool {
    windows
        .iter()
        .filter(|w| !w.allowed && w.takes_clicks)
        .all(|w| {
            let r = w.reach();
            !(x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
        })
}

/// Whether the screen is still as `geometry` says the picture was taken: the
/// same size, at the same scale. macOS only, where the driver reads the
/// point off a picture of the screen as it is at the moment of the action;
/// elsewhere the point goes to the driver in the screen's own pixels, the
/// units it was judged in.
pub(super) fn same_screen(geometry: &ScreenGeometry) -> bool {
    #[cfg(target_os = "macos")]
    {
        let Some((screen, scale)) = main_display_now() else {
            return false;
        };
        (screen.width - geometry.width).abs() < 0.5
            && (screen.height - geometry.height).abs() < 0.5
            && (scale - geometry.scale).abs() < 0.01
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = geometry;
        true
    }
}
