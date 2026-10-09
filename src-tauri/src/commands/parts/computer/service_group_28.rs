// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The computer service the desktop app manages, for its commands.
#[cfg(feature = "tauri-runtime")]
pub(super) fn service(app: &AppHandle) -> Result<Arc<ComputerService>, AppCommandError> {
    app.try_state::<Arc<ComputerService>>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| AppCommandError::configuration_invalid("computer use is not initialised"))
}
