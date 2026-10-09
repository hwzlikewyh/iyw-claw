// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The application `pid` runs, when it is one (see the module note). macOS
/// only: Windows has `windows_owner`, and Linux reads the driver's own list
/// afresh on every call.
#[cfg(target_os = "macos")]
pub fn identify(pid: u32) -> Option<AppIdentity> {
    identify_executable(&executable_path(pid)?)
}
