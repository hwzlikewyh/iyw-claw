// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The identified applications among the owners of `windows` a person could
/// mean — on screen, minimized, hidden with their application, or on another
/// desktop or Space — once each,
/// with the one whose window is frontmost on screen marked active. One
/// process can be several: the frame host is the application in each of its
/// frames. A window nobody can see on this desktop names no application: an
/// application with only such windows has nothing to share, and the one a
/// minimized frame shows is already named by the frame — its own window,
/// standing outside the frame meanwhile, would name it twice.
///
/// On macOS and Windows these are the running applications, each identified
/// by the helper itself (see [`list_windows`]), from the listing — minimized
/// and hidden windows marked — that a window list is made from.
#[cfg(any(test, target_os = "macos", windows))]
pub fn apps_of(windows: Vec<RawWindow>) -> Vec<RawApp> {
    let app_of = |w: &RawWindow| (w.pid, w.app.started_at, w.app.key().map(str::to_string));
    let meant = |w: &RawWindow| {
        w.on_screen
            || w.minimized == Some(true)
            || w.hidden == Some(true)
            || w.on_current_space == Some(false)
    };
    let front = windows
        .iter()
        .filter(|w| w.on_screen)
        .max_by_key(|w| w.z_index.unwrap_or(i64::MIN))
        .map(app_of);
    let mut seen = std::collections::HashSet::new();
    windows
        .into_iter()
        .filter(|w| meant(w) && w.app.key().is_some() && seen.insert(app_of(w)))
        .map(|w| {
            let active = front == Some(app_of(&w));
            RawApp { active, ..w.app }
        })
        .collect()
}
