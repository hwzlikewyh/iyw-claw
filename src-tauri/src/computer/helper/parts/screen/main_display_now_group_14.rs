// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// macOS: the main display's frame in desktop points, and its pixels to a
/// point as the driver reckons them — its picture's width over its width in
/// points, and 1 where the picture is no wider.
#[cfg(target_os = "macos")]
pub(super) fn main_display_now() -> Option<(Rect, f64)> {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayCopyDisplayMode(display: u32) -> *mut std::ffi::c_void;
        fn CGDisplayModeGetPixelWidth(mode: *mut std::ffi::c_void) -> usize;
        fn CGDisplayModeRelease(mode: *mut std::ffi::c_void);
    }
    let screen = main_display()?;
    // SAFETY: plain queries; the mode is released once, and a display that
    // has gone answers no mode.
    let pixels = unsafe {
        let mode = CGDisplayCopyDisplayMode(CGMainDisplayID());
        if mode.is_null() {
            return None;
        }
        let pixels = CGDisplayModeGetPixelWidth(mode);
        CGDisplayModeRelease(mode);
        pixels
    } as f64;
    let scale = if pixels > screen.width {
        pixels / screen.width
    } else {
        1.0
    };
    Some((screen, scale))
}
