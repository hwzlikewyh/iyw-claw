// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Whether the process drawing inside `frame`, a window of the frame host
/// `host`, is still `pid` — where the frame says: `None` while no core window
/// is inside it (minimized, or in passing), when only that the run is alive
/// can be told.
pub fn frame_holds(frame: u64, host: u32, pid: u32) -> Option<bool> {
    let frame = handle(frame)?;
    let mut inside = core_windows(frame)
        .filter_map(owner)
        .filter(|drawer| *drawer != host)
        .peekable();
    inside.peek()?;
    Some(inside.all(|drawer| drawer == pid))
}

/// What asking for a window back came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// It was minimized, and it was asked to come back.
    Asked,
    /// It is not minimized: there was nothing to do.
    AlreadyShown,
    /// No such window, or not `pid`'s.
    NotTheWindow,
}

/// Put `pid`'s window `window_id` back on the screen if it is minimized —
/// as clicking it on the taskbar would, except that it is not made the
/// active window: the person's keyboard focus stays where it is. `ready` is
/// asked just before the one change is made; what it refuses is not done.
pub fn restore<E>(
    window_id: u64,
    pid: u32,
    ready: impl FnOnce() -> Result<(), E>,
) -> Result<Restore, E> {
    let Some(window) = handle(window_id) else {
        return Ok(Restore::NotTheWindow);
    };
    if owner(window) != Some(pid) {
        return Ok(Restore::NotTheWindow);
    }
    // SAFETY: a handle is a plain value; a stale one answers no.
    if unsafe { IsIconic(window) } == 0 {
        return Ok(Restore::AlreadyShown);
    }
    ready()?;
    // SAFETY: as above. Asynchronous, so a hung application cannot hold the
    // helper: whether the window came back is read afterwards.
    unsafe { ShowWindowAsync(window, SW_SHOWNOACTIVATE) };
    Ok(Restore::Asked)
}

/// One top-level window on the screen, as [`screen_windows`] finds it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenHwnd {
    /// Its handle, as the driver's window id is.
    pub id: u64,
    pub pid: u32,
    /// Its frame as the compositor draws it, in physical pixels — the units
    /// the driver's picture of the screen is in.
    pub frame: Rect,
    pub class: String,
    /// A layered overlay every click passes through.
    pub passes_clicks: bool,
}

/// Every top-level window on the screen: shown, not minimized, and not
/// hidden by the compositor.
pub fn screen_windows() -> Vec<ScreenHwnd> {
    unsafe extern "system" fn take(window: HWND, param: isize) -> BOOL {
        // SAFETY: `param` is the vector below, which outlives the walk; the
        // walk calls back on this thread alone.
        let windows = unsafe { &mut *(param as *mut Vec<HWND>) };
        windows.push(window);
        BOOL::from(windows.len() < MAX_SCREEN_WINDOWS)
    }
    let mut windows: Vec<HWND> = Vec::new();
    // SAFETY: a callback that only adds to the vector `param` points at.
    unsafe { EnumWindows(take, &mut windows as *mut Vec<HWND> as isize) };
    windows
        .into_iter()
        .filter_map(|window| {
            // SAFETY: plain queries of a handle; one that has gone since is
            // answered as no window.
            let shown = unsafe { IsWindowVisible(window) } != 0 && unsafe { IsIconic(window) } == 0;
            // SAFETY: as above.
            let style = unsafe { GetWindowLongW(window, GWL_EXSTYLE) } as u32;
            if !shown || cloaked(window) {
                return None;
            }
            Some(ScreenHwnd {
                id: window as usize as u64,
                pid: owner(window)?,
                frame: frame(window)?,
                class: class_of(window),
                passes_clicks: style & WS_EX_LAYERED != 0 && style & WS_EX_TRANSPARENT != 0,
            })
        })
        .collect()
}
