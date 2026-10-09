// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The class `window` was made with; empty when it cannot be read.
pub(super) fn class_of(window: HWND) -> String {
    // A class name is at most 256 characters.
    let mut name = [0u16; 257];
    // SAFETY: a buffer of the length given; the answer is the number of
    // units written without the NUL, 0 for a handle that names no window.
    let len = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
    usize::try_from(len)
        .ok()
        .and_then(|len| name.get(..len))
        .map(String::from_utf16_lossy)
        .unwrap_or_default()
}

/// Whether the compositor hides `window` (see the module note).
pub(super) fn cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: a 4-byte value for the attribute that fills one; a handle that
    // names no window is answered with an error.
    let status = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    status >= 0 && cloaked != 0
}

/// `window`'s frame as the compositor draws it, in physical pixels; `None`
/// when it has none to draw, or has gone.
pub(super) fn frame(window: HWND) -> Option<Rect> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a RECT for the attribute that fills one; a handle that names
    // no window is answered with an error.
    let status = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut rect as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    };
    let frame = Rect {
        x: f64::from(rect.left),
        y: f64::from(rect.top),
        width: f64::from(rect.right) - f64::from(rect.left),
        height: f64::from(rect.bottom) - f64::from(rect.top),
    };
    (status >= 0 && !frame.is_empty()).then_some(frame)
}

/// What a walk turned up: nothing, one (however often it turned up), or
/// several.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Found<T> {
    Nothing,
    One(T),
    Several,
}

pub(super) fn found<T: PartialEq>(mut items: impl Iterator<Item = T>) -> Found<T> {
    let Some(first) = items.next() else {
        return Found::Nothing;
    };
    if items.all(|item| item == first) {
        Found::One(first)
    } else {
        Found::Several
    }
}

/// The window `id` names, as a handle; 0 names none.
pub(super) fn handle(id: u64) -> Option<HWND> {
    usize::try_from(id)
        .ok()
        .filter(|id| *id != 0)
        .map(|id| id as HWND)
}

/// The process owning `window`; `None` when it is no window.
pub(super) fn owner(window: HWND) -> Option<u32> {
    let mut pid = 0;
    // SAFETY: a valid out-pointer; a handle that is no window is answered
    // with 0.
    let thread = unsafe { GetWindowThreadProcessId(window, &mut pid) };
    (thread != 0 && pid != 0).then_some(pid)
}

/// The core windows directly inside `parent` — or, for a null `parent`,
/// standing on their own — front to back.
pub(super) fn core_windows(parent: HWND) -> impl Iterator<Item = HWND> {
    let class: Vec<u16> = CORE_WINDOW_CLASS.encode_utf16().chain(Some(0)).collect();
    let mut after: HWND = ptr::null_mut();
    std::iter::from_fn(move || {
        // SAFETY: a NUL-terminated class name, and a handle the call itself
        // gave (or null); one that has gone since is answered with null.
        let next = unsafe { FindWindowExW(parent, after, class.as_ptr(), ptr::null()) };
        if next.is_null() {
            return None;
        }
        after = next;
        Some(next)
    })
    .take(MAX_CORE_WINDOWS)
}

/// The application user model id `window` says it belongs to: a frame says
/// the one of the application it shows.
pub(super) fn app_user_model_id(window: HWND) -> Option<String> {
    let mut store = ptr::null_mut();
    // SAFETY: a constant interface id, and an out-pointer that holds our one
    // reference on success.
    let status = unsafe { SHGetPropertyStoreForWindow(window, &IID_PROPERTY_STORE, &mut store) };
    if status < 0 || store.is_null() {
        return None;
    }
    let store = Object(store);
    let mut value = PropVariant::empty();
    // SAFETY: a live property store, a key that outlives the call, and an
    // empty value for it to fill — cleared below, whatever it holds.
    let status = unsafe {
        (store.methods::<PropertyStoreMethods>().get_value)(
            store.0,
            &APP_USER_MODEL_ID_KEY,
            &mut value,
        )
    };
    let id = if status >= 0 {
        value.app_user_model_id()
    } else {
        None
    };
    // SAFETY: the value GetValue filled, or left empty; cleared once.
    unsafe { PropVariantClear(&mut value) };
    id
}
