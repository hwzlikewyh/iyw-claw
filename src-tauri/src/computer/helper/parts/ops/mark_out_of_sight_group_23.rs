// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// X11: mark which of `windows` the window manager has minimized (iconified),
/// and which are on another of its desktops — neither of which the driver's
/// listing says: to it all of them are only not mapped.
#[cfg(all(target_os = "linux", feature = "computer-helper"))]
pub async fn mark_out_of_sight(windows: &mut [RawWindow]) {
    let open: Vec<u64> = windows
        .iter()
        .filter(|w| !w.on_screen && w.minimized.is_none())
        .map(|w| w.window_id)
        .collect();
    if open.is_empty() {
        return;
    }
    let said =
        tokio::task::spawn_blocking(move || crate::computer::helper::x11win::window_states(&open))
            .await
            .unwrap_or_default();
    for window in windows.iter_mut() {
        if let Some(state) = said.get(&window.window_id) {
            window.minimized = Some(state.minimized);
            if state.elsewhere {
                window.on_current_space = Some(false);
            }
        }
    }
}
