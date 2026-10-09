// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// macOS: mark which of `windows` are minimized, and which belong to a
/// hidden (⌘H) application — neither of which the driver's listing says.
/// There such a window is only off screen, as are the hidden windows
/// applications keep — so iyw-claw, which lists the one and not the other, would
/// list neither. Accessibility tells them apart (see `crate::computer::helper::axwin`), and is
/// asked about those applications alone ([`owners_to_ask`]). Only called while
/// the helper may ask it.
#[cfg(target_os = "macos")]
pub async fn mark_out_of_sight(windows: &mut [RawWindow]) {
    let (listed, maybe) = owners_to_ask(windows);
    if listed.is_empty() && maybe.is_empty() {
        return;
    }
    let said = crate::computer::helper::axwin::window_states(listed, maybe).await;
    settle(windows, &said);
}
