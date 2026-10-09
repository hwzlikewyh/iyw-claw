// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Where a computer-use service runs.
pub enum ComputerHost {
    /// The desktop app: its own webviews are told, and it has the strip, the
    /// marker and the stop shortcut.
    #[cfg(feature = "tauri-runtime")]
    Desktop(AppHandle),
    /// iyw-claw-server, let share the screen it runs on: its web clients are
    /// told, through `broadcaster`; `emitter` is where it tells of settings.
    Server {
        broadcaster: Arc<WebEventBroadcaster>,
        emitter: EventEmitter,
    },
}
