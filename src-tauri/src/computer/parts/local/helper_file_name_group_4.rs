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

/// 桌面端直接使用当前主程序；独立服务端保留旧安装布局。
/// 正式版本不从 PATH 或用户指定位置寻找可执行文件。
pub fn locate_helper_binary() -> Option<PathBuf> {
    if cfg!(feature = "tauri-runtime") {
        let path = crate::update::runtime::self_exe();
        return (path.is_absolute() && path.is_file()).then_some(path);
    }
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
