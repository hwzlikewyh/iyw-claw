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
pub const HELPER_SIGNING_ID: &str = "app.iywclaw.computer-helper";

/// The helper's own app inside iyw-claw's on macOS, in `Contents/Helpers/`. macOS
/// charges an executable's permissions to the app bundle it sits in, so a
/// helper beside iyw-claw in `Contents/MacOS/` would hold iyw-claw's — every
/// agent's shell's — and none of its own. In an app of its own it is a
/// principal of its own for Accessibility; for Screen Recording only once it
/// is out of iyw-claw's bundle, which is why iyw-claw runs a copy of this app
/// (`helper_app`).
pub const HELPER_APP: &str = "iyw-computer-helper.app";
