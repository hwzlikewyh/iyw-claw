// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// macOS: the main display's frame, in desktop points — the screen the
/// driver's picture is of.
#[cfg(target_os = "macos")]
pub(super) fn main_display() -> Option<Rect> {
    #[repr(C)]
    struct CgRect {
        pub(in crate::computer::helper::screen) x: f64,
        pub(in crate::computer::helper::screen) y: f64,
        pub(in crate::computer::helper::screen) width: f64,
        pub(in crate::computer::helper::screen) height: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> CgRect;
    }
    // SAFETY: plain queries; a display that has gone answers an empty frame.
    let frame = unsafe { CGDisplayBounds(CGMainDisplayID()) };
    let rect = Rect {
        x: frame.x,
        y: frame.y,
        width: frame.width,
        height: frame.height,
    };
    (!rect.is_empty()).then_some(rect)
}

/// Every window on the screen, any layer, as the window server lists it —
/// its number, its owner, its layer and its frame in desktop points — but
/// the ones drawn fully transparent.
#[cfg(target_os = "macos")]
pub(super) fn window_server_windows() -> Vec<(u64, u32, i64, Rect)> {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
    }
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;

    // SAFETY: a plain query; returns a +1 array, or null.
    let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS, 0) };
    if raw.is_null() {
        return Vec::new();
    }
    // SAFETY: the +1 array from above, released by the wrapper.
    let list: CFArray<CFDictionary<CFString, CFType>> =
        unsafe { CFArray::wrap_under_create_rule(raw) };
    let number = |dict: &CFDictionary<CFString, CFType>, key: &'static str| {
        dict.find(CFString::from_static_string(key))
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_f64())
    };
    list.iter()
        .filter_map(|dict| {
            let pid = number(&dict, "kCGWindowOwnerPID")? as u32;
            if number(&dict, "kCGWindowAlpha").is_some_and(|alpha| alpha <= 0.0) {
                return None;
            }
            let layer = number(&dict, "kCGWindowLayer").unwrap_or(0.0) as i64;
            let id = number(&dict, "kCGWindowNumber")? as u64;
            let bounds = dict
                .find(CFString::from_static_string("kCGWindowBounds"))
                .and_then(|v| v.downcast::<CFDictionary>())?;
            // SAFETY: the window server's bounds dictionary has CFString keys
            // and CFNumber values.
            let bounds: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(bounds.as_concrete_TypeRef()) };
            let rect = Rect {
                x: number(&bounds, "X")?,
                y: number(&bounds, "Y")?,
                width: number(&bounds, "Width")?,
                height: number(&bounds, "Height")?,
            };
            (!rect.is_empty()).then_some((id, pid, layer, rect))
        })
        .collect()
}
