// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub fn helper_file_name() -> &'static str {
    if cfg!(windows) {
        "iyw-computer-helper.exe"
    } else {
        "iyw-computer-helper"
    }
}

/// The helper that shipped with the running executable: inside
/// [`HELPER_APP`] when iyw-claw runs from an app bundle on macOS, next to it
/// otherwise — the install directory, or `target/<profile>/` in development
/// (the build copies it there). Deliberately no `PATH` lookup: a helper found
/// somewhere else is not the one that shipped. A debug build also honours
/// `IYW_CLAW_COMPUTER_HELPER_BIN`, for running a freshly built helper; a release
/// build ignores it, since the variable can be set for iyw-claw by anything that
/// can set a launch environment.
pub fn locate_helper_binary() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        if let Some(raw) = std::env::var_os("IYW_CLAW_COMPUTER_HELPER_BIN") {
            let path = PathBuf::from(raw);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    let exe = std::env::current_exe().ok()?;
    let candidate = helper_for(&exe, cfg!(target_os = "macos"))?;
    candidate.is_file().then_some(candidate)
}
