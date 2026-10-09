// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The helper's designated requirement, compiled into release builds.
pub const HELPER_REQUIREMENT: Option<&str> = option_env!("IYW_CLAW_COMPUTER_HELPER_REQUIREMENT");

/// This build's Team ID, compiled into release builds with the requirement:
/// the helper is launched only as a Developer ID build of this team.
pub const HELPER_TEAM_ID: Option<&str> = option_env!("IYW_CLAW_COMPUTER_TEAM_ID");

/// The helper's signing identifier (its designated requirement names it too):
/// on macOS, the bundle identifier of the helper app.
pub const HELPER_SIGNING_ID: &str = if cfg!(feature = "tauri-runtime") {
    "app.iywclaw"
} else {
    "app.iywclaw.computer-helper"
};

/// 独立服务端仍使用此 app 布局；桌面端直接使用主应用。
pub const HELPER_APP: &str = "iyw-computer-helper.app";
