// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// What the desktop app adds to computer use: the stop shortcut, held with
/// the OS while computer use is on; the strip above every window while
/// anything is shared; and the mark an action leaves where it landed.
/// iyw-claw-server has none of them — its Stop is in its web clients' panel.
#[cfg(feature = "tauri-runtime")]
pub(super) struct DesktopUi {
    pub(in crate::commands::computer) app: AppHandle,
    pub(in crate::commands::computer) stop_key: StopKey,
    pub(in crate::commands::computer) indicator: Indicator,
    pub(in crate::commands::computer) marker: Marker,
}
